#!/usr/bin/env python3
"""NexusMind workspace runner: discovery, isolated agents/QA and durable delivery.

Repository code runs only in configured container/VM adapters, never on the host.
"""
import argparse
import base64
import errno
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


def git(args, cwd, timeout=60, binary=False, input_data=None, allowed_codes=(0,), credentials=None, http_headers=None):
    # Never inherit GIT_CONFIG, SSH commands, access tokens, hooks or user filters.
    # Empty HOME prevents reading user git configuration. Network fetches may use
    # only an explicit operator-owned, host-bound source read credential profile.
    with tempfile.TemporaryDirectory(prefix="nexusmind-git-home-") as home:
        env = {"PATH": os.defpath, "HOME": home, "LANG": "C", "GIT_CONFIG_NOSYSTEM": "1",
               "GIT_TERMINAL_PROMPT": "0", "GIT_CONFIG_GLOBAL": os.devnull}
        settings = []
        if credentials:
            host, username, token = credentials
            basic = base64.b64encode((username + ':' + token).encode()).decode()
            settings.append(('http.https://' + host + '/.extraHeader', 'Authorization: Basic ' + basic))
        if http_headers:
            origin, headers = http_headers
            settings.extend(('http.' + origin.rstrip('/') + '/.extraHeader', value) for value in headers)
        if settings:
            env['GIT_CONFIG_COUNT'] = str(len(settings))
            for index, (key, value) in enumerate(settings):
                env['GIT_CONFIG_KEY_' + str(index)] = key
                env['GIT_CONFIG_VALUE_' + str(index)] = value
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
            branch = git(['symbolic-ref', '--quiet', '--short', 'HEAD'], here, allowed_codes=(0, 1))
            if not branch:
                raise ValueError('Choose a named source branch before confirming the inventory')
            repositories.append({"path": here.relative_to(root).as_posix(), "remote": remote,
                                 "head_sha": commit, "branch": branch, "writable": False})
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


def fetch_source(repo, destination, config, workspace_id, roots, proxy=None):
    """Read a confirmed branch through the backend, or an explicit local profile."""
    remote = urllib.parse.urlsplit(repo['remote'])
    hosts = config.get('source_hosts', ['github.com'])
    if (not isinstance(hosts, list) or remote.scheme != 'https' or not remote.hostname or remote.hostname not in hosts
            or remote.port not in (None, 443) or remote.username or remote.password
            or remote.query or remote.fragment):
        raise ValueError('Source refresh requires a credential-free HTTPS remote on an operator-approved source host')
    branch = repo.get('branch')
    if not isinstance(branch, str) or not branch or branch.startswith('-') or branch == 'HEAD':
        raise ValueError('Source refresh requires a confirmed named branch')
    git(['check-ref-format', 'refs/heads/' + branch], destination)
    credentials = None
    filename = config.get('source_credentials', {}).get(workspace_id)
    if filename:
        path = Path(filename)
        if not path.is_absolute() or any(root == path.resolve() or root in path.resolve().parents for root in roots):
            raise ValueError('Source credentials must live outside repository roots')
        value = json.loads(private_file(path))
        if (not isinstance(value, dict) or set(value) != {'host', 'username', 'token'} or value['host'] != remote.hostname
                or not all(isinstance(value[k], str) and 0 < len(value[k]) <= 4096 for k in value)
                or any(c in value['username'] + value['token'] for c in '\r\n')):
            raise ValueError('Invalid source read credential profile')
        credentials = (value['host'], value['username'], value['token'])
    target = repo['remote']
    headers = None
    if not filename:
        if not proxy:
            raise ValueError('Source refresh requires the authenticated backend transport')
        target = proxy['url']
        headers = (target, proxy['headers'])
    try:
        git(['-c', 'http.followRedirects=false', '-c', 'protocol.file.allow=never',
             'fetch', '--depth=1', '--no-tags', '--no-recurse-submodules', '--', target, 'refs/heads/' + branch],
            destination, timeout=60, credentials=credentials, http_headers=headers)
        return git(['rev-parse', '--verify', 'FETCH_HEAD^{commit}'], destination)
    except (ValueError, subprocess.TimeoutExpired):
        raise ValueError('No se pudieron actualizar las fuentes de ' + repo['path'] +
                         '; verifica la rama y el acceso GitHub autorizado en el backend (o el perfil local si lo seleccionaste). No se usará el commit antiguo.') from None


