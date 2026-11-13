import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import application_environment as app


class ApplicationEnvironmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)
        (self.work / 'api').mkdir()
        self.repos = [{'path': 'api'}]

    def compose(self, text):
        (self.work / 'api' / 'compose.yaml').write_text(text)
        return app.plan(self.work, self.repos)

    def test_discovers_ordered_services_and_disposable_postgres_identity(self):
        result = self.compose('''services:
  api:
    image: python:3.12
    depends_on: [database]
    environment:
      DATABASE_URL: ${DATABASE_URL}
    ports: ["8080:8080"]
  database:
    image: postgres:16
    volumes: ["data:/var/lib/postgresql/data"]
volumes:
  data:
''')
        self.assertEqual(list(result), ['database', 'api'])
        database = result['database']['_env']
        self.assertEqual(database['POSTGRES_USER'], 'nexusmind_test')
        self.assertGreater(len(database['POSTGRES_PASSWORD']), 30)
        self.assertIn('@database:5432/nexusmind_test', result['api']['_env']['DATABASE_URL'])
        self.assertNotEqual(database['POSTGRES_PASSWORD'], app.plan(self.work, self.repos)['database']['_env']['POSTGRES_PASSWORD'])

    def test_never_interpolates_host_secrets_and_reports_only_the_variable_name(self):
        with patch.dict(os.environ, {'API_KEY': 'host-private-key'}):
            with self.assertRaisesRegex(app.EnvironmentDecision, 'Supply test environment variable: API_KEY') as error:
                self.compose('services: {api: {image: "python:3.12", environment: {API_KEY: "${API_KEY}"}}}')
            self.assertNotIn('host-private-key', str(error.exception))
        supplied = app.plan(self.work, self.repos, {'API_KEY': 'explicit-test-value'})
        self.assertEqual(supplied['api']['_env']['API_KEY'], 'explicit-test-value')

    def test_rejects_privileges_host_paths_external_resources_and_yaml_aliases(self):
        cases = [
            'services: {api: {image: "python:3.12", privileged: true}}',
            'services: {api: {image: "python:3.12", network_mode: host}}',
            'services: {api: {image: "python:3.12", env_file: /etc/passwd}}',
            'include: /etc/passwd\nservices: {}',
            'services: {api: {image: "python:3.12", volumes: ["/etc:/data:ro"]}}',
            'services: {api: {image: "python:3.12", volumes: [".:/var/run/docker.sock"]}}',
            'services: {api: {image: "python:3.12"}}\nvolumes: {production: {external: true}}',
            'services: {api: {image: "python:3.12"}}\nnetworks: {production: {external: true}}',
            'services: {api: &app {image: "python:3.12"}, other: *app}',
            'services: {api: {image: "python:3.12", environment: {DATABASE_URL: "postgresql://prod.example.com/db"}}}',
        ]
        for text in cases:
            with self.subTest(text=text), self.assertRaises((app.EnvironmentDecision, FileNotFoundError)):
                self.compose(text)

    def test_bind_mounts_are_candidate_only_and_read_only(self):
        (self.work / 'api' / 'init.sql').write_text('select 1')
        services = self.compose('services: {database: {image: "postgres:16", volumes: ["./init.sql:/docker-entrypoint-initdb.d/init.sql:ro"]}}')
        self.assertEqual(services['database']['_mounts'][0], (str(self.work / 'api' / 'init.sql'), '/docker-entrypoint-initdb.d/init.sql'))
        for value in ['../outside', '/etc/passwd', 'api/file,src=/etc', 'api/\nfile']:
            with self.assertRaises((app.EnvironmentDecision, FileNotFoundError)):
                app.inside(self.work, value)

    def test_build_context_cannot_escape_and_missing_dependencies_are_not_guessed(self):
        for text in [
            'services: {api: {build: {context: /tmp}}}',
            'services: {api: {build: {context: ../../../tmp}}}',
            'services: {api: {image: "python:3.12", depends_on: [external]}}',
            'services: {a: {image: "python:3.12", depends_on: [b]}, b: {image: "python:3.12", depends_on: [a]}}',
        ]:
            with self.subTest(text=text), self.assertRaises((app.EnvironmentDecision, FileNotFoundError)):
                self.compose(text)

    def test_dockerfile_bases_are_resolved_and_frontends_remote_copy_and_privileged_runs_rejected(self):
        image = 'sha256:' + 'a' * 64
        with patch.object(app, 'resolve_image', return_value=(image, {})) as resolve, patch.object(app, 'local_base', return_value=image):
            text = app.dockerfile({}, 'ARG BASE=node:22\nFROM ${BASE} AS builder\nRUN npm ci\nFROM builder AS app\nCOPY --from=builder /app /app\n')
            self.assertIn('FROM ' + image + ' AS builder', text)
            self.assertIn('FROM builder AS app', text)
            resolve.assert_called_once_with({}, 'node:22', None, None)
        for recipe in ['# syntax=evil/frontend\nFROM node:22', '\ufeff# syntax=evil/frontend\nFROM node:22', 'FROM node:22\nADD https://evil/file /file', 'FROM node:22\nRUN --network=host curl localhost', 'FROM node:22\nCOPY --from=external /app /app', 'FROM node:22\nONBUILD RUN command', 'FROM node:22\nRUN <<EOF\ncommand\nEOF']:
            with self.subTest(recipe=recipe), patch.object(app, 'resolve_image', return_value=(image, {})), patch.object(app, 'local_base', return_value=image), self.assertRaises(app.EnvironmentDecision):
                app.dockerfile({}, recipe)

    def test_missing_dockerfile_defaults_are_reported_together_before_image_resolution(self):
        recipe='FROM node:22\nARG APP_CONFIG_APP_IDENTIFIER\nARG NEXT_PUBLIC_API_URL\nARG NEXT_PUBLIC_SITE_URL\nARG NEXT_PUBLIC_API_URL\nARG PORT=3000\n'
        with patch.object(app, 'resolve_image') as resolve:
            with self.assertRaises(app.EnvironmentDecision) as error: app.dockerfile({},recipe)
        self.assertEqual(str(error.exception),'Declare non-secret Dockerfile ARG defaults: APP_CONFIG_APP_IDENTIFIER, NEXT_PUBLIC_API_URL, NEXT_PUBLIC_SITE_URL')
        resolve.assert_not_called()
        with self.assertRaisesRegex(app.EnvironmentDecision,'Invalid Dockerfile ARG declaration'):
            app.dockerfile({},'FROM node:22\nARG bad-name\n')

    def test_external_registry_cannot_trigger_docker_or_credential_helpers(self):
        with patch.object(app.subprocess, 'run') as run:
            for reference in ['evil.example/app:1', 'localhost:5000/app:1', 'user/app:1', 'node', 'node:22 --privileged']:
                with self.assertRaises(app.EnvironmentDecision): app.resolve_image({}, reference)
            run.assert_not_called()

    def test_node_manifest_detection_and_ambiguous_compose_are_explicit(self):
        (self.work / 'api' / 'package.json').write_text('{"scripts":{"start":"node server.js"}}')
        self.assertTrue(app.plan(self.work, self.repos)['api']['_node'])
        (self.work / 'api' / 'compose.yaml').write_text('services: {web: {image: "nginx:alpine"}}')
        (self.work / 'api' / 'docker-compose.yml').write_text('services: {}')
        with self.assertRaisesRegex(app.EnvironmentDecision, 'Choose one Compose'):
            app.plan(self.work, self.repos)
        self.assertEqual(list(app.plan(self.work, self.repos, entrypoints=['api/compose.yaml'])), ['web'])

    def test_builds_require_an_actual_bounded_cgroup_not_cli_resource_hints(self):
        def bounded(path, *args, **kwargs):
            if str(path) == '/proc/self/cgroup': return '0::/user.slice/runner.service\n'
            if path.name == 'memory.max': return str(1024 ** 3)
            if path.name == 'pids.max': return '256'
            raise FileNotFoundError()
        with patch.object(Path, 'read_text', bounded):
            self.assertEqual(app.build_cgroup(), '/user.slice/runner.service')
        with patch.object(Path, 'read_text', return_value='0::/\n'), self.assertRaises(app.EnvironmentDecision):
            app.build_cgroup()

    def test_docker_client_cannot_inherit_personal_proxies_or_host_credentials(self):
        from execution import runtime_environment
        with patch.dict(os.environ, {'NEXUSMIND_API_KEY': 'host-secret', 'HTTP_PROXY': 'http://private-proxy', 'DOCKER_CONFIG': '/private/docker'}):
            env = runtime_environment({'state_dir': str(self.work)}, 'run')
        self.assertNotIn('HTTP_PROXY', env)
        self.assertNotIn('NEXUSMIND_API_KEY', env)
        self.assertEqual(env['DOCKER_CONFIG'], str(self.work / 'run' / '.docker-client'))

    def test_operator_policy_is_required_before_any_build_or_pull(self):
        with patch.object(app.subprocess, 'run') as run, self.assertRaisesRegex(app.EnvironmentDecision, 'authorize isolated application builds'):
            app.prepare({}, {'snapshot': {'workspace': {'id': 'ws'}}}, self.work, None, [], None)
        run.assert_not_called()

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Set FACTORY_TEST_IMAGE for candidate Docker build/bring-up integration')
    def test_real_candidate_dockerfile_build_and_service_are_sha_bound(self):
        from test_execution import ExecutionTests
        import runner
        import delivery
        fixture = ExecutionTests()
        fixture.setUp()
        self.addCleanup(fixture.tearDown)
        # One authorized application, with a base image already available locally.
        fixture.job['snapshot']['workspace']['repositories'] = fixture.job['snapshot']['workspace']['repositories'][:1]
        fixture.job['execution']['heads'] = {r['path']: r['head_sha'] for r in fixture.job['snapshot']['workspace']['repositories']}
        fixture.config.update(allow_application_builds=True, allowed_images=[os.environ['FACTORY_TEST_IMAGE']], managed_images={'agent': os.environ['FACTORY_TEST_IMAGE']})
        repo = Path(fixture.config['state_dir']) / fixture.job['run_id'] / 'api'
        recipe = 'FROM ' + os.environ['FACTORY_TEST_IMAGE'] + '\nWORKDIR /app\nCOPY code.txt /app/code.txt\nENTRYPOINT []\nEXPOSE 8080\nCMD ["python3", "-m", "http.server", "8080"]\n'
        (repo / 'Dockerfile').write_text(recipe)
        (repo / 'code.txt').write_text('candidate\n')
        runner.git(['add', '.'], repo)
        runner.git(['-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '-qm', 'candidate application'], repo)
        fixture.job['execution']['heads']['api'] = runner.git(['rev-parse', 'HEAD'], repo)
        fixture.job['step'] = {'id': 'environment', 'kind': 'check', 'adapter': 'integrated-environment', 'timeout_seconds': 120}
        try:
            success, evidence, _ = delivery.services(fixture.config, fixture.job, [fixture.root], fixture.api, runner.git, runner.private_file)
        except app.EnvironmentDecision:
            self.fail('\n'.join(data.decode() for name, data in fixture.api.artifacts if name.startswith('application-build-')))
        self.assertTrue(success)
        self.assertEqual(evidence[0]['commits'], fixture.job['execution']['heads'])
        reports = [json.loads(data) for name, data in fixture.api.artifacts if name == 'integrated-environment.json']
        self.assertTrue(reports[0]['automatic'])
        self.assertRegex(reports[0]['applications']['api']['image'], '^sha256:')
        import subprocess
        import execution
        network = json.loads(subprocess.check_output(['docker', 'network', 'inspect', execution.network_name(fixture.job['run_id'])]))[0]
        self.assertTrue(network['Internal'])
        self.assertEqual(network['Options']['com.docker.network.bridge.inhibit_ipv4'], 'true')
        bridge = json.loads(subprocess.check_output(['ip', '-j', 'address', 'show', 'br-' + network['Id'][:12]]))
        self.assertFalse(any(address.get('family') == 'inet' for interface in bridge for address in interface['addr_info']))
        fixture.job['step'] = {'id': 'functional', 'kind': 'check', 'adapter': 'qa', 'write_repositories': [], 'timeout_seconds': 60}
        adapter = {'kind': 'checks', 'image': os.environ['FACTORY_TEST_IMAGE'], 'network': 'services', 'checks': [{'name': 'functional', 'command': ['python3', '-c', "import urllib.request; assert urllib.request.urlopen('http://api:8080/code.txt').read()==b'candidate\\n'"]}]}
        self.assertTrue(execution.isolated_adapter(fixture.config, fixture.job, adapter, [fixture.root], fixture.api, runner.git, runner.private_file)[0])
        self.assertEqual((fixture.root / 'api' / 'code.txt').read_text(), 'old\n')

    @unittest.skipUnless(os.environ.get('FACTORY_NODE_TEST_IMAGE'), 'Set FACTORY_NODE_TEST_IMAGE for real offline npm/build/start/test:seed integration')
    def test_real_node_manifest_build_and_declared_test_fixture(self):
        from test_execution import ExecutionTests
        import runner
        import delivery
        import execution
        fixture = ExecutionTests()
        fixture.setUp()
        self.addCleanup(fixture.tearDown)
        fixture.job['snapshot']['workspace']['repositories'] = fixture.job['snapshot']['workspace']['repositories'][:1]
        fixture.job['execution']['heads'] = {r['path']: r['head_sha'] for r in fixture.job['snapshot']['workspace']['repositories']}
        image = os.environ['FACTORY_NODE_TEST_IMAGE']
        fixture.config.update(allow_application_builds=True, allowed_images=[image], managed_images={'agent': image})
        repo = Path(fixture.config['state_dir']) / fixture.job['run_id'] / 'api'
        package = {'name': 'candidate-app', 'version': '1.0.0', 'config': {'port': 8080}, 'scripts': {'start': 'node server.js', 'build': 'node -e "require(\'fs\').writeFileSync(\'built.txt\',\'candidate\')"', 'test:seed': 'node -e "require(\'fs\').writeFileSync(\'/tmp/fixture\',\'seeded\')"'}}
        (repo / 'package.json').write_text(json.dumps(package))
        (repo / 'package-lock.json').write_text(json.dumps({'name': 'candidate-app', 'version': '1.0.0', 'lockfileVersion': 3, 'packages': {'': {'name': 'candidate-app', 'version': '1.0.0'}}}))
        (repo / 'server.js').write_text("const fs=require('fs');require('http').createServer((q,r)=>r.end(fs.readFileSync('built.txt','utf8')+':'+(fs.existsSync('/tmp/fixture')?'seeded':'empty')+':'+String(process.env.NEXUSMIND_API_KEY))).listen(Number(process.env.PORT),'0.0.0.0')")
        runner.git(['add', '.'], repo)
        runner.git(['-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '-qm', 'candidate application'], repo)
        fixture.job['execution']['heads']['api'] = runner.git(['rev-parse', 'HEAD'], repo)
        fixture.job['step'] = {'id': 'environment', 'kind': 'check', 'adapter': 'integrated-environment', 'timeout_seconds': 120}
        try:
            self.assertTrue(delivery.services(fixture.config, fixture.job, [fixture.root], fixture.api, runner.git, runner.private_file)[0])
        except Exception:
            self.fail('\n'.join(data.decode() for name, data in fixture.api.artifacts if name.endswith('.log')))
        fixture.job['step'] = {'id': 'functional', 'kind': 'check', 'adapter': 'qa', 'write_repositories': [], 'timeout_seconds': 60}
        adapter = {'kind': 'checks', 'image': image, 'network': 'services', 'checks': [{'name': 'functional', 'command': ['python3', '-c', "import urllib.request; assert urllib.request.urlopen('http://api:8080').read()==b'candidate:seeded:undefined'"]}]}
        self.assertTrue(execution.isolated_adapter(fixture.config, fixture.job, adapter, [fixture.root], fixture.api, runner.git, runner.private_file)[0])
        self.assertTrue(any(name.startswith('test-seed-') for name, _ in fixture.api.artifacts))


if __name__ == '__main__': unittest.main()
