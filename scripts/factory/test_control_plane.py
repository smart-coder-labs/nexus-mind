import http.server
import json
import threading
import unittest
from unittest.mock import Mock

import control_plane
import execution
import runner

TOKEN = 'nmr_private_runner_credential'
RUN = '6f1d2c3b-aaaa-bbbb-cccc-0123456789ab'


class Backend(http.server.BaseHTTPRequestHandler):
    """Answers each request with the next scripted (status, content type, body)."""
    script, seen = [], []

    def respond(self):
        length = int(self.headers.get('Content-Length') or 0)
        if length: self.rfile.read(length)
        type(self).seen.append((self.command, self.path, self.headers.get('X-Request-Id')))
        status, mime, body = type(self).script.pop(0) if type(self).script else (200, 'application/json', b'{"ok":true}')
        self.send_response(status)
        if mime: self.send_header('Content-Type', mime)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    do_GET = do_POST = respond

    def log_message(self, *args):
        pass


class ControlPlaneTests(unittest.TestCase):
    def setUp(self):
        Backend.script, Backend.seen = [], []
        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Backend)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.api = runner.API('http://127.0.0.1:%d' % self.server.server_port, TOKEN)
        self.api.retry_delays = (0, 0, 0)

    def tearDown(self):
        self.server.shutdown(); self.server.server_close()

    def fail_with(self, path, data=None):
        with self.assertRaises(runner.ControlPlaneError) as caught:
            self.api.call(path, data)
        return caught.exception, runner.exception_diagnostic(caught.exception)

    def test_ingress_html_error_is_named_with_status_route_and_request_id(self):
        Backend.script = [(502, 'text/html', b'<html>Bad Gateway ' + RUN.encode() + b'</html>')] * 4
        error, diagnostic = self.fail_with('/runs/' + RUN + '/steps/verify/lease', {'lease_token': 'fence'})
        self.assertEqual(diagnostic['error'], 'adapter_http_502')
        self.assertEqual(diagnostic['request'], 'POST /runs/:run/steps/:step/lease')
        self.assertFalse(diagnostic['response_json'])
        self.assertTrue(diagnostic['retryable'] and diagnostic['hold'])
        self.assertIn('ingress', diagnostic['solution'])
        # The request id in the diagnostic is the one the backend received last.
        self.assertEqual(diagnostic['request_id'], Backend.seen[-1][2])
        self.assertRegex(diagnostic['request_id'], r'^rn-[0-9a-f]{16}$')
        text = json.dumps(diagnostic)
        for secret in (TOKEN, RUN, 'fence', 'Bad Gateway'):
            self.assertNotIn(secret, text)

    def test_only_idempotent_calls_are_retried(self):
        Backend.script = [(503, 'text/plain', b'unavailable')] * 4
        self.fail_with('/runs/' + RUN + '/steps/verify/events', {'kind': 'log.chunk', 'message': 'x'})
        self.assertEqual(len(Backend.seen), 4)
        self.assertEqual(len({request_id for *_, request_id in Backend.seen}), 4)
        for path in ('/claim', '/runs/' + RUN + '/steps/verify/model', '/runs/' + RUN + '/steps/publish/publish',
                     '/runs/' + RUN + '/steps/verify/complete'):
            with self.subTest(path=path):
                Backend.script, Backend.seen = [(503, 'text/plain', b'unavailable')], []
                self.fail_with(path, {})
                self.assertEqual(len(Backend.seen), 1)

    def test_transient_failure_recovers_without_surfacing(self):
        Backend.script = [(504, 'text/html', b'timeout'), (200, 'application/json', b'{"active":true}')]
        self.assertEqual(self.api.call('/runs/' + RUN + '/steps/verify/lease', {'lease_token': 'fence'}), {'active': True})

    def test_contract_errors_are_not_retried_and_keep_the_backend_code(self):
        Backend.script = [(422, 'application/json', b'{"code":"invalid_event","error":"x"}')]
        error, diagnostic = self.fail_with('/runs/' + RUN + '/steps/verify/events', {})
        self.assertEqual(len(Backend.seen), 1)
        self.assertEqual((diagnostic['error'], diagnostic['http_status'], diagnostic['retryable']), ('invalid_event', 422, False))
        self.assertIn('contrato de eventos', diagnostic['solution'])

    def test_unknown_route_from_an_older_backend_names_a_version_mismatch(self):
        for status, mime, body, name in [(404, None, b'', 'adapter_http_404'), (405, None, b'', 'adapter_http_405'),
                                         (404, 'application/json', b'{"code":"route_not_found","error":"x"}', 'route_not_found'),
                                         (426, 'application/json', b'{"code":"runner_version_unsupported","error":"x"}', 'runner_version_unsupported')]:
            with self.subTest(status=status, name=name):
                Backend.script = [(status, mime, body)]
                _, diagnostic = self.fail_with('/claim', {})
                self.assertEqual(diagnostic['error'], name)
                self.assertIn('versiones de contrato', diagnostic['solution'])

    def test_backend_json_5xx_still_holds_and_is_retryable(self):
        Backend.script = [(503, 'application/json', b'{"code":"storage_unavailable","error":"x"}')] * 4
        _, diagnostic = self.fail_with('/runs/' + RUN, None)
        self.assertEqual((diagnostic['error'], diagnostic['hold'], diagnostic['retryable']), ('storage_unavailable', True, True))
        self.assertIn('request_id', diagnostic['solution'])

    def test_unreachable_backend_is_a_network_error(self):
        port = self.server.server_port
        self.tearDown()
        api = runner.API('http://127.0.0.1:%d' % port, TOKEN); api.retry_delays = (0, 0, 0)
        with self.assertRaises(runner.ControlPlaneError) as caught:
            api.call('/workspaces')
        diagnostic = runner.exception_diagnostic(caught.exception)
        self.assertEqual(diagnostic['error'], 'adapter_network_ConnectionRefusedError')
        self.assertIsNone(diagnostic['http_status'])
        self.assertTrue(diagnostic['retryable'])
        self.setUp()

    def test_route_template_hides_identifiers(self):
        self.assertEqual(control_plane.route_template('/runs/' + RUN + '/steps/s1/sources/3/info/refs?service=git-upload-pack'),
                         '/runs/:run/steps/:step/sources/:index/info/refs')
        self.assertEqual(control_plane.route_template('/workspaces/w-1/inventory'), '/workspaces/:workspace/inventory')
        self.assertEqual(control_plane.route_template('/runs'), '/runs')

    def test_runner_announces_its_contract_when_claiming(self):
        api = Mock()
        api.call.side_effect = lambda path, data=None: {'workspaces': []} if path == '/workspaces' else {'job': None} if path == '/claim' else {}
        with unittest.mock.patch.object(runner, 'API', return_value=api), unittest.mock.patch.object(runner, 'reconcile'):
            runner.tick_locked({'api_url': 'http://localhost', 'runner_token': TOKEN, 'state_dir': '/nonexistent'}, [])
        claim = next(call.args[1] for call in api.call.call_args_list if call.args[0] == '/claim')
        self.assertIn('runner-contract:%d' % control_plane.RUNNER_CONTRACT, claim['capabilities'])


