import importlib.util
import errno
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("runner", Path(__file__).with_name("runner.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.root = self.base / "projects"
        self.repo = self.root / "api"
        self.repo.mkdir(parents=True)
        self.command("init", "-q")
        self.command("config", "user.email", "test@example.com")
        self.command("config", "user.name", "Test")
        (self.repo / "file.txt").write_text("committed")
        self.command("add", "file.txt")
        self.command("commit", "-qm", "initial")

    def tearDown(self):
        self.temp.cleanup()

    def command(self, *args):
        return subprocess.run(["git", *args], cwd=self.repo, check=True, capture_output=True).stdout.decode().strip()

    def test_exception_diagnostics_are_actionable_without_leaking_values(self):
        cases = [
            (OSError('AF_UNIX path too long'), 'model_relay_socket_path_too_long'),
            (OSError(errno.ENOSPC, 'private-secret', '/private-secret'), 'adapter_os_error_ENOSPC'),
            (FileNotFoundError(errno.ENOENT, 'private-secret', '/private-secret'), 'adapter_os_error_ENOENT'),
            (subprocess.TimeoutExpired(['private-secret'], 10, output=b'private-secret'), 'adapter_operation_timed_out'),
            (subprocess.CalledProcessError(1, ['private-secret'], stderr=b'private-secret'), 'adapter_process_failed'),
            (KeyError('private-secret'), 'adapter_configuration_missing'),
        ]
        for error, code in cases:
            with self.subTest(code=code):
                diagnostic = runner.exception_diagnostic(error)
                self.assertEqual(diagnostic['error'], code)
                self.assertTrue(diagnostic['solution'])
                self.assertTrue(diagnostic['hold'])
                self.assertNotIn('private-secret', json.dumps(diagnostic))

    def test_discovery_skips_symlinks_and_does_not_grant_write(self):
        (self.root / "alias").symlink_to(self.repo, target_is_directory=True)
        found = runner.discover(self.root, [self.root])
        self.assertEqual([r["path"] for r in found], ["api"])
        self.assertFalse(found[0]["writable"])
        with self.assertRaises(ValueError):
            runner.discover(self.base, [self.root])

    def test_prepare_uses_committed_code_and_preserves_dirty_clone(self):
        repos = runner.discover(self.root, [self.root])
        (self.repo / "file.txt").write_text("personal changes")
        (self.repo / ".env").write_text("SECRET=private")
        job = {"run_id": "run-1", "snapshot": {"workspace": {"root_path": str(self.root), "repositories": repos}}}
        evidence = runner.prepare(job, [self.root], self.base / "state")
        checkout = self.base / "state" / "run-1" / "api"
        self.assertEqual((checkout / "file.txt").read_text(), "committed")
        self.assertFalse((checkout / ".env").exists())
        self.assertEqual((self.repo / "file.txt").read_text(), "personal changes")
        self.assertEqual(evidence[0]["status"], "pass")
        # Preparation is idempotent after an ambiguous completion/network failure.
        self.assertEqual(runner.prepare(job, [self.root], self.base / "state"), evidence)

    def test_new_runs_fetch_branch_heads_but_retries_keep_the_first_fetch(self):
        self.command('branch', '-M', 'release/accounting')
        self.command('remote', 'add', 'origin', 'https://github.com/acme/api.git')
        old = self.command('rev-parse', 'HEAD')
        repos = runner.discover(self.root, [self.root])
        self.assertEqual(repos[0]['branch'], 'release/accounting')
        upstream = self.base / 'upstream'
        subprocess.run(['git', 'clone', '-q', str(self.repo), str(upstream)], check=True)
        def commit(text):
            (upstream / 'file.txt').write_text(text)
            runner.git(['add', 'file.txt'], upstream)
            runner.git(['-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '-qm', text], upstream)
            return runner.git(['rev-parse', 'HEAD'], upstream)
        first = commit('fixed lockfile')
        remote = self.base / 'remote.git'
        subprocess.run(['git', 'clone', '-q', '--bare', str(upstream), str(remote)], check=True)
        (self.repo / 'file.txt').write_text('personal dirty work')
        (self.repo / '.env').write_text('PRIVATE=must-not-copy')
        job = {'run_id': 'fresh-1', 'snapshot': {'sources_pending': True, 'workspace': {
            'id': 'workspace', 'root_path': str(self.root), 'repositories': repos}}}
        credential = self.base / 'source-read.json'
        credential.write_text(json.dumps({'host': 'github.com', 'username': 'x-access-token', 'token': 'fixture-read-secret'}))
        credential.chmod(0o600)
        config = {'source_credentials': {'workspace': str(credential)}}
        native = runner.git
        fetches = []
        def local_transport(args, cwd, *positional, **kwargs):
            if 'fetch' in args:
                self.assertIn('refs/heads/release/accounting', args)
                self.assertEqual(kwargs.get('credentials'), ('github.com', 'x-access-token', 'fixture-read-secret'))
                self.assertIn('https://github.com/acme/api.git', args)
                fetches.append(list(args))
                # Local bare Git stands in for HTTPS; production forbids file fetch.
                args = [str(remote) if a == 'https://github.com/acme/api.git' else
                        'protocol.file.allow=always' if a == 'protocol.file.allow=never' else a for a in args]
            return native(args, cwd, *positional, **kwargs)
        state = self.base / 'state'
        with patch.object(runner, 'git', side_effect=local_transport):
            evidence = runner.prepare(job, [self.root], state, config)
            self.assertEqual(evidence[0]['commits'], {'api': first})
            second = commit('new upstream commit')
            native(['push', str(remote), 'HEAD:refs/heads/release/accounting'], upstream)
            # Ambiguous completion and an ordinary preparation retry do not refetch.
            self.assertEqual(runner.prepare(job, [self.root], state, config), evidence)
            pinned = json.loads(json.dumps(job))
            pinned['snapshot']['sources_pending'] = False
            pinned['snapshot']['workspace']['repositories'][0]['head_sha'] = first
            self.assertEqual(runner.prepare(pinned, [self.root], state, config), evidence)
            job['run_id'] = 'fresh-2'
            self.assertEqual(runner.prepare(job, [self.root], state, config)[0]['commits'], {'api': second})
        self.assertEqual(len(fetches), 2)
        self.assertNotIn('fixture-read-secret', (state / 'fresh-1' / 'checkout-manifest.json').read_text())
        self.assertNotIn('fixture-read-secret', (state / 'fresh-1' / 'api' / '.git' / 'config').read_text())
        self.assertEqual(self.command('rev-parse', 'HEAD'), old)
        self.assertEqual((self.repo / 'file.txt').read_text(), 'personal dirty work')
        self.assertEqual((state / 'fresh-1' / 'api' / 'file.txt').read_text(), 'fixed lockfile')
        self.assertFalse((state / 'fresh-1' / 'api' / '.env').exists())
        self.assertEqual(self.command('remote', 'get-url', 'origin'), repos[0]['remote'])

    def test_failed_refresh_never_falls_back_to_the_old_commit(self):
        self.command('remote', 'add', 'origin', 'https://github.com/acme/api.git')
        job = {'run_id': 'failed-refresh', 'snapshot': {'sources_pending': True, 'workspace': {
            'id': 'workspace', 'root_path': str(self.root), 'repositories': runner.discover(self.root, [self.root])}}}
        state = self.base / 'state'
        with patch.object(runner, 'fetch_source', side_effect=ValueError('read access unavailable')) as fetch:
            with self.assertRaisesRegex(ValueError, 'read access unavailable'):
                runner.prepare(job, [self.root], state)
            self.assertFalse((state / job['run_id']).exists())
            fetch.reset_mock()
            self.command('remote', 'set-url', 'origin', 'https://github.com/other/repo.git')
            with self.assertRaisesRegex(ValueError, 'Source remote differs'):
                runner.prepare(job, [self.root], state)
            fetch.assert_not_called()
        self.assertEqual((self.repo / 'file.txt').read_text(), 'committed')

    def test_source_fetch_fails_closed_for_unapproved_hosts_credentials_and_branches(self):
        for remote in ('http://github.com/acme/api', 'https://localhost/acme/api',
                       'https://token@github.com/acme/api', 'file:///private/repo',
                       'https://github.com/acme/api?token=secret'):
            with self.subTest(remote=remote), self.assertRaises(ValueError):
                runner.fetch_source({'remote': remote, 'branch': 'master'}, self.repo, {}, 'workspace', [self.root])
        with self.assertRaises(ValueError):
            runner.fetch_source({'remote': 'https://github.com/acme/api', 'branch': '--upload-pack=evil'}, self.repo, {}, 'workspace', [self.root])
        profile = self.base / 'source.json'
        profile.write_text(json.dumps({'host': 'another.example.com', 'username': 'x-access-token', 'token': 'fixture-secret'}))
        profile.chmod(0o600)
        with self.assertRaisesRegex(ValueError, '^Invalid source read credential profile$'):
            runner.fetch_source({'remote': 'https://github.com/acme/api', 'branch': 'master'}, self.repo,
                                {'source_credentials': {'workspace': str(profile)}}, 'workspace', [self.root])

    def test_configuration_must_be_private_and_not_a_symlink(self):
        config = self.base / "config.json"
        config.write_text("{}")
        config.chmod(0o600)
        self.assertEqual(runner.private_file(config), "{}")
        link = self.base / "link.json"
        link.symlink_to(config)
        with self.assertRaises(OSError):
            runner.private_file(link)
        config.chmod(0o644)
        with self.assertRaises(ValueError):
            runner.private_file(config)

    def test_rejects_embedded_credentials_and_remote_http(self):
        self.command("remote", "add", "origin", "https://secret@github.com/acme/api")
        with self.assertRaises(ValueError):
            runner.discover(self.root, [self.root])
        with self.assertRaises(ValueError):
            runner.API("http://example.com", "secret")
        with self.assertRaises(ValueError):
            runner.API("https://example.com/path", "secret")

    def test_manual_environment_references_stay_private_and_outside_repos(self):
        env = self.base / "backend.env"
        env.write_text("PASSWORD=secret")
        env.chmod(0o600)
        config = {"environments": {"workspace": {"backend": str(env)}}}
        self.assertEqual(runner.validate_environment_references(config, "workspace", [self.root]), ["backend"])
        env.chmod(0o644)
        with self.assertRaises(ValueError):
            runner.validate_environment_references(config, "workspace", [self.root])
        local = self.repo / ".env"
        local.write_text("PASSWORD=secret")
        local.chmod(0o600)
        config["environments"]["workspace"]["backend"] = str(local)
        with self.assertRaises(ValueError):
            runner.validate_environment_references(config, "workspace", [self.root])

    def test_repository_code_and_hooks_do_not_execute(self):
        marker = self.base / "hook-executed"
        hook = self.repo / ".git" / "hooks" / "post-checkout"
        hook.write_text("#!/bin/sh\ntouch " + str(marker) + "\n")
        hook.chmod(0o700)
        repos = runner.discover(self.root, [self.root])
        job = {"run_id": "run-2", "snapshot": {"workspace": {"root_path": str(self.root), "repositories": repos}}}
        runner.prepare(job, [self.root], self.base / "state")
        self.assertFalse(marker.exists())


if __name__ == "__main__":
    unittest.main()
