"""Sequential, redacted and idempotent streaming of a growing log file into a
step's event history.

Reads by byte offset and never jumps to the tail, so no output is silently
skipped. Cuts at line boundaries and never inside a secret, so redaction works
even when a value would straddle two events. While the backend is unreachable
chunks wait in a bounded local queue; resends carry the same offset and the
backend de-duplicates them. Every byte that could not be delivered is counted
and reported as a gap, never presented as complete output.
"""
import re

CHUNK = 3500            # bytes per event, below the backend's 4096-byte message limit
SCAN = CHUNK + 8192     # look-ahead used to avoid cutting inside a secret or key block
MAX_PENDING = 512 * 1024
MAX_PEM_WAIT = 64 * 1024
SENDS_PER_POLL = 8

CREDENTIAL_PATTERNS = [
    (re.compile(r'(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----'), '[REDACTED PRIVATE KEY]'),
    (re.compile(r'(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*'), '[REDACTED PRIVATE KEY]'),
    (re.compile(r'(?i)\b(authorization|proxy-authorization|cookie|set-cookie|x-api-key)(\s*[:=]\s*)[^\r\n"]+'), r'\1\2[REDACTED]'),
    (re.compile(r'(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}'), 'Bearer [REDACTED]'),
    (re.compile(r'([a-zA-Z][a-zA-Z0-9+.-]*://)[^/\s:@]+:[^/\s@]+@'), r'\1[REDACTED]@'),
    (re.compile(r'\b(nmr_|nm_live_|ghp_|gho_|ghu_|ghs_|ghr_|github_pat_|sk-ant-|sk-|xox[abprs]-)[A-Za-z0-9_-]{16,}'), '[REDACTED]'),
    (re.compile(r'\bAKIA[0-9A-Z]{16}\b'), '[REDACTED]'),
]
_BYTE_PATTERNS = [re.compile(p.pattern.encode(), p.flags & ~re.UNICODE) for p, _ in CREDENTIAL_PATTERNS[2:]]
_PEM_BEGIN = re.compile(rb'-----BEGIN [A-Z ]*PRIVATE KEY-----')
_PEM_END = re.compile(rb'-----END [A-Z ]*PRIVATE KEY-----')


def redact_patterns(text):
    for pattern, replacement in CREDENTIAL_PATTERNS:
        text = pattern.sub(replacement, text)
    return text


def protected_spans(data, secrets):
    """Byte ranges a cut must not fall inside. An unterminated key block extends
    to the end of the data: wait for its end before emitting any of it."""
    spans = []
    for secret in secrets:
        start = data.find(secret)
        while start != -1:
            spans.append((start, start + len(secret)))
            start = data.find(secret, start + 1)
    for pattern in _BYTE_PATTERNS:
        spans.extend(m.span() for m in pattern.finditer(data))
    for begin in _PEM_BEGIN.finditer(data):
        end = _PEM_END.search(data, begin.end())
        spans.append((begin.start(), end.end() if end else len(data) + 1))
    return spans


def cut_point(data, final, secrets=()):
    """How many leading bytes of `data` can be emitted now (0 = wait for more)."""
    if final and len(data) <= CHUNK:
        point = len(data)
    elif len(data) <= CHUNK:
        point = data.rfind(b'\n') + 1
    else:
        point = data.rfind(b'\n', 0, CHUNK) + 1 or CHUNK
    for start, end in sorted(protected_spans(data[:SCAN], secrets)):
        if start < point < end:
            if start > 0:
                point = start
            elif end <= len(data):
                point = end
            elif final or len(data) >= MAX_PEM_WAIT:
                point = len(data)
            else:
                return 0
    while 0 < point < len(data) and data[point] & 0xC0 == 0x80:
        point -= 1  # never split a UTF-8 character
    return point


class LogStream:
    """`send(kind, message, fields)` returns 'ok', 'retry' (transient: keep and
    resend later) or 'quota' (budget exhausted: stop). Lease and contract
    errors propagate and stop the step, as they must."""

    def __init__(self, send, path, redact, secrets=(), source='runner', stream='combined', kind='log.chunk'):
        self.send, self.path, self.redact = send, path, redact
        self.secrets = [s.encode() for s in secrets if len(s) >= 4]
        self.source, self.stream, self.kind = source, stream, kind
        self.read_offset = 0
        self.buffer, self.buffer_offset = b'', 0
        self.pending, self.pending_bytes = [], 0
        self.sent = self.gap_bytes = self.unreported_gap = 0
        self.truncated = False

    def _read(self):
        try:
            with open(self.path, 'rb') as log:
                log.seek(self.read_offset)
                data = log.read(MAX_PENDING)
        except FileNotFoundError:
            return False
        self.read_offset += len(data)
        self.buffer += data
        return len(data) == MAX_PENDING

    def _cut(self, final):
        while self.buffer:
            point = cut_point(self.buffer, final, self.secrets)
            if not point:
                return
            raw, self.buffer = self.buffer[:point], self.buffer[point:]
            offset, self.buffer_offset = self.buffer_offset, self.buffer_offset + point
            if self.truncated:
                self.gap_bytes += len(raw)
                continue
            text = redact_patterns(self.redact(raw.decode(errors='replace')))
            self.pending.append((offset, text, len(raw)))
            self.pending_bytes += len(raw)
        while self.pending_bytes > MAX_PENDING:
            _, _, size = self.pending.pop(0)
            self.pending_bytes -= size
            self.gap_bytes += size
            self.unreported_gap += size

    def _flush(self, limit):
        sends = 0
        while self.pending and (limit is None or sends < limit):
            offset, text, size = self.pending[0]
            fields = {'offset': offset, 'stream': self.stream, 'source': self.source}
            if self.unreported_gap:
                fields['data'] = {'gap_bytes': self.unreported_gap}
            outcome = self.send(self.kind, text, fields)
            sends += 1
            if outcome == 'retry':
                return False
            self.pending.pop(0)
            self.pending_bytes -= size
            if outcome == 'quota':
                self.truncated = True
                self.gap_bytes += size + self.pending_bytes
                self.pending, self.pending_bytes = [], 0
                return True
            self.sent += 1
            self.unreported_gap = 0
        return True

    def poll(self):
        self._read()
        self._cut(False)
        self._flush(SENDS_PER_POLL)

    def finish(self):
        """Deliver everything left. Bytes that still cannot be delivered are gaps."""
        delivering = True
        while True:
            more = self._read()
            self._cut(not more)
            # One failed delivery is enough: an unreachable backend must not hold
            # the step through a retry per remaining chunk.
            delivering = delivering and self._flush(None)
            if not more:
                break
        self.gap_bytes += self.pending_bytes
        self.pending, self.pending_bytes = [], 0
        return self.stats()

    def stats(self):
        return {'bytes': self.read_offset, 'events': self.sent, 'gap_bytes': self.gap_bytes,
                'truncated': self.truncated}