def prepare(job, roots, state_dir, config=None, heartbeat=lambda: None):
    """Refresh new runs in private clones; retries reuse the already pinned base."""
    refresh = job['snapshot'].get('sources_pending') is True
    config = config or {}
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
    # A failed first preparation leaves only its completion journal; after that
    # journal is acknowledged, an empty directory is safe to prepare again.
    if target.is_dir() and not target.is_symlink() and not any(target.iterdir()):
        target.rmdir()
    if target.exists() or target.is_symlink():
        manifest_path = target / 'checkout-manifest.json'
        if target.is_symlink() or not manifest_path.is_file():
            raise ValueError('Run checkout needs reconciliation')
        data = manifest_path.read_bytes()
        manifest = json.loads(data)
        expected = {r['path']: r['head_sha'] for r in workspace['repositories']}
        if refresh:
            identity = {r['path']: {'remote': r['remote'], 'branch': r.get('branch')} for r in workspace['repositories']}
            if manifest.get('sources') != identity or set(manifest.get('commits', {})) != set(expected):
                raise ValueError('Run checkout source identity needs reconciliation')
            expected = manifest['commits']
        if manifest.get('commits') != expected or any(git(['rev-parse', 'HEAD'], target / path) != sha for path, sha in expected.items()):
            raise ValueError('Existing checkout differs from the pinned preparation')
        return [{'check': 'committed-checkouts-prepared', 'status': 'pass', 'commits': expected, 'artifact_digest': hashlib.sha256(data).hexdigest()}]
    stage = Path(tempfile.mkdtemp(prefix="prepare-", dir=state_dir))
    commits = {}
    try:
        for index, repo in enumerate(workspace["repositories"]):
            heartbeat()
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
            if refresh:
                if git(['config', '--get', 'remote.origin.url'], source) != repo['remote']:
                    raise ValueError('Source remote differs from the confirmed inventory')
                proxy = None
                if config.get('api_url') and config.get('runner_token'):
                    api = API(config['api_url'], config['runner_token'])
                    if not safe_id(job['step']['id']):
                        raise ValueError('Invalid source preparation step')
                    proxy = {'url': api.base + '/v1/factory/runner/runs/' + job['run_id'] +
                             '/steps/' + job['step']['id'] + '/sources/' + str(index),
                             'headers': ['Authorization: Bearer ' + config['runner_token'],
                                         'X-NexusMind-Lease: ' + job['lease_token']]}
                sha = fetch_source(repo, destination, config, workspace['id'], roots, proxy)
            git(["checkout", "--detach", sha, "--"], destination, 300)
            # Avoid publishing back into the user's source clone by accident.
            git(["remote", "remove", "origin"], destination)
            actual = git(["rev-parse", "HEAD"], destination)
            if actual != sha:
                raise ValueError("Prepared commit mismatch")
            commits[repo["path"]] = actual
        manifest = {"run_id": job["run_id"], "commits": commits, "isolation": "checkout-only-not-a-sandbox"}
        if refresh:
            manifest['sources'] = {r['path']: {'remote': r['remote'], 'branch': r['branch']} for r in workspace['repositories']}
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
    if any(state.glob('*/services.json')) or any(state.glob('*/application-images.json')):
        active = {r['id']: r['status'] for r in api.call('/runs')['runs']}
        for manifest in list(state.glob('*/services.json')) + list(state.glob('*/application-images.json')):
            if active.get(manifest.parent.name) not in ('running', 'queued'):
                cleanup(config, manifest.parent.name, state)


def exception_diagnostic(error):
    """Report system failure kinds without exposing commands, paths or secrets."""
    code = 'adapter_internal_error'
    solution = 'Revisa los eventos y artefactos de esta fase; no reintentes sin identificar la causa.'
    if isinstance(error, subprocess.TimeoutExpired):
        code = 'adapter_operation_timed_out'
        solution = 'Una operación excedió su tiempo límite. Revisa su fase y disponibilidad del runtime antes de reintentar.'
    elif isinstance(error, subprocess.CalledProcessError):
        code = 'adapter_process_failed'
        solution = 'Un proceso terminó con error. Revisa su log redactado para corregir la causa.'
    elif isinstance(error, KeyError):
        code = 'adapter_configuration_missing'
        solution = 'Falta un campo requerido en la configuración del adaptador; revisa la configuración aplicada al runner.'
    elif isinstance(error, OSError):
        code = 'adapter_os_error_' + errno.errorcode.get(error.errno, type(error).__name__)
        solution = {
            errno.ENOSPC: 'El runner no tiene espacio libre. Libera espacio o amplía el disco sin borrar datos de las aplicaciones.',
            errno.EACCES: 'El runner no tiene permisos para un recurso requerido. Revisa propietario y permisos sin desactivar el aislamiento.',
            errno.ENOENT: 'Falta un archivo o ejecutable requerido en el runner. Revisa la instalación del runtime y sus referencias.',
            errno.EMFILE: 'El runner agotó los descriptores de archivo. Revisa procesos y límites antes de reintentar.',
        }.get(error.errno, 'Falló una operación del sistema en el runner. Revisa el tipo de error y los recursos del runtime.')
        if str(error) == 'AF_UNIX path too long':
            code = 'model_relay_socket_path_too_long'
            solution = 'Actualiza el runner: el relay debe admitir rutas largas sin mover los repositorios ni exponer credenciales.'
    return {'error': code, 'exception_type': type(error).__name__, 'solution': solution, 'hold': True}


