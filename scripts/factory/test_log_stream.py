import json
import os
from pathlib import Path
import tempfile
import time
import unittest
from unittest import mock

import log_stream
from log_stream import CHUNK, LogStream
import infra_incidents


class Sink:
    def __init__(self, outcomes=()):
        self.outcomes = list(outcomes)
        self.events = []
        self.attempts = 0

    def __call__(self, kind, text, fields):
        self.attempts += 1
        outcome = self.outcomes.pop(0) if self.outcomes else 'ok'
        if outcome == 'ok':
            self.events.append((kind, text, dict(fields)))
        return outcome

    def text(self):
        return ''.join(text for _, text, _ in self.events)


class LogStreamTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.path = Path(self.dir.name) / 'runtime.log'
        self.path.write_bytes(b'')

    def tearDown(self):
        self.dir.cleanup()

    def append(self, data):
        with self.path.open('ab') as log:
            log.write(data)

    def test_reads_sequentially_without_skipping_and_cuts_at_lines(self):
        sink = Sink()
        stream = LogStream(sink, self.path, lambda t: t)
        expected = b''
        for batch in range(30):
            chunk = b''.join(b'batch %d line %d \xc3\xa9 ok\n' % (batch, i) for i in range(200))
            expected += chunk
            self.append(chunk)
            stream.poll()
        stats = stream.finish()
        self.assertEqual(sink.text(), expected.decode())
        offsets = [fields['offset'] for _, _, fields in sink.events]
        self.assertEqual(offsets, sorted(offsets))
        self.assertTrue(all(text.endswith('\n') for _, text, _ in sink.events))
        self.assertTrue(all(len(text.encode()) <= CHUNK for _, text, _ in sink.events))
        self.assertEqual(stats['gap_bytes'], 0)
        self.assertEqual(stats['bytes'], len(expected))

    def test_secret_straddling_a_chunk_boundary_is_redacted_whole(self):
        secret = 'Zq9-local-db-password-7Hk2'
        prefix = b'x' * (CHUNK - 10)
        self.append(prefix + secret.encode() + b'y' * 3000)  # one unbroken line
        sink = Sink()
        stream = LogStream(sink, self.path, lambda t: t.replace(secret, '[REDACTED]'), [secret])
        stream.finish()
        joined = sink.text()
        self.assertNotIn(secret[:8], joined)
        self.assertNotIn(secret[-8:], joined)
        self.assertEqual(joined.count('[REDACTED]'), 1)
        for _, text, _ in sink.events:
            self.assertNotIn('Zq9-local', text)

    def test_credential_shapes_and_key_blocks_are_never_split(self):
        key = b'-----BEGIN RSA PRIVATE KEY-----\n' + b'MIIEpAIBAAKCAQEA\n' * 120 + b'-----END RSA PRIVATE KEY-----\n'
        token = b'ghp_' + b'a1B2' * 10
        self.append(b'build ok\n' * 300 + key + b'Authorization: Bearer ' + token + b'\n' + b'done\n')
        sink = Sink()
        stream = LogStream(sink, self.path, lambda t: t)
        stream.poll()  # partial key block must wait for its end
        stream.finish()
        joined = sink.text()
        self.assertNotIn('MIIEpAIBAAKCAQEA', joined)
        self.assertNotIn(token.decode(), joined)
        self.assertIn('[REDACTED PRIVATE KEY]', joined)
        self.assertTrue(joined.endswith('done\n'))

    def test_transient_failures_resend_same_offset_without_duplicates(self):
        self.append(b''.join(b'line %d\n' % i for i in range(2000)))
        sink = Sink(['retry', 'retry', 'ok', 'retry'])
        stream = LogStream(sink, self.path, lambda t: t)
        for _ in range(10):
            stream.poll()
        stream.finish()
        self.assertEqual(sink.text(), ''.join('line %d\n' % i for i in range(2000)))
        self.assertEqual(stream.gap_bytes, 0)

    def test_overflow_while_offline_is_counted_and_reported_as_gap(self):
        with mock.patch.object(log_stream, 'MAX_PENDING', 64 * 1024):
            sink = Sink(['retry'] * 1000)
            stream = LogStream(sink, self.path, lambda t: t)
            for _ in range(10):
                self.append(b'z' * 99 + b'\n' * 1 + b'q' * 899 + b'\n' * 1 + b'w' * 9000 + b'\n')
                stream.poll()
            lost = stream.gap_bytes
            self.assertGreater(lost, 0)
            sink.outcomes = []
            stream.poll()
            self.assertEqual(sink.events[0][2]['data']['gap_bytes'], lost)

    def test_quota_stops_sending_and_counts_the_rest(self):
        self.append(b'a' * 100 + b'\n' + b'b' * 20000 + b'\n')
        sink = Sink(['ok', 'quota'])
        stats = LogStream(sink, self.path, lambda t: t).finish()
        self.assertTrue(stats['truncated'])
        self.assertGreater(stats['gap_bytes'], 19000)
        self.assertEqual(len(sink.events), 1)

    def test_unreachable_backend_does_not_hold_the_step(self):
        self.append(b'line\n' * 100000)
        sink = Sink(['retry'] * 10000)
        stats = LogStream(sink, self.path, lambda t: t).finish()
        self.assertLessEqual(sink.attempts, 2)
        self.assertEqual(stats['gap_bytes'], 500000)