class LogEventTests(unittest.TestCase):
    def setUp(self):
        self.job = {'run_id': RUN, 'lease_token': 'fence', 'step': {'id': 'verify'}}
        self.sent = []

    def api(self, *failures):
        failures = list(failures)
        api = Mock()
        def call(path, data=None):
            if failures:
                raise failures.pop(0)
            self.sent.append(data)
            return {'recorded': True}
        api.call.side_effect = call
        return api

    def error(self, status, code=''):
        return control_plane.ControlPlaneError('POST', '/runs/:run/steps/:step/events', 'rn-1', status, code, bool(code))

    def test_outage_and_quota_drop_logs_without_failing_the_step(self):
        api = self.api(self.error(502), self.error(422, 'event_quota'))
        execution.log_event(api, self.job, 'log.chunk', 'first')
        execution.log_event(api, self.job, 'log.chunk', 'second')
        self.assertEqual(self.job['dropped_events'], 2)
        execution.log_event(api, self.job, 'log.chunk', 'third')
        self.assertTrue(self.sent[0]['message'].startswith('[runner: 2 eventos anteriores no se registraron]'))
        self.assertTrue(self.sent[0]['message'].endswith('third'))
        self.assertNotIn('dropped_events', self.job)

    def test_contract_and_lease_errors_still_raise(self):
        for code in ('invalid_event', 'lease_conflict'):
            with self.subTest(code=code):
                with self.assertRaises(control_plane.ControlPlaneError):
                    execution.log_event(self.api(self.error(409 if code == 'lease_conflict' else 422, code)), self.job, 'log.chunk', 'x')

    def test_monitor_survives_a_failed_log_chunk_but_not_a_lost_lease(self):
        import tempfile, pathlib
        with tempfile.TemporaryDirectory() as temp:
            log = pathlib.Path(temp) / 'out.log'
            job = dict(self.job, lease_until=int(__import__('time').time()) + 60, step={'id': 'verify', 'timeout_seconds': 30})
            command = ['python3', '-c', 'import time\nfor i in range(7):\n print(i, flush=True); time.sleep(1)']
            calls = []
            def call(path, data=None):
                calls.append(path.rsplit('/', 1)[-1])
                if path.endswith('/events'):
                    raise self.error(503)
                return {'active': True}
            api = Mock(); api.call.side_effect = call
            self.assertEqual(execution.monitor(command, {}, job, api, 'agent', log), 0)
            self.assertIn('events', calls); self.assertIn('lease', calls)
            def lost(path, data=None):
                if path.endswith('/lease'):
                    raise control_plane.ControlPlaneError('POST', '/runs/:run/steps/:step/lease', 'rn-2', 409, 'lease_conflict', True)
                return {'recorded': True}
            api.call.side_effect = lost
            with self.assertRaises(control_plane.ControlPlaneError):
                execution.monitor(command, {}, job, api, 'agent', log)


