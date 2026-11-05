import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest
import urllib.request
from unittest.mock import patch

import model_relay
from container_entry import parse_pi
from runner import API


class ModelRelayTests(unittest.TestCase):
    def test_real_loopback_stream_never_exposes_runner_or_lease_to_the_sandbox(self):
        received = []
        payload = b'event: response.completed\ndata: {"type":"response.completed","response":{"id":"fixture"}}\n\n'

        class Backend(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_POST(self):
                received.append((self.path, self.headers['Authorization'], json.loads(self.rfile.read(int(self.headers['Content-Length'])))))
                self.send_response(200)
                self.end_headers()
                self.wfile.write(payload)

        backend = http.server.HTTPServer(('127.0.0.1', 0), Backend)
        threading.Thread(target=backend.serve_forever, daemon=True).start()
        try:
            with tempfile.TemporaryDirectory() as temporary:
                local = model_relay.serve_local(temporary)
                api = API('http://127.0.0.1:' + str(backend.server_port), 'host-only-runner-credential')
                job = {'run_id': 'run', 'step': {'id': 'review'}, 'lease_token': 'host-only-fence'}
                try:
                    with model_relay.Relay(temporary, api, job):
                        call = urllib.request.Request('http://127.0.0.1:' + str(local.server_port) + '/codex/responses', data=json.dumps({'model': 'gpt-fixture', 'stream': True, 'input': []}).encode())
                        with urllib.request.urlopen(call, timeout=10) as response:
                            self.assertEqual(response.read(), payload)
                    self.assertEqual([p.name for p in Path(temporary).iterdir()], ['model.sock'])
                    self.assertTrue((Path(temporary) / 'model.sock').is_socket())
                    self.assertEqual(received[0][0], '/v1/factory/runner/runs/run/steps/review/model')
                    self.assertEqual(received[0][1], 'Bearer host-only-runner-credential')
                    self.assertEqual(received[0][2]['lease_token'], 'host-only-fence')
                finally:
                    local.shutdown()
                    local.server_close()
        finally:
            backend.shutdown()
            backend.server_close()

    @unittest.skipUnless(os.environ.get('FACTORY_PI_BIN') or os.environ.get('FACTORY_MANAGED_IMAGE'), 'Requires the pinned Pi CLI or SDK image; no live model is used')
    def test_real_pi_cli_completes_through_offline_ipc_with_a_fixture_model(self):
        item = {'id': 'msg_fixture', 'type': 'message', 'role': 'assistant', 'status': 'completed', 'content': [{'type': 'output_text', 'text': '{"success":true}', 'annotations': []}]}
        events = [
            {'type': 'response.created', 'response': {'id': 'response_fixture', 'status': 'in_progress', 'output': []}},
            {'type': 'response.output_item.added', 'output_index': 0, 'item': dict(item, status='in_progress', content=[])},
            {'type': 'response.content_part.added', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'part': {'type': 'output_text', 'text': '', 'annotations': []}},
            {'type': 'response.output_text.delta', 'item_id': item['id'], 'output_index': 0, 'content_index': 0, 'delta': '{"success":true}'},
            {'type': 'response.output_item.done', 'output_index': 0, 'item': item},
            {'type': 'response.completed', 'response': {'id': 'response_fixture', 'status': 'completed', 'output': [item], 'usage': {'input_tokens': 5, 'output_tokens': 3, 'input_tokens_details': {'cached_tokens': 0}}}},
        ]
        payload = ''.join('event: ' + e['type'] + '\ndata: ' + json.dumps(e) + '\n\n' for e in events).encode()
        class Backend(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_args): pass
            def do_POST(self):
                self.rfile.read(int(self.headers['Content-Length']))
                self.send_response(200);self.send_header('Content-Type', 'text/event-stream');self.end_headers();self.wfile.write(payload)
        backend = http.server.HTTPServer(('127.0.0.1', 0), Backend)
        threading.Thread(target=backend.serve_forever, daemon=True).start()
        try:
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary); ipc = root / 'ipc';ipc.mkdir(); home = root / 'home';home.mkdir()
                local = model_relay.serve_local(ipc)
                (home / 'models.json').write_text(json.dumps(model_relay.pi_configuration('gpt-fixture', local.server_port)))
                (home / 'settings.json').write_text(json.dumps(model_relay.PI_SETTINGS))
                api = API('http://127.0.0.1:' + str(backend.server_port), 'host-only-runner-credential')
                job = {'run_id': 'run', 'step': {'id': 'review'}, 'lease_token': 'host-only-fence'}
                try:
                    if os.environ.get('FACTORY_MANAGED_IMAGE'):
                        import execution, runner, runtime_settings, test_execution
                        fixture = test_execution.ExecutionTests();fixture.setUp()
                        try:
                            image = os.environ['FACTORY_MANAGED_IMAGE']
                            fixture.config.update(allow_backend_models=True, managed_images={'agent': image})
                            fixture.api.base, fixture.api.token, fixture.api.opener = api.base, api.token, api.opener
                            fixture.job['step'].update(kind='review', id='review', adapter='isolated-reviewer', write_repositories=[])
                            profile = {'agents': [{'role': 'reviewer', 'harness': 'pi', 'model': 'gpt-fixture', 'model_connection': 'backend', 'network': 'none', 'image': image}]}
                            adapters, _ = runtime_settings.compile_settings(profile, fixture.config)
                            success, _, outcome = execution.isolated_adapter(fixture.config, fixture.job, adapters['isolated-reviewer'], [fixture.root], fixture.api, runner.git, runner.private_file)
                            self.assertTrue(success, repr(fixture.api.artifacts))
                            self.assertEqual(outcome['output']['billing'], 'subscription_not_invoice')
                            self.assertEqual(outcome['heads'], fixture.job['execution']['heads'])
                            self.assertTrue(all(b'host-only-' not in content for _, content in fixture.api.artifacts))
                            return
                        finally:
                            fixture.tearDown()
                    with model_relay.Relay(ipc, api, job):
                        command = [os.environ['FACTORY_PI_BIN'], '--mode', 'json', '--print', '--no-session', '--no-extensions', '--no-skills', '--no-prompt-templates', '--no-themes', '--no-context-files', '--no-approve', '--no-mcp', '--tools', 'read', '--model', 'workflow/gpt-fixture']
                        result = subprocess.run(command, input=b'Return JSON success.', capture_output=True, cwd=root, timeout=30, env={'PATH': os.defpath, 'HOME': str(home), 'PI_CODING_AGENT_DIR': str(home), 'LANG': 'C.UTF-8'})
                    self.assertEqual(result.returncode, 0, result.stderr.decode())
                    try:
                        parsed = parse_pi(result.stdout)
                    except ValueError:
                        self.fail(result.stdout.decode() + '\n' + result.stderr.decode())
                    self.assertTrue(parsed['success'], result.stdout.decode())
                    self.assertNotIn(b'host-only-', result.stdout)
                finally:
                    local.shutdown();local.server_close()
        finally:
            backend.shutdown();backend.server_close()

    def test_socket_bind_never_overwrites_an_existing_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            private = root / 'private';private.write_text('must-not-be-read')
            (root / 'model.sock').symlink_to(private)
            with self.assertRaises(OSError):
                model_relay.Relay(root, None, {})
            self.assertEqual(private.read_text(), 'must-not-be-read')

    def test_pi_requires_settled_success_and_counts_authoritative_usage_only(self):
        message = {'role': 'assistant', 'model': 'gpt-fixture', 'stopReason': 'stop', 'content': [{'type': 'text', 'text': '{"success":true,"cost_usd":999}'}], 'usage': {'input': 12, 'output': 3}}
        events = [{'type': 'message_end', 'message': message}, {'type': 'agent_end', 'messages': [message]}, {'type': 'agent_settled', 'aborted': False}]
        raw = lambda: '\n'.join(json.dumps(e) for e in events).encode()
        result = parse_pi(raw())
        self.assertEqual(result['usage']['input'], 12)
        self.assertEqual(result['cost_usd'], 0)
        self.assertEqual(result['billing'], 'subscription_not_invoice')
        events[-1]['aborted'] = True
        with self.assertRaises(ValueError):
            parse_pi(raw())
        events.pop()
        with self.assertRaises(ValueError):
            parse_pi(raw())

    def test_managed_pi_needs_local_sdk_authority_and_never_resumes(self):
        import runtime_settings
        image = 'sha256:' + 'a' * 64
        profile = {'agents': [{'role': 'reviewer', 'harness': 'pi', 'model': 'gpt-fixture', 'image': image, 'network': 'none', 'model_connection': 'backend'}]}
        config = {'allow_backend_models': True, 'managed_images': {'agent': image}}
        adapters, _ = runtime_settings.compile_settings(profile, config)
        command = adapters['isolated-reviewer']['command']
        self.assertIn('--no-session', command)
        self.assertEqual(command[command.index('--tools') + 1], 'read,grep,find,ls')
        self.assertNotIn('--resume', command)
        with self.assertRaises(runtime_settings.RuntimeSettingsError):
            runtime_settings.compile_settings(profile, {})

    def test_provisioning_registers_only_the_verified_recipe_image_id(self):
        import hashlib
        import managed_agent
        from types import SimpleNamespace
        digest = hashlib.sha256(Path(managed_agent.__file__).with_name('Dockerfile.agent').read_bytes()).hexdigest()
        image = 'sha256:' + 'b' * 64
        cached = [{'Id': image, 'Config': {'Labels': {'org.nexusmind.sdk-recipe': digest}}}]
        config = {'auto_provision_agent_runtime': True, 'allowed_images': []}
        with patch.object(managed_agent.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout=json.dumps(cached).encode())) as run:
            managed_agent.provision(config)
        self.assertEqual(run.call_count, 1)
        self.assertEqual(config['managed_images']['agent'], image)
        self.assertEqual(config['allowed_images'], [image])


if __name__ == '__main__':
    unittest.main()
