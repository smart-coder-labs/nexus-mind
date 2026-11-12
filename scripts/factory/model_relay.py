"""Model-only Unix socket bridge: network=none, no credentials in the sandbox."""
from contextlib import closing, contextmanager
import os
import http.client
import http.server
import json
from pathlib import Path
import socket
import socketserver
import subprocess
import threading
import urllib.error
import urllib.request

MAX_REQUEST = 2 * 1024 * 1024
MAX_RESPONSE = 32 * 1024 * 1024
MAX_REQUESTS = 96
PI_SETTINGS = {'transport': 'sse', 'retry': {'enabled': False}, 'cacheWarming': 'off', 'enableInstallTelemetry': False}
ERROR = b'event: error\ndata: {"type":"error","error":{"type":"model_relay_failed","message":"Model relay unavailable; inspect execution readiness"}}\n\n'
BUDGET_ERROR = b'event: error\ndata: {"type":"error","error":{"type":"model_request_budget_exceeded","message":"Model request budget exceeded for this step"}}\n\n'


class BudgetExceeded(Exception):
    pass


def backend_code(error):
    """Only a fixed, known backend code is ever read from an upstream error."""
    try:
        code = json.loads(error.read(4096) or b'{}').get('code')
    except (ValueError, OSError, AttributeError):
        return None
    return code if code == 'model_request_budget_exceeded' else None


def request_body(handler):
    length = int(handler.headers.get('Content-Length', '0'))
    if handler.path != '/codex/responses' or not 0 < length <= MAX_REQUEST:
        raise ValueError('Invalid model request')
    return handler.rfile.read(length)


def headers(handler):
    handler.send_response(200)
    handler.send_header('Content-Type', 'text/event-stream')
    handler.send_header('Cache-Control', 'no-store')
    handler.end_headers()


def forward(source, handler, stop=None):
    total = 0
    while stop is None or not stop.is_set():
        data = source.read1(16384)
        if not data:
            break
        total += len(data)
        if total > MAX_RESPONSE:
            raise ValueError('Model response too large')
        handler.wfile.write(data)
        handler.wfile.flush()


@contextmanager
def unix_address(path):
    path = Path(path)
    if len(os.fsencode(path)) < 104 or not Path('/proc/self/fd').is_dir():
        yield str(path)
        return
    # Linux limits sockaddr_un, not the socket's actual filesystem location.
    # A directory fd keeps the address short without chdir or moving the workspace.
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        yield '/proc/self/fd/' + str(fd) + '/' + path.name
    finally:
        os.close(fd)


class Relay:
    def __init__(self, root, api, job):
        self.stop = threading.Event()
        stop = self.stop
        handled = 0

        class Handler(http.server.BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(5)

            def log_message(self, *_args):
                pass

            def do_POST(self):
                nonlocal handled
                started = False
                try:
                    body = json.loads(request_body(self))
                    handled += 1
                    if handled > MAX_REQUESTS:
                        raise BudgetExceeded()
                    payload = json.dumps({'lease_token': job['lease_token'], 'request': body}).encode()
                    url = api.base + '/v1/factory/runner/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/model'
                    call = urllib.request.Request(url, data=payload, headers={'Authorization': 'Bearer ' + api.token, 'Content-Type': 'application/json'})
                    try:
                        response = api.opener.open(call, timeout=60)
                    except urllib.error.HTTPError as error:
                        if backend_code(error) == 'model_request_budget_exceeded':
                            raise BudgetExceeded() from None
                        raise
                    with response:
                        headers(self);started = True
                        forward(response, self, stop)
                except Exception as error:
                    # Never reflect upstream headers, errors or credentials.
                    if not started:
                        headers(self)
                    self.wfile.write(BUDGET_ERROR if isinstance(error, BudgetExceeded) else ERROR)

        with unix_address(Path(root) / 'model.sock') as address:
            self.server = socketserver.UnixStreamServer(address, Handler)
        self.server.timeout = .1
        self.thread = threading.Thread(target=self.run, daemon=True)

    def run(self):
        # ponytail: one request per sandbox; backend separately caps global concurrency.
        while not self.stop.is_set():
            self.server.handle_request()

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *args):
        self.stop.set()
        self.thread.join(timeout=2)
        self.server.server_close()


class UnixHTTP(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__('localhost', timeout=60)
        self.path = str(path)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(self.timeout)
        with unix_address(self.path) as address:
            self.sock.connect(address)


def serve_local(root):
    """Container loopback HTTP to its one scoped Unix socket; no host TCP access."""
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_POST(self):
            try:
                body = request_body(self)
                encoding = self.headers.get('Content-Encoding', 'identity')
                if encoding == 'zstd':
                    # Node is already in the SDK; bound decompression in its stdlib.
                    script = "process.stdout.write(require('node:zlib').zstdDecompressSync(require('node:fs').readFileSync(0),{maxOutputLength:2097152}))"
                    body = subprocess.run(['node', '-e', script], input=body, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=5, check=True).stdout
                elif encoding != 'identity':
                    raise ValueError('Unsupported model request encoding')
                json.loads(body)
                with closing(UnixHTTP(Path(root) / 'model.sock')) as host:
                    host.request('POST', '/codex/responses', body=body, headers={'Content-Type': 'application/json'})
                    with host.getresponse() as response:
                        headers(self)
                        forward(response, self)
            except (OSError, ValueError, http.client.HTTPException, subprocess.SubprocessError):
                self.close_connection = True

    server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def pi_configuration(model, port):
    # Pi decodes this claim locally. This marker has NO authority, unlike OAuth.
    import base64
    claim = base64.urlsafe_b64encode(b'{"https://api.openai.com/auth":{"chatgpt_account_id":"isolated-workflow"}}').decode().rstrip('=')
    return {'providers': {'workflow': {'baseUrl': 'http://127.0.0.1:' + str(port), 'api': 'openai-codex-responses', 'apiKey': 'e30.' + claim + '.not-a-credential', 'models': [{'id': model, 'reasoning': True, 'contextWindow': 200000, 'maxTokens': 16384, 'compat': {'supportsMaxOutputTokens': False}}]}}}