if __name__ == '__main__':
    unittest.main()


class CompletionJournalTests(unittest.TestCase):
    def test_large_journal_is_replayed(self):
        import json, os, tempfile
        from pathlib import Path
        import runner
        with tempfile.TemporaryDirectory() as state:
            heads = {'repo-%02d' % i: 'a' * 40 for i in range(49)}
            body = {'lease_token': 't', 'success': False, 'evidence': [{'check': 'build', 'status': 'not_applicable', 'commits': heads} for _ in range(100)],
                    'outcome': {'input_revision': 1, 'heads': heads, 'output': {}, 'cost_usd': 0}}
            journal = Path(state) / 'run' / 'pending-completion.json'
            journal.parent.mkdir()
            fd = os.open(journal, os.O_WRONLY | os.O_CREAT, 0o600)
            with os.fdopen(fd, 'w') as stream:
                json.dump({'run_id': 'run', 'step_id': 'verify', 'attempt': 1, 'body': body}, stream)
            self.assertGreater(journal.stat().st_size, runner.CONFIG_LIMIT)
            api = unittest.mock.Mock()
            api.call.side_effect = lambda path, data=None: {'steps': [{'id': 'verify', 'status': 'running', 'attempt': 1}], 'execution': {'heads': heads}} if path == '/runs/run' else {}
            runner.reconcile({'state_dir': state}, api)
            api.call.assert_any_call('/runs/run/steps/verify/complete', body)
            self.assertFalse(journal.exists())

    def test_config_limit_still_applies(self):
        import os, tempfile
        import runner
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, 'config.json')
            fd = os.open(path, os.O_WRONLY | os.O_CREAT, 0o600)
            with os.fdopen(fd, 'w') as stream:
                stream.write('x' * (runner.CONFIG_LIMIT + 1))
            with self.assertRaises(ValueError):
                runner.private_file(path)