class InfraIncidentTests(unittest.TestCase):
    def test_only_allowlisted_reasons_in_the_window_are_forwarded(self):
        output = json.dumps({'items': [
            {'reason': 'Evicted', 'lastTimestamp': '2026-11-06T22:47:13Z', 'count': 1,
             'involvedObject': {'kind': 'Pod', 'name': 'nexusmind-backend-6f4c94b646-9wfnk'},
             'message': 'The node was low on resource: ephemeral-storage.'},
            {'reason': 'Scheduled', 'lastTimestamp': '2026-11-06T22:47:20Z'},
            {'reason': 'Killing', 'lastTimestamp': '2026-11-05T10:00:00Z'},
        ]})
        import calendar
        start = calendar.timegm((2026, 11, 6, 22, 47, 13, 0, 0, 0)) - 60
        found = infra_incidents.parse(output, start, start + 120)
        self.assertEqual([i['reason'] for i in found], ['Evicted'])
        self.assertEqual(found[0]['object'], 'Pod/nexusmind-backend-6f4c94b646-9wfnk')
        self.assertFalse(found[0]['confirmed_cause'])

    def test_collector_is_disabled_by_default_and_never_raises(self):
        self.assertEqual(infra_incidents.collect(None, 0, 1), [])
        self.assertEqual(infra_incidents.collect({'command': ['/nonexistent/kubectl']}, 0, 1), [])


class CheckoutRetentionTests(unittest.TestCase):
    def test_only_owned_old_finished_checkouts_are_removed(self):
        import runner
        with tempfile.TemporaryDirectory() as state:
            root = Path(state)
            old = time.time() - 10 * 86400
            for run, status, owned, extra in [('done', 'succeeded', True, None), ('blocked', 'blocked', True, None),
                                              ('foreign', 'cancelled', False, None), ('journal', 'succeeded', True, 'pending-completion.json'),
                                              ('unknown', None, True, None)]:
                (root / run).mkdir()
                manifest = root / run / 'checkout-manifest.json'
                manifest.write_text(json.dumps({'run_id': run if owned else 'other'}))
                os.utime(manifest, (old, old))
                if extra:
                    (root / run / extra).write_text('{}')
            api = mock.Mock()
            api.call.return_value = {'runs': [{'id': r, 'status': s} for r, s in
                                              [('done', 'succeeded'), ('blocked', 'blocked'), ('foreign', 'cancelled'), ('journal', 'succeeded')]]}
            self.assertEqual(runner.prune_checkouts({'state_dir': state}, api), ['done'])
            self.assertEqual(sorted(p.name for p in root.iterdir() if p.is_dir()), ['blocked', 'foreign', 'journal', 'unknown'])
            self.assertEqual(runner.prune_checkouts({'state_dir': state}, api), [])  # at most hourly


if __name__ == '__main__':
    unittest.main()
