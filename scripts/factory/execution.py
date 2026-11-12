"""Runner-side isolated execution. Never executes repository code on the host.

Docker/Podman adapters run with immutable images and constrained mounts. External
adapters are explicitly operator-installed VM/device sandbox drivers (e.g. iOS).
"""
import base64
import contextlib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
import time
import uuid

from control_plane import ControlPlaneError
from log_stream import LogStream

MAX_LOG = 1024 * 1024
IGNORE = {'.git', '.codex', '.claude', 'node_modules', 'target', '.next', '__pycache__'}
SAFE_NAME = re.compile(r'^[a-zA-Z0-9_-]{1,100}$')


def environment(config, workspace_id, refs, roots, private_file):
    available = config.get('environments', {}).get(workspace_id, {})
    files, secrets = [], []
    for ref in refs:
        if ref not in available:
            raise ValueError('Missing local environment reference: ' + ref)
        path = Path(available[ref])
        canonical = path.resolve(strict=True)
        if path.is_symlink() or any(r == canonical or r in canonical.parents for r in roots):
            raise ValueError('Unsafe environment reference')
        text = private_file(path)
        # Docker's env-file syntax is deliberately not shell syntax. Never source.
        for line in text.splitlines():
            line = line.strip()
            if not line or line.startswith('#'):
                continue
            if '=' not in line or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', line.split('=', 1)[0]):
                raise ValueError('Environment file must contain literal KEY=value lines')
            value = line.split('=', 1)[1]
            if len(value) >= 4:
                secrets.extend([value, value.strip('\"\'')])
        files.append(canonical)
    return files, secrets


def secret_like(value):
    """Literals that carry no secret (booleans, null, numbers) are never redacted:
    replacing them corrupts structured output and floods logs with markers."""
    return len(value) >= 4 and value.lower() not in ('true', 'false', 'null', 'none') and not re.fullmatch(r'[-+]?\d+(\.\d+)?', value)


def redact_structure(value, secrets):
    """Redact inside strings only, so JSON structure can never be broken."""
    if isinstance(value, str):
        return redact(value, secrets)
    if isinstance(value, list):
        return [redact_structure(v, secrets) for v in value]
    if isinstance(value, dict):
        return {redact(k, secrets) if isinstance(k, str) else k: redact_structure(v, secrets) for k, v in value.items()}
    return value


def redact(text, secrets):
    for secret in sorted(set(secrets), key=len, reverse=True):
        if secret_like(secret):
            text = text.replace(secret, '[REDACTED]')
    text = re.sub(r'(?i)(bearer\s+|(?:api[_-]?key|password|secret|token)[\"\s]*[:=][\"\s]*)([^\s\",}]+)', r'\1[REDACTED]', text)
    text = re.sub(r'(?:gh[pousr]_[A-Za-z0-9_]+|sk-[A-Za-z0-9_-]{12,}|nmr_[A-Za-z0-9-]+)', '[REDACTED]', text)
    return text


class WorkspaceOutputError(ValueError):
    def __init__(self, code, path, actual, limit, solution):
        super().__init__(code)
        self.details = {'error': code, 'path': path, 'actual': actual,
                        'limit': limit, 'solution': solution, 'hold': True}


def ignored_source(name):
    return (name in IGNORE or name.endswith('.zip') or name == '.env'
            or name.startswith('.env.') and not name.endswith(('.example', '.sample', '.template')))


def check_tree(root):
    """No devices, sockets, hard links or escaping symlinks from a container."""
    root = root.resolve()
    total = 0
    entries = 0
    for here, dirs, files in os.walk(root, followlinks=False):
        # Generated dependency/cache paths are never read or adopted on the host.
        # They may link to preinstalled dependencies inside the runtime image.
        dirs[:] = [d for d in dirs if not ignored_source(d) or d=='.git']
        for name in dirs + [f for f in files if not ignored_source(f) or f=='.git']:
            entries += 1
            if entries > 100000: raise ValueError('Workspace file-count quota exceeded')
            p = Path(here) / name
            info = p.lstat()
            if name == '.git':
                raise ValueError('Container output must not contain Git metadata')
            if stat.S_ISLNK(info.st_mode):
                resolved = p.resolve()
                if root not in resolved.parents and resolved != root:
                    raise ValueError('Escaping workspace symlink')
            elif not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)):
                raise ValueError('Unsupported workspace file type')
            elif stat.S_ISREG(info.st_mode):
                if info.st_nlink != 1:
                    raise WorkspaceOutputError(
                        'workspace_hard_link', p.relative_to(root).as_posix(), info.st_nlink, 1,
                        'Sustituye el enlace duro por una copia independiente en el workspace del runner y reintenta la fase. No modifiques el otro archivo enlazado.')
                if info.st_size > 10 * 1024 * 1024:
                    raise WorkspaceOutputError(
                        'workspace_file_too_large', p.relative_to(root).as_posix(), info.st_size, 10 * 1024 * 1024,
                        'El archivo supera 10 MiB. Si es generado, retíralo de la entrada/salida del sandbox; si es código necesario, reduce o divide su contenido. Conserva el original y reintenta la fase. Una aprobación no permite omitir este control.')
                total += info.st_size
                if total > 256 * 1024 * 1024:
                    raise ValueError('Workspace output quota exceeded')