def tick_locked(config, roots):
    from execution import isolated_adapter, cleanup, upload, WorkspaceOutputError
    from delivery import services, validate_services, gate, publish
    for manifest in list(Path(config['state_dir']).glob('*/services.json')) + list(Path(config['state_dir']).glob('*/application-images.json')):
        if not manifest.exists(): continue
        if time.time()-json.loads(manifest.read_text()).get('created_at',0)>7200:
            cleanup(config,manifest.parent.name,config['state_dir'])
    api = API(config['api_url'], config['runner_token'])
    reconcile(config, api)
    # Polling has its own persisted due times and scoped connectors; no intake
    # credential is ever downloaded to the runner.
    api.call('/poll-intakes', {})
    from runtime_settings import sync, compile_settings, for_job, suite
    remote_capabilities=[]
    for workspace in api.call('/workspaces')['workspaces']:
        if 'runtime' in workspace:
            report=sync(config,workspace,api,roots,private_file)
            if report['applied_revision'] and report['applied_revision']==workspace['runtime']['revision']:
                declared,services_config=compile_settings(workspace['runtime']['config'],config)
                remote_capabilities.extend('adapter:'+name for name in declared)
                remote_capabilities.extend('executor:' + profile.get('executor', profile.get('harness', profile.get('format', 'claude'))) for profile in declared.values() if profile.get('kind') == 'agent')
                if services_config or (workspace['runtime']['config'].get('auto_environment',True) and report.get('auto_environment')):remote_capabilities.append('adapter:integrated-environment')
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
    capabilities += ['agent-runtime:v1', 'agent-runtime:v2', 'workspace-sources:v1', 'workspace-git-read:v1']
    capabilities += ['executor:' + profile.get('executor', profile.get('harness', profile.get('format', 'claude'))) for profile in adapters.values() if profile.get('kind') == 'agent']
    capabilities += config.get('capabilities', []) + remote_capabilities
    job = api.call('/claim', {'capabilities': capabilities})['job']
    if not job:
        return
    evidence, success = [], False
    outcome = {'input_revision': job['execution']['revision'], 'heads': job['execution']['heads'], 'output': {}, 'cost_usd': 0}
    try:
        config=for_job(config,job,roots,private_file)
        adapter = job['step']['adapter']
        if adapter == 'prepare-workspace' and job['step']['kind'] == 'prepare':
            validate_environment_references(config, job['snapshot']['workspace']['id'], roots)
            started = time.monotonic()
            def source_heartbeat():
                if time.monotonic() - started >= job['step']['timeout_seconds']:
                    raise ValueError('Source preparation exceeded the step time budget')
                api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/lease', {'lease_token': job['lease_token']})
            evidence = prepare(job, roots, config['state_dir'], config, source_heartbeat)
            data = (Path(config['state_dir']) / job['run_id'] / 'checkout-manifest.json').read_bytes()
            artifact = upload(api, job, 'checkout-manifest.json', 'application/json', data)
            evidence[0]['artifact_digest'] = artifact['digest']
            if job['snapshot'].get('sources_pending') is True:
                outcome['heads'] = evidence[0]['commits']
                outcome['output'] = {'sources_refreshed': True}
            success = True
        elif adapter == 'integrated-environment':
            success, evidence, outcome = services(config, job, roots, api, git, private_file)
        elif adapter == 'evidence-gate':
            success, evidence, outcome = gate(job, api)
        elif adapter == 'policy-publisher':
            success, evidence, outcome = publish(config, job, api, git)
        else:
            profile = config.get('adapters',{})[adapter]
            if job['step']['write_repositories']:
                cleanup(config, job['run_id'], config['state_dir'])
            if profile.get('network') == 'services': validate_services(config, job)
            if profile.get('kind')=='suite':
                success,evidence,outcome=suite(config,job,profile,roots,api,git,private_file)
            else:
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
        # include private arguments. Workspace diagnostics contain only relative
        # paths, counts/sizes and fixed remediation text; never file contents.
        outcome['output'] = (error.details if isinstance(error, WorkspaceOutputError)
                             else {'error': str(error)[:1000], 'hold': True} if isinstance(error, ValueError)
                             else exception_diagnostic(error))
        if isinstance(error, urllib.error.HTTPError):
            try:
                code = json.load(error).get('code', '')
                outcome['output'] = {'error': code, 'hold': code in ('policy_approval_required', 'policy_never')
                                     or (400 <= error.code < 500 and error.code not in (408, 429))}
                if code == 'invalid_event':
                    outcome['output']['solution'] = 'El backend rechazó un evento del runner. Actualiza el runner para usar el contrato de eventos admitido; no es un fallo de los tests y no se reintentará automáticamente.'
            except (ValueError, OSError): pass
            finally: error.close()
        print('Workflow step blocked: ' + json.dumps(outcome['output']), flush=True)
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
                for manifest in list(Path(config['state_dir']).glob('*/services.json')) + list(Path(config['state_dir']).glob('*/application-images.json')):
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
