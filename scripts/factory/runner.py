#!/usr/bin/env python3
"""NexusMind workspace runner: discovery, isolated agents/QA and durable delivery.

Repository code runs only in configured container/VM adapters, never on the host.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

SKIP = {".git", "node_modules", "vendor", "target", ".worktrees", ".next", ".cache"}


def private_file(path):
    """Read a private, non-symlink regular file without following its final link."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ValueError("Configuration must be an owned regular file with mode 0600")
        if info.st_size > 65536:
            raise ValueError("Configuration file too large")
        with os.fdopen(fd, "r", closefd=False) as stream:
            return stream.read()
    finally:
        os.close(fd)


def inside(path, roots):
    resolved = Path(path).resolve(strict=True)
    if not resolved.is_dir() or not any(resolved == root or root in resolved.parents for root in roots):
        raise ValueError("Path outside locally allowed roots")
    return resolved


def git(args, cwd, timeout=60, binary=False, input_data=None, allowed_codes=(0,)):
    # Never inherit GIT_CONFIG, SSH commands, access tokens, hooks or user filters.
    # Empty HOME prevents reading user git configuration; no credential is needed
    # for copying an already-present local repository.
    with tempfile.TemporaryDirectory(prefix="nexusmind-git-home-") as home:
        env = {"PATH": os.defpath, "HOME": home, "LANG": "C", "GIT_CONFIG_NOSYSTEM": "1",
               "GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_GLOBAL": os.devnull}
        result = subprocess.run(
            ["git", "-c", "core.hooksPath=" + os.devnull, "-c", "protocol.ext.allow=never",
             "-c", "protocol.file.allow=always", *args],
            cwd=cwd, env=env, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, timeout=timeout, check=False,
            **({'stdin': subprocess.DEVNULL} if input_data is None else {'input': input_data}),
        )
        if result.returncode not in allowed_codes:
            # Raw stderr may contain remotes/tokens. Keep it out of API/log output.
            raise ValueError("Git operation failed; inspect the repository locally")
        return result.stdout if binary else result.stdout.decode("utf-8").strip()


def discover(root, roots):
    root = inside(root, roots)
    repositories = []
    visited = 0
    for current, dirs, files in os.walk(root, followlinks=False):
        visited += 1
        if visited > 20000:
            raise ValueError("Discovery directory limit exceeded")
        here = Path(current)
        dirs[:] = sorted(d for d in dirs if d not in SKIP and not (here / d).is_symlink())
        marker = here / ".git"
        if marker.exists():
            if marker.is_symlink() or not marker.is_dir():
                # Worktrees can point at Git metadata outside the allowed tree.
                # Register the original clone, not an existing personal worktree.
                dirs[:] = []
                continue
            if here == root:
                raise ValueError("Choose a parent folder containing repositories, not a repository itself")
            commit = git(["rev-parse", "--verify", "HEAD^{commit}"], here)
            if len(commit) not in (40, 64) or any(c not in "0123456789abcdef" for c in commit):
                raise ValueError("Invalid committed revision")
            # Reject credential-bearing remotes; never report them to NexusMind.
            try:
                remote = git(["config", "--get", "remote.origin.url"], here)
            except ValueError:
                remote = ""
            if ("://" in remote and any(c in remote.split("://", 1)[1] for c in "@?#")) or any(ord(c) < 32 for c in remote):
                raise ValueError("Remove embedded credentials from Git remotes before discovery")
            repositories.append({"path": here.relative_to(root).as_posix(), "remote": remote,
                                 "head_sha": commit, "writable": False})
            dirs[:] = []
            if len(repositories) > 100:
                raise ValueError("Repository limit exceeded")
    return sorted(repositories, key=lambda r: r["path"])