def copy_source(source, destination):
    def ignored(_directory, names):
        return [n for n in names if ignored_source(n)]
    shutil.copytree(source, destination, symlinks=True, ignore=ignored)
    try:
        check_tree(destination)
    except WorkspaceOutputError as error:
        error.details['path'] = destination.name + '/' + error.details['path']
        raise


def pinned_image(image):
    if not isinstance(image, str) or not (re.search(r'@sha256:[0-9a-f]{64}$', image) or re.fullmatch(r'sha256:[0-9a-f]{64}', image)):
        raise ValueError('Adapter images must be pinned by digest (not mutable tags)')
    return image


def node_bootstrap_image(config, image):
    return bool(image) and (image == config.get('managed_images', {}).get('node') or image in config.get('node_bootstrap_images', []))


def runtime_command(config):
    runtime = config.get('container_runtime', 'docker')
    if runtime not in ('docker', 'podman'):
        raise ValueError('Unsupported container runtime')
    return runtime


def runtime_environment(config, run):
    # Docker otherwise injects personal proxy settings from ~/.docker/config.json
    # into builds/containers, even when no --env or --secret was requested.
    root = Path(config['state_dir']) / run / '.docker-client'
    if root.is_symlink(): raise ValueError('Unsafe Docker client state')
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    return {'PATH': os.defpath, 'HOME': str(root), 'DOCKER_CONFIG': str(root), 'BUILDX_CONFIG': str(root / 'buildx'), 'LANG': 'C', 'XDG_RUNTIME_DIR': '/run/user/' + str(os.getuid())}


def network_name(run):
    return 'nm-' + run.replace('-', '')[:28]


def restrictions(node_bootstrap=False):
    if os.getuid()==0: raise ValueError('Run the workspace runner as an unprivileged user')
    return ['--user', str(os.getuid()) + ':' + str(os.getgid()), '--read-only', '--cap-drop=ALL',
            '--security-opt=no-new-privileges', '--pids-limit=256', '--memory=2g', '--cpus=2',
            '--ulimit', 'nofile=1024:1024', '--ulimit', ('fsize=268435456:268435456' if node_bootstrap else 'fsize=16777216:16777216'), '--tmpfs', ('/tmp:rw,nosuid,nodev,size=1024m' if node_bootstrap else '/tmp:rw,nosuid,nodev,size=128m'),
            '--tmpfs', '/home/runner:rw,nosuid,nodev,size=64m,mode=1777', '--env', 'HOME=/home/runner']


def adapter_network(config, adapter, run):
    network = adapter.get('network', 'none')
    if network == 'services':
        return network_name(run)
    if network == 'none':
        return 'none'
    if network in ('host', 'bridge', 'container') or network not in config.get('allowed_egress_networks', []):
        raise ValueError('Network is not locally authorized')
    probe=subprocess.run([runtime_command(config),'network','inspect',network,'--format','{{.Internal}}'],capture_output=True,text=True,timeout=10)
    if probe.returncode or probe.stdout.strip()!='true':
        raise ValueError('Agent egress networks must be internal; connect an allowlisting model proxy explicitly')
    return network


