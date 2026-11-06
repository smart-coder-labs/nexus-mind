import importlib.util
import errno
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

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