def validate_environment_references(config, workspace_id, roots):
    """Validate local manual .env references without copying/publishing values.

    References are per workspace and per service, configured locally. A future
    isolated runtime adapter must request injection separately; preparation does
    not expose them to an agent or create a repository .env file.
    """
    refs = config.get("environments", {}).get(workspace_id, {})
    if not isinstance(refs, dict) or len(refs) > 32:
        raise ValueError("Invalid environment reference map")
    for service, filename in refs.items():
        if not safe_id(service):
            raise ValueError("Invalid environment service name")
        path = Path(filename)
        if not path.is_absolute() or path.is_symlink():
            raise ValueError("Environment references must be absolute non-symlink files")
        canonical = path.resolve(strict=True)
        if any(canonical == root or root in canonical.parents for root in roots):
            raise ValueError("Environment files must live outside source roots")
        private_file(path)  # validate owner/mode/type/size; never store or log contents
    return sorted(refs)


def safe_id(value):
    return isinstance(value, str) and 0 < len(value) <= 100 and all(c.isascii() and (c.isalnum() or c in "-_") for c in value)


def prepare(job, roots, state_dir):
    """Copy pinned committed code, not dirty files; never modify original clones."""
    if not safe_id(job["run_id"]):
        raise ValueError("Invalid run id")
    workspace = job["snapshot"]["workspace"]
    root = inside(workspace["root_path"], roots)
    state_dir = Path(state_dir).resolve()
    # Run output must not appear inside source roots/discovery.
    if any(state_dir == r or r in state_dir.parents for r in roots):
        raise ValueError("Runner state directory must be outside workspace roots")
    state_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    mode = state_dir.stat()
    if mode.st_uid != os.getuid() or mode.st_mode & 0o077:
        raise ValueError("Runner state directory must be private (0700)")
    target = state_dir / job["run_id"]
    if target.exists() or target.is_symlink():
        manifest_path = target / 'checkout-manifest.json'
        if target.is_symlink() or not manifest_path.is_file():
            raise ValueError('Run checkout needs reconciliation')
        data = manifest_path.read_bytes()
        expected = {r['path']: r['head_sha'] for r in workspace['repositories']}
        if json.loads(data).get('commits') != expected or any(git(['rev-parse', 'HEAD'], target / path) != sha for path, sha in expected.items()):
            raise ValueError('Existing checkout differs from the pinned preparation')
        return [{'check': 'committed-checkouts-prepared', 'status': 'pass', 'commits': expected, 'artifact_digest': hashlib.sha256(data).hexdigest()}]
    stage = Path(tempfile.mkdtemp(prefix="prepare-", dir=state_dir))
    commits = {}
    try:
        for repo in workspace["repositories"]:
            relative = Path(repo["path"])
            if relative.is_absolute() or ".." in relative.parts or not relative.parts:
                raise ValueError("Unsafe repository path")
            source = inside(root / relative, [root])
            if any(p.is_symlink() for p in [root / Path(*relative.parts[:i]) for i in range(1, len(relative.parts) + 1)]):
                raise ValueError("Symlink repository paths are not allowed")
            if not (source / ".git").is_dir() or (source / ".git").is_symlink():
                raise ValueError("Expected original repository clone")
            sha = repo["head_sha"]
            if len(sha) not in (40, 64) or any(c not in "0123456789abcdef" for c in sha):
                raise ValueError("Invalid commit")
            destination = stage / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            git(["clone", "--no-local", "--no-checkout", "--", str(source), str(destination)], stage, 300)
            git(["checkout", "--detach", sha, "--"], destination, 300)
            # Avoid publishing back into the user's source clone by accident.
            git(["remote", "remove", "origin"], destination)
            actual = git(["rev-parse", "HEAD"], destination)
            if actual != sha:
                raise ValueError("Prepared commit mismatch")
            commits[repo["path"]] = actual
        manifest = {"run_id": job["run_id"], "commits": commits, "isolation": "checkout-only-not-a-sandbox"}
        encoded = json.dumps(manifest, sort_keys=True).encode()
        (stage / "checkout-manifest.json").write_bytes(encoded)
        stage.rename(target)
        return [{"check": "committed-checkouts-prepared", "status": "pass", "commits": commits,
                 "artifact_digest": hashlib.sha256(encoded).hexdigest()}]
    except BaseException:
        shutil.rmtree(stage)
        raise


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError("Runner API redirects are not allowed")