def cleanup(config, run, state_dir):
    root = Path(state_dir) / run
    manifest = root / 'services.json'
    if not manifest.exists():
        if (root / 'application-images.json').exists():
            from application_environment import cleanup as cleanup_application
            cleanup_application(config, run)
        return
    services = json.loads(manifest.read_text())
    runtime = runtime_command(config)
    client_env = runtime_environment(config, run)
    for name in services.get('containers', []) + services.get('probes', []):
        if not re.fullmatch(r'nm-[a-zA-Z0-9_-]+', name):
            raise ValueError('Unsafe service name')
        subprocess.run([runtime, 'rm', '-f', name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30, env=client_env)
    subprocess.run([runtime, 'network', 'rm', network_name(run)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30, env=client_env)
    from application_environment import cleanup as cleanup_application
    cleanup_application(config, run)
    manifest.unlink(missing_ok=True)


def stage_sources(job, state_dir, git):
    run_root = Path(state_dir) / job['run_id']
    if not run_root.is_dir() or run_root.is_symlink():
        raise ValueError('Committed run checkouts are missing')
    marker=run_root/'checkout-manifest.json'
    if not marker.is_file() or marker.is_symlink() or json.loads(marker.read_text()).get('run_id')!=job['run_id']:
        raise ValueError('Run checkout ownership marker is missing')
    working = Path(tempfile.mkdtemp(prefix='step-', dir=run_root))
    work = working / 'work'
    work.mkdir()
    heads = job['execution']['heads']
    for repo in job['snapshot']['workspace']['repositories']:
        source = run_root / repo['path']
        actual = git(['rev-parse', 'HEAD'], source)
        if actual != heads[repo['path']]:
            raise ValueError('Local checkout differs from the server change set; reconcile before retry')
        git(['diff', '--exit-code', 'HEAD'], source)
        # This is a marked, runner-owned disposable clone, never a personal repo.
        # Remove untracked build output so the sandbox receives only the commit.
        git(['clean','-fdx'],source)
        destination = work / repo['path']
        destination.parent.mkdir(parents=True, exist_ok=True)
        copy_source(source, destination)
    return working, work


def monitor(command, config, job, api, name, log_path, secrets=None, source='runner'):
    """Bound process lifetime, output and cancellation; no shell interpolation.
    Output is streamed sequentially (no tail jumps) while the process runs, and
    the outcome is recorded as `command.finished` on every exit path."""
    started = time.monotonic()
    deadline = min(job['step']['timeout_seconds'], max(1, job['lease_until'] - int(time.time()) - 5))
    stream = LogStream(lambda kind, text, fields: send_event(api, job, kind, text, fields), log_path,
                       lambda text: redact(text, secrets or []), secrets or [], source=source)
    finished = {'exit_code': None, 'timed_out': False, 'output_exceeded': False}
    last_lease = started
    try:
        with log_path.open('wb') as output:
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT,
                                       env=runtime_environment(config, job['run_id']) if command[0] in ('docker', 'podman') else {'PATH': os.defpath, 'HOME': os.environ.get('HOME', '/tmp'), 'LANG': 'C'})
            try:
                while process.poll() is None:
                    if time.monotonic() - started > deadline:
                        finished['timed_out'] = True
                        raise ValueError('Adapter time budget exceeded (' + str(int(deadline)) + ' s)')
                    if log_path.stat().st_size > 16 * MAX_LOG:
                        finished['output_exceeded'] = True
                        raise ValueError('Adapter output budget exceeded (' + str(16 * MAX_LOG) + ' bytes)')
                    time.sleep(1)
                    stream.poll()
                    if time.monotonic() - last_lease >= 5:
                        api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/lease', {'lease_token': job['lease_token']})
                        last_lease = time.monotonic()
                finished['exit_code'] = process.returncode
                return process.returncode
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill(); process.wait()
                if name:
                    subprocess.run([runtime_command(config), 'rm', '-f', name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
    finally:
        try:
            stats = stream.finish()
            code = finished['exit_code']
            data = dict(finished, duration_ms=int((time.monotonic() - started) * 1000), bytes=stats['bytes'],
                        gap_bytes=stats['gap_bytes'], truncated=stats['truncated'],
                        dropped_events=job.pop('dropped_events', 0))
            if code is not None and code < 0:
                data['signal'] = -code
            if code == 137:
                data['hint'] = 'killed_or_out_of_memory'
            summary = ('exit ' + str(code)) if code is not None else ('timeout' if finished['timed_out'] else 'interrupted')
            send_event(api, job, 'command.finished', summary, {'source': source, 'data': data,
                       'severity': 'info' if code == 0 else 'error'})
        except OSError:
            pass  # A lost lease or I/O error must never hide the primary outcome.


def mark_stage(job, stage):
    if isinstance(job.get('_trace'), dict):
        job['_trace']['stage'] = stage


def upload(api, job, name, mime, data):
    if len(data) > MAX_LOG:
        raise ValueError('Artifact exceeds per-file limit')
    return api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/artifacts', {
        'lease_token': job['lease_token'], 'name': name, 'mime': mime,
        'content_base64': base64.b64encode(data).decode(),
    })


def send_event(api, job, kind, message, fields=None):
    """Send one structured event. Returns 'ok', 'retry' (transient failure) or
    'quota' (the run's budget for this class is exhausted). Contract and lease
    errors raise. Structured fields are dropped for a backend that predates them."""
    path = '/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/events'
    body = {'lease_token': job['lease_token'], 'kind': kind, 'message': message.encode()[:4000].decode(errors='ignore')}
    if fields and not getattr(api, 'legacy_events', False):
        body.update({k: v for k, v in fields.items() if v is not None})
    try:
        api.call(path, body)
        return 'ok'
    except ControlPlaneError as error:
        if error.backend_code == 'invalid_json' and len(body) > 3:
            api.legacy_events = True
            return send_event(api, job, kind, message)
        if error.backend_code == 'event_quota':
            return 'quota'
        if error.transient:
            print('Workflow event not recorded: ' + json.dumps({'kind': kind, 'status': error.status,
                  'code': error.backend_code or None, 'request_id': error.request_id}), flush=True)
            return 'retry'
        raise


def log_event(api, job, kind, message, fields=None):
    """Best-effort: a backend outage or the per-run event quota must not abort the
    step it describes. Contract and lease errors still raise. Lost events are
    counted and announced in the next event that does get through."""
    dropped = job.get('dropped_events', 0)
    if dropped:
        message = '[runner: ' + str(dropped) + ' eventos anteriores no se registraron]\n' + message
    if send_event(api, job, kind, message, fields) != 'ok':
        job['dropped_events'] = dropped + 1
        return
    job.pop('dropped_events', None)


@contextlib.contextmanager
def preserve_failure_logs(api, job, working, out, secrets):
    """On timeout, cancellation or an exception, keep the redacted tail of every
    log as an artifact. A failed export never replaces the original error."""
    try:
        yield
    except BaseException:
        for path in [working / 'runtime.log', *sorted(out.glob('*.log'))]:
            try:
                if path.is_file() and not path.is_symlink():
                    with path.open('rb') as log:
                        log.seek(max(0, path.stat().st_size - MAX_LOG // 2))
                        tail = log.read().decode(errors='replace')
                    upload(api, job, ('failure-' + path.name)[:120], 'text/plain', redact(tail, secrets).encode()[:MAX_LOG])
            except Exception:  # noqa: BLE001 - best effort by design
                pass
        raise


def protect_verification_scripts(config,job,adapter,git):
    if adapter.get('allow_verification_script_changes'):
        return  # Explicit local-operator authority; never an issue/repo setting.
    names={'test','test:ci','test:unit','test:integration','test:e2e','test:smoke','build','build:prod','lint','lint:ci','typecheck','type-check','check:types','types:check','check'}
    for repo in job['snapshot']['workspace']['repositories']:
        root=Path(config['state_dir'])/job['run_id']/repo['path']
        paths=git(['ls-tree','-r','--name-only','-z',repo['head_sha']],root,binary=True).split(b'\0')
        for raw in paths:
            path=raw.decode()
            if not (path=='package.json' or path.endswith('/package.json')): continue
            baseline=json.loads(git(['show',repo['head_sha']+':'+path],root))
            try: current=json.loads(git(['show','HEAD:'+path],root))
            except ValueError: current={}
            for name,body in baseline.get('scripts',{}).items():
                if name in names and current.get('scripts',{}).get(name)!=body:
                    raise ValueError('Verification script changed; configure independently trusted checks before publishing')


def repo_secrets(api, job):
    """Operator-managed secrets per repository for this leased step ({} when none,
    or when the backend predates them). The backend never serves review steps."""
    try:
        return api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/repo-secrets',
                        {'lease_token': job['lease_token']}).get('repositories', {})
    except ControlPlaneError as error:
        if error.status == 404:
            return {}
        raise


def secret_values(repositories):
    """Values to redact from logs: every .env value and every .npmrc credential."""
    from node_environment import parse_npmrc, secrets as npm_secrets
    values = []
    for entry in repositories.values():
        for line in entry.get('env', '').splitlines():
            line = line.strip()
            if line and not line.startswith('#') and '=' in line:
                value = line.split('=', 1)[1].strip()
                values.extend([value, value.strip('\'"')])
        if entry.get('npmrc'):
            values.extend(npm_secrets(parse_npmrc(entry['npmrc'])))
    return [v for v in values if secret_like(v)]


def write_env_files(work, repositories):
    """Write each repository's .env into the disposable step copy. `.env` is an
    ignored source name, so it never flows back into commits or publication."""
    root = Path(work).resolve()
    for path, entry in repositories.items():
        if 'env' not in entry:
            continue
        repo = (root / path).resolve()
        if repo == root or root not in repo.parents or not repo.is_dir() or (root / path).is_symlink():
            raise ValueError('Repository secret target is outside the staged workspace')
        target = repo / '.env'
        if target.is_symlink() or target.exists():
            target.unlink()
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, 'w') as stream:
            stream.write(entry['env'] if entry['env'].endswith('\n') else entry['env'] + '\n')


def isolated_adapter(config, job, adapter, roots, api, git, private_file):
    if adapter.get('kind') not in ('agent','checks'): raise ValueError('Invalid adapter kind')
    if adapter['kind']=='checks' and (not adapter.get('checks') or any(not SAFE_NAME.fullmatch(c.get('name','')) for c in adapter['checks'])):
        raise ValueError('Checks need safe, unique named commands')
    if adapter['kind']=='checks' and len({c['name'] for c in adapter['checks']})!=len(adapter['checks']): raise ValueError('Duplicate check names')
    if adapter['kind']=='checks': protect_verification_scripts(config,job,adapter,git)
    mark_stage(job, 'stage_sources')
    working, work = stage_sources(job, config['state_dir'], git)
    verified_source = tree_fingerprint(work)
    out = working / 'output'; out.mkdir()
    inp = working / 'input'; inp.mkdir()
    refs = adapter.get('environment_refs', [])
    if job['step']['kind'] == 'review' and refs:
        # Reviews should reason about code/evidence, never need application secrets.
        raise ValueError('Independent reviewers cannot receive environment secrets')
    env_files, secrets = environment(config, job['snapshot']['workspace']['id'], refs, roots, private_file)
    # Operator secrets reach only deterministic checks, never an LLM agent's
    # context: .env goes into the staged copy, .npmrc stays on the host.
    repositories = repo_secrets(api, job) if adapter['kind'] == 'checks' and job['step']['kind'] != 'review' else {}
    secrets.extend(secret_values(repositories))
    write_env_files(work, repositories)
    backend_model = adapter.get('model_connection') == 'backend'
    if backend_model and (adapter.get('driver', 'container') != 'container' or adapter.get('kind') != 'agent' or adapter.get('harness') != 'pi' or adapter.get('network') != 'none' or not config.get('allow_backend_models') or adapter['image'] != config.get('managed_images', {}).get('agent')):
        raise ValueError('Backend models require the locally authorized, offline Pi runtime')
    if adapter.get('model_connection') and not backend_model:
        from runtime_settings import _model
        model_file, model_secrets = _model(config,adapter['model_connection'],roots,private_file)
        env_files.append(model_file);secrets.extend(model_secrets)
    secrets.append(config['runner_token'])
    context = {k: job[k] for k in ['run_id', 'step', 'task', 'dependency_results']}
    if job.get('task_scope') == 'documentation':
        context['task_scope'] = 'documentation'
    context['step'] = dict(job['step'])
    if isinstance(job['step'].get('options'), dict):
        context['step']['options'] = {k: v for k, v in job['step']['options'].items() if k not in ('runtime', 'runtime_snapshot')}
    from agent_runtime import effective_runtime
    resolved = effective_runtime(job, adapter).get('config', {}).get('runtime_resolved', {})
    context['harness_markdown'] = [{'id': c['id'], 'files': c['files']} for c in resolved.get('components', []) if c.get('kind') == 'instructions' and c.get('enabled', True)]
    context['commits'] = job['execution']['heads']
    budget = job['snapshot']['definition'].get('max_cost_usd', 20)
    attempts = sum(s.get('max_attempts',1) for s in job['snapshot']['definition'].get('steps',[job['step']]))
    # Fixed per-attempt allocation also bounds spend when an API timeout loses
    # usage reporting. Attempts are persisted and never reset by repair cycles.
    context['remaining_cost_usd'] = max(0.01, min(budget / max(1,attempts), budget - job['execution']['cost_usd']))
    if job['step']['kind'] != 'review':
        context['repair_feedback'] = job['execution'].get('repair_feedback')
    # Read-only diff context is reconstructed from the pinned base, not from the
    # implementation conversation. No .git directory is exposed to the model.
    context['diffs'] = {}
    for repo in job['snapshot']['workspace']['repositories']:
        diff = git(['diff', '--no-ext-diff', '--no-textconv', repo['head_sha'], 'HEAD', '--'], Path(config['state_dir']) / job['run_id'] / repo['path'])
        if len(diff.encode()) > 256 * 1024:
            raise ValueError('Diff context exceeds review budget')
        context['diffs'][repo['path']] = diff
    # Agents run offline (network=none for Pi), so `npm ci` inside them cannot
    # work: writable agent steps get the same lockfile-pinned offline install as checks.
    agent_node = []
    # Same condition as the read-only /work mount below.
    if adapter['kind']=='agent' and adapter.get('driver','container')=='container' and adapter.get('role') not in ('investigation','reviewer') and job['step']['kind']!='review':
        agent_node = [{'cwd':r} for r in job['step'].get('write_repositories') or [] if (work/r/'package.json').is_file() and (work/r/'package-lock.json').is_file()]
    if adapter.get('node_bootstrap') or agent_node:
        if adapter.get('node_bootstrap') and (adapter['kind']!='checks' or not node_bootstrap_image(config, adapter['image'])):
            raise ValueError('Node environment must be registered by the operator')
        from node_environment import prepare
        started=time.monotonic();last_heartbeat=started
        def heartbeat():
            nonlocal last_heartbeat
            if time.monotonic()-last_heartbeat>=5:
                api.call('/runs/'+job['run_id']+'/steps/'+job['step']['id']+'/lease',{'lease_token':job['lease_token']});last_heartbeat=time.monotonic()
        if agent_node:
            # Only the operator .npmrc (host-side, private registries); agents never get .env.
            repositories={path:{'npmrc':entry['npmrc']} for path,entry in repo_secrets(api, job).items() if entry.get('npmrc')}
            secrets.extend(secret_values(repositories))
        mark_stage(job, 'node_dependencies')
        log_event(api,job,'command.started','Preparando dependencias npm fijadas por lockfile, sin scripts de instalación')
        npmrc={path:entry['npmrc'] for path,entry in repositories.items() if entry.get('npmrc')}
        destination=inp/'node-dependencies'
        try:
            prepare(work,adapter.get('checks') or agent_node,destination,Path(config['state_dir'])/'.node-cache',heartbeat,timeout=min(300,job['step']['timeout_seconds']),npmrc=npmrc)
            if agent_node:adapter=dict(adapter,node_dependencies=[c['cwd'] for c in agent_node]);context['node_dependencies']=adapter['node_dependencies']
        except (ValueError, OSError) as error:
            if not agent_node:raise
            # Best effort for agents: without prepared dependencies they run as before.
            shutil.rmtree(destination,ignore_errors=True)
            log_event(api,job,'dependency.log','Dependencias npm no preparadas para el agente: '+str(error))
        remaining=job['step']['timeout_seconds']-(time.monotonic()-started)
        if remaining<1:raise ValueError('Node environment preparation exceeded step time budget')
        # Shallow copy keeps the shared step trace; only the timeout changes.
        job=dict(job);job['step']=dict(job['step']);job['step']['timeout_seconds']=int(remaining)
    from agent_runtime import stage_runtime
    stage_runtime(config, job, adapter, inp)
    (inp / 'context.json').write_text(json.dumps(context))
    (inp / 'adapter.json').write_text(json.dumps(adapter))
    name = None
    if adapter.get('driver', 'container') == 'container':
        runtime = runtime_command(config)
        name = 'nm-' + uuid.uuid4().hex
        command = [runtime, 'run', '--rm', '--pull=never', '--name', name, *restrictions(bool(adapter.get('node_bootstrap') or adapter.get('node_dependencies'))),
                   '--network', adapter_network(config, adapter, job['run_id']), '--workdir', '/work',
                   '--mount', 'type=bind,src=' + str(work) + ',dst=/work' + (',readonly' if adapter.get('role') in ('investigation','reviewer') or job['step']['kind']=='review' else ''),
                   '--mount', 'type=bind,src=' + str(inp) + ',dst=/input,readonly',
                   '--mount', 'type=bind,src=' + str(out) + ',dst=/output',
                   '--mount', 'type=bind,src=' + str(Path(__file__).with_name('container_entry.py').resolve()) + ',dst=/entry.py,readonly']
        command += ['--mount', 'type=bind,src=' + str(Path(__file__).with_name('agent_runtime.py').resolve()) + ',dst=/agent_runtime.py,readonly']
        if backend_model:
            model_root = working / 'model'; model_root.mkdir()
            command += ['--mount', 'type=bind,src=' + str(model_root) + ',dst=/model',
                        '--mount', 'type=bind,src=' + str(Path(__file__).with_name('model_relay.py').resolve()) + ',dst=/model_relay.py,readonly']
        if runtime=='podman': command += ['--userns=keep-id']
        for filename in env_files:
            command += ['--env-file', str(filename)]
        command += ['--entrypoint=python3', pinned_image(adapter['image']), '/entry.py']
    elif adapter.get('driver') == 'vm':
        # Explicit, trusted local VM/device driver, never a repository-provided
        # shell command. It must implement the same isolated input/work/output protocol.
        if len(adapter['driver_command'])!=1: raise ValueError('VM adapters require a single installed driver executable')
        driver = Path(adapter['driver_command'][0]).resolve(strict=True)
        if driver.name in ('sh','bash','zsh','env','python','python3','node','ruby','perl','pwsh'):
            raise ValueError('A host interpreter is not an isolated VM driver')
        info = driver.stat()
        if not driver.is_file() or info.st_uid not in (0,os.getuid()) or info.st_mode & 0o022 or any(r == driver or r in driver.parents for r in roots):
            raise ValueError('VM driver must be an operator-installed non-writable executable outside source roots')
        command = [str(driver), '--input', str(inp), '--work', str(work), '--output', str(out), '--timeout', str(job['step']['timeout_seconds'])]
        if env_files:
            raise ValueError('VM drivers must broker secrets themselves; host env files are not forwarded')
    else:
        raise ValueError('Unsupported isolation driver')
    mark_stage(job, 'container')
    log_event(api, job, 'agent.started' if adapter['kind'] == 'agent' else 'command.started', job['step']['adapter'])
    from model_relay import Relay
    with Relay(model_root, api, job) if backend_model else contextlib.nullcontext(), preserve_failure_logs(api, job, working, out, secrets):
        exit_code = monitor(command, config, job, api, name, working / 'runtime.log', secrets,
                            source='agent' if adapter['kind'] == 'agent' else 'check')
    mark_stage(job, 'collect_output')
    check_tree(out)
    check_tree(work)
    if adapter['kind']=='checks' and {k:v for k,v in tree_fingerprint(work).items() if k in verified_source}!=verified_source:
        raise ValueError('Verification changed source files instead of checking the pinned commit')
    if exit_code or not (out / 'result.json').is_file() or (out / 'result.json').is_symlink():
        result = {'success': False, 'error': 'isolated_adapter_failed'}
    else:
        if (out / 'result.json').stat().st_size > 16384:
            raise ValueError('Adapter result exceeds limit')
        result = json.loads((out / 'result.json').read_text())
        if not isinstance(result, dict) or not isinstance(result.get('success'), bool):
            raise ValueError('Invalid adapter result')
    result = redact_structure(result, secrets)
    evidence = []
    # Upload text logs only after redaction. Binary screenshots are explicitly
    # named artifacts and must not contain secrets (operator QA responsibility).
    for path in [working / 'runtime.log', *sorted(out.glob('*.log'))]:
        if path.is_symlink() or path.stat().st_size > 16 * MAX_LOG:
            raise ValueError('Unsafe/oversized adapter log')
        text = redact(path.read_text(errors='replace'), secrets)
        for offset in range(0, max(1, len(text)), MAX_LOG // 2):
            chunk = text[offset:offset + MAX_LOG // 2].encode()
            # Complete, redacted logs as artifacts; the live history was streamed.
            upload(api, job, path.name[:80] + '-' + str(offset), 'text/plain', chunk)
    for path in sorted(out.rglob('*')):
        if path.suffix in ('.png', '.jpg', '.jpeg') and path.is_file() and not path.is_symlink():
            mime = 'image/png' if path.suffix == '.png' else 'image/jpeg'
            name = path.name if path.parent == out else hashlib.sha256(str(path.relative_to(out)).encode()).hexdigest()[:12] + '-' + path.name
            upload(api, job, name[:120], mime, path.read_bytes())
    if result['success'] and adapter['kind']=='agent':
        import difflib
        for repo in job['snapshot']['workspace']['repositories']:
            source=Path(config['state_dir'])/job['run_id']/repo['path']
            modified=work/repo['path']
            original_fingerprint=tree_fingerprint(source)
            changed_fingerprint=without_generated(source,original_fingerprint,tree_fingerprint(modified),git)
            chunks=[]
            for relative in sorted(set(original_fingerprint)|set(changed_fingerprint)):
                if original_fingerprint.get(relative)==changed_fingerprint.get(relative): continue
                def lines(root):
                    p=root/relative
                    if p.is_symlink(): return ['symlink: '+os.readlink(p)+'\\n']
                    if not p.is_file(): return []
                    data=p.read_bytes()
                    if b'\\0' in data: return ['[binary content sha256 '+hashlib.sha256(data).hexdigest()+']\\n']
                    return data.decode(errors='replace').splitlines(keepends=True)
                chunks.extend(difflib.unified_diff(lines(source),lines(modified),fromfile=repo['path']+'/'+relative+' (before)',tofile=repo['path']+'/'+relative+' (candidate)'))
            preview=redact(''.join(chunks),secrets).encode()
            if len(preview)>MAX_LOG: raise ValueError('Candidate diff exceeds review artifact budget')
            if preview: upload(api,job,'candidate-'+hashlib.sha256(repo['path'].encode()).hexdigest()[:12]+'.diff','text/plain',preview)
    mark_stage(job, 'upload_artifacts')
    result_data = json.dumps(result, sort_keys=True).encode()
    artifact = upload(api, job, 'result.json', 'application/json', result_data)
    heads = dict(job['execution']['heads'])
    if result['success'] and adapter['kind'] == 'agent':
        try:
            heads = adopt_changes(config, job, work, git, secrets)
        except BaseException:
            # Roll back ONLY runner-owned disposable clones if any repository in
            # an atomic multi-repo adoption fails. Personal clones are never here.
            for repo in job['snapshot']['workspace']['repositories']:
                original = Path(config['state_dir']) / job['run_id'] / repo['path']
                git(['reset', '--hard', job['execution']['heads'][repo['path']]], original)
                git(['clean', '-fdx'], original)
            raise
    if adapter['kind'] == 'checks':
        checks = result.get('checks', [])
        configured = {c['name'] for c in adapter.get('checks', [])}
        reported = {c.get('name') for c in checks}
        if configured != reported or not configured:
            result['success'] = False
            result['error'] = 'missing_check_results'
        for check in checks:
            evidence.append({'check': check['name'], 'status': 'not_applicable' if check.get('status')=='not_applicable' else ('pass' if check.get('exit_code') == 0 else 'fail'), 'commits': heads, 'artifact_digest': artifact['digest']})
    else:
        evidence.append({'check': job['step']['id'], 'status': 'not_applicable' if result.get('not_applicable_reason') else ('pass' if result['success'] else 'fail'), 'commits': heads, 'artifact_digest': artifact['digest']})
    return result['success'], evidence, {'input_revision': job['execution']['revision'], 'heads': heads, 'output': result, 'cost_usd': result.get('cost_usd', 0)}


def adopt_changes(config, job, work, git, secrets):
    heads = dict(job['execution']['heads'])
    authorized = set(job['step']['write_repositories'])
    for repo in job['snapshot']['workspace']['repositories']:
        relative = repo['path']
        original = Path(config['state_dir']) / job['run_id'] / relative
        modified = work / relative
        check_tree(modified)
        # Compare only source files, excluding generated outputs. No untrusted Git
        # index/config/hooks can reach the host. Validation precedes all adoption.
        baseline = work.parent / ('baseline-' + hashlib.sha256(relative.encode()).hexdigest()[:12])
        copy_source(original, baseline)
        before = tree_fingerprint(baseline)
        after = without_generated(original,before,tree_fingerprint(modified),git)
        if before != after and relative not in authorized:
            raise ValueError('Read-only repository was modified: ' + relative)
        if relative not in authorized or before == after:
            continue
        for p in modified.rglob('*'):
            if p.is_file() and not p.is_symlink() and not any(ignored_source(part) for part in p.relative_to(modified).parts):
                content = p.read_bytes()
                if any(len(s) >= 4 and s.encode() in content for s in secrets):
                    raise ValueError('Credential detected in generated code')
        # Never allow an agent to delete/rewrite tests unnoticed. Review and the
        # deterministic QA commands are still required for every new revision.
        for child in original.iterdir():
            if child.name != '.git':
                if child.is_dir() and not child.is_symlink(): shutil.rmtree(child)
                else: child.unlink()
        for child in modified.iterdir():
            if ignored_source(child.name): continue
            target = original / child.name
            if child.is_dir() and not child.is_symlink(): shutil.copytree(child, target, symlinks=True, ignore=lambda _d,names:[n for n in names if ignored_source(n)])
            elif child.is_symlink(): target.symlink_to(os.readlink(child))
            else: shutil.copy2(child, target)
        # ponytail: excluded inputs stay unchanged in Git, never copied into the sandbox.
        excluded = [p for p in git(['ls-files', '-z'], original, binary=True).split(b'\0')
                    if p and any(ignored_source(part) for part in Path(os.fsdecode(p)).parts)]
        if excluded:
            git(['--literal-pathspecs', 'restore', '--source=HEAD', '--worktree',
                 '--pathspec-from-file=-', '--pathspec-file-nul'], original,
                input_data=b'\0'.join(excluded) + b'\0')
        git(['add', '-A', '--', '.'], original)
        # Deterministic metadata lets the control plane reproduce the exact
        # verified Git object over GitHub's Git Database API without sharing a token.
        message = 'NexusMind ' + job['run_id'] + ' ' + job['step']['id'] + ' attempt ' + str(job['attempt'])
        git(['-c', 'user.name=NexusMind', '-c', 'user.email=factory@nexusmind.local', '-c', 'commit.gpgSign=false',
             'commit', '-qm', message, '--date=2000-01-01T00:00:00Z'], original)
        # Rewrite committer date with a trusted environment-free hash-object path.
        raw = git(['cat-file', 'commit', 'HEAD'], original)
        raw = re.sub(r'committer NexusMind <factory@nexusmind.local> \d+ [+-]\d{4}',
                     'committer NexusMind <factory@nexusmind.local> 946684800 +0000', raw, count=1) + '\n'
        # git() does not accept stdin; invoke the same restricted helper via a
        # temporary input file consumed by hash-object, never repository code.
        commit_file = work.parent / 'commit-object.txt'; commit_file.write_text(raw)
        sha = git(['hash-object', '-t', 'commit', '-w', str(commit_file)], original)
        git(['update-ref', 'HEAD', sha], original)
        heads[relative] = sha
    return heads


def without_generated(source, before, after, git):
    added=set(after)-set(before)
    if not added:
        return after
    ignored=set(git(['check-ignore','--no-index','-z','--stdin'],source,binary=True,
                    input_data=b'\0'.join(p.encode() for p in sorted(added))+b'\0',allowed_codes=(0,1)).split(b'\0'))
    return {p:v for p,v in after.items() if p.encode() not in ignored}


def tree_fingerprint(root):
    result = {}
    for here, dirs, files in os.walk(root, followlinks=False):
        dirs[:] = [d for d in dirs if not ignored_source(d)]
        for name in [f for f in files if not ignored_source(f)] + [d for d in dirs if (Path(here) / d).is_symlink()]:
            p = Path(here) / name
            rel = p.relative_to(root).as_posix()
            if p.is_symlink(): data = os.readlink(p).encode()
            else: data = p.read_bytes()
            result[rel] = (hashlib.sha256(data).hexdigest(), bool(p.lstat().st_mode & 0o111))
    return result