class API:
    def __init__(self, base_url, token):
        parsed = urllib.parse.urlsplit(base_url)
        if parsed.username or parsed.password or parsed.query or parsed.fragment or parsed.path not in ("", "/"):
            raise ValueError("API URL must be an origin without credentials")
        if parsed.scheme != "https" and not (parsed.scheme == "http" and parsed.hostname in ("localhost", "127.0.0.1", "::1")):
            raise ValueError("Runner credentials require HTTPS (except loopback)")
        self.base = base_url.rstrip("/")
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect)

    def call(self, path, data=None, timeout=30):
        request = urllib.request.Request(self.base + "/v1/factory/runner" + path,
            data=None if data is None else json.dumps(data).encode(),
            headers={"Authorization": "Bearer " + self.token, "Content-Type": "application/json"})
        with self.opener.open(request, timeout=timeout) as response:
            return json.load(response)


def reconcile(config, api):
    from execution import cleanup
    state = Path(config['state_dir'])
    for journal in state.glob('*/pending-completion.json'):
        saved = json.loads(private_file(journal))
        run = api.call('/runs/' + saved['run_id'])
        step = next(s for s in run['steps'] if s['id'] == saved['step_id'])
        if step['status'] == 'running' and step['attempt'] == saved['attempt']:
            api.call('/runs/' + saved['run_id'] + '/steps/' + saved['step_id'] + '/complete', saved['body'])
        elif run['execution']['heads'] != saved['body']['outcome']['heads']:
            # A cancelled/expired write was not adopted by the control plane.
            # Reset only disposable clones to the authoritative server heads.
            for path, sha in run['execution']['heads'].items():
                repo = state / saved['run_id'] / path
                git(['reset', '--hard', sha], repo); git(['clean', '-fdx'], repo)
        journal.unlink()
    if any(state.glob('*/services.json')):
        active = {r['id']: r['status'] for r in api.call('/runs')['runs']}
        for manifest in state.glob('*/services.json'):
            if active.get(manifest.parent.name) not in ('running', 'queued'):
                cleanup(config, manifest.parent.name, state)


def tick_locked(config, roots):
    from execution import isolated_adapter, cleanup, upload
    from delivery import services, validate_services, gate, publish
    for manifest in Path(config['state_dir']).glob('*/services.json'):
        if time.time()-json.loads(manifest.read_text()).get('created_at',0)>7200:
            cleanup(config,manifest.parent.name,config['state_dir'])
    api = API(config['api_url'], config['runner_token'])
    reconcile(config, api)
    # Polling has its own persisted due times and scoped connectors; no intake
    # credential is ever downloaded to the runner.
    api.call('/poll-intakes', {})
    for workspace in api.call('/workspaces')['workspaces']:
        repos = discover(workspace['root_path'], roots)
        if repos:
            api.call('/workspaces/' + workspace['id'] + '/inventory', {'repositories': repos})
    adapters = config.get('adapters', {})
    capabilities = ['adapter:prepare-workspace', 'workspace:prepare', 'adapter:evidence-gate', 'adapter:policy-publisher']
    if config.get('workspace_profiles'):
        capabilities += ['adapter:integrated-environment']
    for name in adapters:
        if not safe_id(name): raise ValueError('Invalid adapter name')
        capabilities.append('adapter:' + name)
    capabilities += config.get('capabilities', [])
    job = api.call('/claim', {'capabilities': capabilities})['job']
    if not job:
        return
    evidence, success = [], False
    outcome = {'input_revision': job['execution']['revision'], 'heads': job['execution']['heads'], 'output': {}, 'cost_usd': 0}
    try:
        adapter = job['step']['adapter']
        if adapter == 'prepare-workspace' and job['step']['kind'] == 'prepare':
            validate_environment_references(config, job['snapshot']['workspace']['id'], roots)
            evidence = prepare(job, roots, config['state_dir'])
            data = (Path(config['state_dir']) / job['run_id'] / 'checkout-manifest.json').read_bytes()
            artifact = upload(api, job, 'checkout-manifest.json', 'application/json', data)
            evidence[0]['artifact_digest'] = artifact['digest']
            success = True
        elif adapter == 'integrated-environment':
            success, evidence, outcome = services(config, job, roots, api, git, private_file)
        elif adapter == 'evidence-gate':
            success, evidence, outcome = gate(job, api)
        elif adapter == 'policy-publisher':
            success, evidence, outcome = publish(config, job, api, git)
        else:
            profile = adapters[adapter]
            if job['step']['write_repositories']:
                cleanup(config, job['run_id'], config['state_dir'])
            if profile.get('network') == 'services': validate_services(config, job)
            success, evidence, outcome = isolated_adapter(config, job, profile, roots, api, git, private_file)
            if profile.get('network') == 'services': validate_services(config, job)
    except (ValueError, OSError, subprocess.SubprocessError, KeyError) as error:
        success, evidence = False, []
        if outcome['heads'] != job['execution']['heads']:
            for path, sha in job['execution']['heads'].items():
                repo = Path(config['state_dir']) / job['run_id'] / path
                git(['reset', '--hard', sha], repo); git(['clean', '-fdx'], repo)
            outcome['heads'] = job['execution']['heads']
        # Avoid exception strings from subprocess/network libraries, which can
        # include private arguments. Our own validation messages contain no values.
        message = str(error) if isinstance(error, ValueError) else 'isolated_adapter_failed'
        outcome['output'] = {'error': message[:1000], 'hold': True}
        if isinstance(error, urllib.error.HTTPError):
            try:
                code = json.load(error).get('code', '')
                outcome['output'] = {'error': code, 'hold': code in ('policy_approval_required', 'policy_never')}
            except (ValueError, OSError): pass
        print('Workflow step blocked; inspect its events and artifacts', flush=True)
    body = {'lease_token': job['lease_token'], 'success': success, 'evidence': evidence, 'outcome': outcome}
    journal = Path(config['state_dir']) / job['run_id'] / 'pending-completion.json'
    journal.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(journal, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as stream:
        json.dump({'run_id': job['run_id'], 'step_id': job['step']['id'], 'attempt': job['attempt'], 'body': body}, stream)
        stream.flush(); os.fsync(stream.fileno())
    response = api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/complete', body)
    journal.unlink()
    if response.get('status') in ('succeeded', 'cancelled', 'blocked'):
        cleanup(config, job['run_id'], config['state_dir'])


def tick(config):
    import fcntl
    roots = [Path(p).resolve(strict=True) for p in config['allowed_roots']]
    state = Path(config['state_dir']).resolve()
    if any(state == r or r in state.parents for r in roots): raise ValueError('State must be outside source roots')
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    if state.stat().st_uid != os.getuid() or state.stat().st_mode & 0o077: raise ValueError('State must be private')
    with (state / 'runner.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        tick_locked(config, roots)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, help="Private local JSON file (0600)")
    parser.add_argument("--once", action="store_true")
    args = parser.parse_args()
    config = json.loads(private_file(args.config))
    while True:
        try:
            tick(config)
        except (ValueError, OSError, urllib.error.URLError, subprocess.TimeoutExpired) as error:
            if isinstance(error,urllib.error.HTTPError) and error.code==401:
                from execution import cleanup
                for manifest in Path(config['state_dir']).glob('*/services.json'):
                    cleanup(config,manifest.parent.name,config['state_dir'])
                raise SystemExit('Runner credential revoked or rotated; update the private configuration and restart')
            print("Runner tick failed; check connectivity, local roots and registration", flush=True)
            if args.once:
                raise SystemExit(1)
        if args.once:
            break
        time.sleep(10)


if __name__ == "__main__":
    main()
