"""Candidate-only application environments. No Compose execution or host env inheritance.

Compose is a declarative input, not permission to run `compose up` with arbitrary
host mounts, networks or privileges. Unsupported intent is an explicit decision.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shlex
import shutil
import subprocess
import time
import uuid
from urllib.parse import urlsplit, quote

from execution import pinned_image, runtime_command, runtime_environment, monitor

COMPOSE = ('compose.yaml', 'compose.yml', 'docker-compose.yml', 'docker-compose.yaml')
OFFICIAL = {'node', 'python', 'postgres', 'redis', 'mysql', 'mariadb', 'nginx', 'alpine', 'debian', 'ubuntu', 'golang', 'rust'}
NAME = re.compile(r'^[a-zA-Z][a-zA-Z0-9_-]{0,39}$')
VARIABLE = re.compile(r'\$\{([A-Za-z_][A-Za-z0-9_]*)(?:(:-|-|:\?|\?)([^}]*))?\}|\$([A-Za-z_][A-Za-z0-9_]*)')


class EnvironmentDecision(ValueError):
    """Contains only paths, field names and missing variable names, never values."""


def inside(root, value):
    if not isinstance(value, str) or Path(value).is_absolute() or ',' in value or any(ord(c) < 32 for c in value):
        raise EnvironmentDecision('Environment paths must be relative to the candidate')
    path = (root / value).resolve(strict=True)
    if path != root.resolve() and root.resolve() not in path.parents:
        raise EnvironmentDecision('Environment path escapes the candidate')
    return path


def relative_source(work, root, value):
    if not isinstance(value, str) or Path(value).is_absolute():
        raise EnvironmentDecision('Build contexts and bind sources must be candidate-relative')
    return inside(work, str((root / value).relative_to(work)))


def load_compose(path):
    # Use the existing YAML ecosystem, not an incomplete hand-written YAML parser.
    import yaml
    text = path.read_text()
    if len(text) > 1024 * 1024:
        raise EnvironmentDecision('Compose manifest exceeds the size budget')
    try:
        for index, event in enumerate(yaml.parse(text)):
            if index > 20000 or isinstance(event, yaml.AliasEvent):
                raise EnvironmentDecision('Compose aliases/oversized manifests need an explicit profile')
        data = yaml.safe_load(text)
    except yaml.YAMLError:
        raise EnvironmentDecision('Invalid Compose YAML') from None
    if not isinstance(data, dict) or not isinstance(data.get('services'), dict):
        raise EnvironmentDecision('Compose must declare services')
    if set(data) - {'name', 'version', 'services', 'volumes', 'networks'}:
        raise EnvironmentDecision('Compose includes/secrets/configs need an explicit profile')
    if any(v is not None and (not isinstance(v, dict) or v.get('external') or set(v) - {'name'}) for v in (data.get('volumes') or {}).values()):
        raise EnvironmentDecision('External volumes are not disposable test data')
    # Network names and host bindings never become authority; use one internal network.
    if any(v is not None and (not isinstance(v, dict) or v.get('external') or v.get('driver', 'bridge') != 'bridge' or set(v) - {'name', 'driver', 'internal'}) for v in (data.get('networks') or {}).values()):
        raise EnvironmentDecision('External or custom networks need an explicit decision')
    return data


def argv(value):
    if value is None:
        return []
    result = shlex.split(value) if isinstance(value, str) else value
    if not isinstance(result, list) or not result or any(not isinstance(x, str) or '\x00' in x for x in result):
        raise EnvironmentDecision('Invalid service command')
    return result


def env_pairs(raw):
    if isinstance(raw, list):
        if any(not isinstance(line, str) for line in raw): raise EnvironmentDecision('Invalid service environment')
        raw = dict(line.split('=', 1) if '=' in line else (line, None) for line in raw)
    if not isinstance(raw, dict) or len(raw) > 256 or any(not isinstance(key, str) or not re.fullmatch('[A-Za-z_][A-Za-z0-9_]*', key) for key in raw):
        raise EnvironmentDecision('Invalid service environment')
    return raw


def expand(value, values):
    if not isinstance(value, (str, int, float, bool)):
        raise EnvironmentDecision('Environment values must be literals')
    value = str(value)
    def replace(match):
        name = match[1] or match[4]
        if name in values and (values[name] or match[2] != ':-'):
            return values[name]
        if match[2] in ('-', ':-'):
            return match[3]
        raise EnvironmentDecision('Supply test environment variable: ' + name)
    return VARIABLE.sub(replace, value.replace('$$', '\x01')).replace('\x01', '$')


def plan(work, repositories, supplied=None, entrypoints=None):
    """Pure discovery/validation, with all ambiguities resolved BEFORE starting anything."""
    work = Path(work).resolve()
    supplied = dict(supplied or {})
    files = []
    if entrypoints:
        files = [inside(work, path) for path in entrypoints]
    else:
        for repo in repositories:
            root = inside(work, repo['path'])
            found = [root / name for name in COMPOSE if (root / name).is_file()]
            if len(found) > 1:
                raise EnvironmentDecision('Choose one Compose entrypoint in ' + repo['path'])
            if found: files.extend(found)
            elif (root / 'Dockerfile').is_file(): files.append(root / 'Dockerfile')
            elif (root / 'package.json').is_file():
                try: scripts = json.loads((root / 'package.json').read_text()).get('scripts', {})
                except (ValueError, AttributeError): raise EnvironmentDecision('Invalid Node manifest: ' + repo['path']) from None
                if isinstance(scripts, dict) and any(isinstance(scripts.get(name), str) and scripts[name] for name in ('start', 'dev')):
                    files.append(root / 'package.json')
    if not files:
        raise EnvironmentDecision('No Compose, Dockerfile or runnable package manifest found')
    services = {}
    for path in files:
        inside(work, str(path.relative_to(work)))
        if path.name in COMPOSE:
            compose = load_compose(path)
            for name, source in compose['services'].items():
                if not isinstance(name, str) or not NAME.fullmatch(name):
                    raise EnvironmentDecision('Invalid Compose service name')
                if not isinstance(source, dict):
                    raise EnvironmentDecision('Invalid Compose service')
                supported = {'image', 'build', 'command', 'entrypoint', 'working_dir', 'environment', 'depends_on', 'healthcheck', 'ports', 'expose', 'volumes', 'networks', 'restart', 'labels', 'profiles'}
                if set(source) - supported or source.get('profiles'):
                    raise EnvironmentDecision('Unsupported/privileged Compose settings in service: ' + name)
                service = copy.deepcopy(source)
                service['_root'] = str(path.parent.relative_to(work))
                service['_manifest'] = str(path.relative_to(work))
                if name in services:
                    raise EnvironmentDecision('Ambiguous service name across manifests: ' + name)
                services[name] = service
        else:
            name = re.sub('[^a-zA-Z0-9_-]', '-', path.parent.name)
            if not NAME.fullmatch(name) or name in services:
                raise EnvironmentDecision('Choose explicit environment entrypoints to disambiguate applications')
            services[name] = {'build': '.' if path.name == 'Dockerfile' else None, '_node': path.name == 'package.json', '_root': str(path.parent.relative_to(work)), '_manifest': str(path.relative_to(work))}
    if not 1 <= len(services) <= 16 or any(not NAME.fullmatch(name) for name in services):
        raise EnvironmentDecision('Select between 1 and 16 safely named services')
    # Generate disposable DB identities, never discover a production password.
    values = dict(supplied)
    database = [name for name, s in services.items() if re.match(r'^(?:docker.io/)?(?:library/)?postgres[:@]', str(s.get('image', '')))]
    if len(database) == 1:
        values.setdefault('POSTGRES_USER', 'nexusmind_test')
        values.setdefault('POSTGRES_DB', 'nexusmind_test')
        values.setdefault('POSTGRES_PASSWORD', secrets.token_hex(24))
        declared = env_pairs(services[database[0]].get('environment') or {})
        for key in ('POSTGRES_USER', 'POSTGRES_PASSWORD', 'POSTGRES_DB'):
            if key in declared:
                raw = declared[key]
                variable = VARIABLE.fullmatch(str(raw)) if raw is not None else None
                if variable and variable[2] not in ('-', ':-') and (variable[1] or variable[4]) not in values:
                    values[variable[1] or variable[4]] = values[key]
                values[key] = expand(raw if raw is not None else '${' + key + '}', values)
        values.setdefault('DATABASE_URL', 'postgresql://' + quote(values['POSTGRES_USER'], safe='') + ':' + quote(values['POSTGRES_PASSWORD'], safe='') + '@' + database[0] + ':5432/' + quote(values['POSTGRES_DB'], safe=''))
    redis = [name for name, s in services.items() if re.match(r'^(?:docker.io/)?(?:library/)?redis[:@]', str(s.get('image', '')))]
    if len(redis) == 1: values.setdefault('REDIS_URL', 'redis://' + redis[0] + ':6379/0')
    for name, service in services.items():
        root = inside(work, service['_root'])
        raw = env_pairs(service.get('environment') or {})
        service['_env'] = {key: expand(value if value is not None else '${' + key + '}', values) for key, value in raw.items()}
        if service.get('_node'):
            for key in ('DATABASE_URL', 'REDIS_URL'):
                if key in values: service['_env'].setdefault(key, values[key])
        if name in database:
            for key in ('POSTGRES_USER', 'POSTGRES_PASSWORD', 'POSTGRES_DB'):
                service['_env'].setdefault(key, values[key])
        if Path(service['_manifest']).name not in COMPOSE or service.get('_node'):
            service['_env'].update(supplied)
        for value in service['_env'].values():
            for url in re.findall(r'[a-zA-Z][a-zA-Z0-9+.-]*://[^\s]+', value):
                if urlsplit(url).hostname not in set(services) | {'localhost', '127.0.0.1', '::1'}:
                    raise EnvironmentDecision('External dependency requires an explicit reviewed service profile: ' + name)
        service['_command'] = argv(service.get('command'))
        service['_entrypoint'] = argv(service.get('entrypoint'))
        for command in (service['_command'], service['_entrypoint']):
            for index, value in enumerate(command):
                if VARIABLE.search(value.replace('$$', '')):
                    raise EnvironmentDecision('Use container environment variables ($$VAR), not command argument secrets: ' + name)
                command[index] = value.replace('$$', '$')
        health = service.get('healthcheck')
        if health:
            if not isinstance(health, dict) or set(health) - {'test', 'interval', 'timeout', 'retries', 'start_period', 'start_interval', 'disable'}:
                raise EnvironmentDecision('Invalid Compose healthcheck: ' + name)
            if isinstance(health.get('test'), list):
                health['test'] = [str(value).replace('$$', '$') for value in health['test']]
        service['_mounts'], service['_tmpfs'] = [], []
        for volume in service.get('volumes', []):
            if not isinstance(volume, str):
                raise EnvironmentDecision('Long-form volumes need an explicit profile: ' + name)
            parts = volume.split(':')
            target = parts[1] if len(parts) > 1 else parts[0]
            if not target.startswith('/') or '..' in Path(target).parts or target in ('/', '/etc', '/proc', '/sys', '/dev', '/run', '/var/run', '/home') or 'docker.sock' in target or ',' in target:
                raise EnvironmentDecision('Unsafe volume target: ' + name)
            if len(parts) > 1 and (parts[0].startswith(('.', '/')) or '/' in parts[0]):
                source = relative_source(work, root, parts[0])
                service['_mounts'].append((str(source), target))
            else:
                service['_tmpfs'].append(target)
        if name in database:
            service['_postgres'] = True
            service['_env'].setdefault('PGDATA', '/var/lib/postgresql/data/nexusmind')
            service['_tmpfs'] += ['/var/lib/postgresql/data', '/var/run/postgresql']
        if name in redis:
            service['_tmpfs'].append('/data')
        build = service.get('build')
        if build:
            build = {'context': build} if isinstance(build, str) else build
            if not isinstance(build, dict) or set(build) - {'context', 'dockerfile', 'target'}:
                raise EnvironmentDecision('Build args/secrets/SSH/additional contexts require a reviewed profile: ' + name)
            context = relative_source(work, root, build.get('context', '.'))
            dockerfile = inside(context, build.get('dockerfile', 'Dockerfile'))
            service['_build'] = {'context': str(context), 'dockerfile': str(dockerfile), 'target': build.get('target')}
        elif not service.get('image') and not service.get('_node'):
            raise EnvironmentDecision('Service has neither image nor build: ' + name)
        dependencies = service.get('depends_on', [])
        if not isinstance(dependencies, (list, dict)) or any(dep not in services for dep in dependencies):
            raise EnvironmentDecision('Unresolved service dependency: ' + name)
        service['_dependencies'] = list(dependencies)
        if service.get('_node'):
            service['_dependencies'] += database + redis
        service['_redactions'] = list(values.values()) + list(service['_env'].values())
        for value in list(service['_redactions']):
            for url in re.findall(r'[a-zA-Z][a-zA-Z0-9+.-]*://[^\s]+', value):
                if urlsplit(url).password: service['_redactions'].append(urlsplit(url).password)
    from graphlib import TopologicalSorter, CycleError
    try:
        ordered = list(TopologicalSorter({name: service['_dependencies'] for name, service in services.items()}).static_order())
    except CycleError:
        raise EnvironmentDecision('Cyclic service dependencies need an explicit decision') from None
    return {name: services[name] for name in ordered}


def resolve_image(config, reference, job=None, api=None):
    runtime = runtime_command(config)
    client_env = runtime_environment(config, job['run_id']) if job else {'PATH': os.defpath, 'HOME': '/nonexistent', 'DOCKER_CONFIG': '/nonexistent'}
    if reference in config.get('allowed_images', []):
        pinned_image(reference)
    else:
        # Public, operator-allowlisted repositories only. No arbitrary registry/SSRF.
        match = re.fullmatch(r'(?:docker.io/)?(?:library/)?([a-z0-9-]+)(?::[a-zA-Z0-9_.-]+|@sha256:[0-9a-f]{64})', reference or '')
        if not match or match[1] not in config.get('application_image_repositories', sorted(OFFICIAL)):
            raise EnvironmentDecision('Authorize an external image repository explicitly')
        if job is None:
            subprocess.run([runtime, 'pull', reference], check=True, capture_output=True, timeout=240, env=client_env)
        else:
            log = Path(config['state_dir']) / job['run_id'] / ('image-pull-' + hashlib.sha256(reference.encode()).hexdigest()[:12] + '.log')
            if monitor([runtime, 'pull', reference], config, job, api, None, log) != 0:
                raise EnvironmentDecision('Approved public image could not be prepared')
    metadata = json.loads(subprocess.check_output([runtime, 'image', 'inspect', reference], timeout=15, env=client_env))[0]
    if metadata.get('Config', {}).get('OnBuild'):
        raise EnvironmentDecision('ONBUILD base images require an explicit profile')
    return pinned_image(metadata['Id']), metadata


def build_cgroup():
    """Use the runner's existing bounded systemd cgroup, not ignored Buildx hints.

    Docker's embedded BuildKit can silently ignore per-step --resource options.
    --cgroup-parent places every RUN underneath this aggregate memory/PID limit.
    """
    try:
        group = next(line[3:] for line in Path('/proc/self/cgroup').read_text().splitlines() if line.startswith('0::'))
        if group == '/' or not group.startswith('/') or not group.endswith(('.service', '.scope')):
            raise ValueError()
        root = Path('/sys/fs/cgroup') / group.lstrip('/')
        memory = int((root / 'memory.max').read_text())
        pids = int((root / 'pids.max').read_text())
        if not 64 * 1024 * 1024 <= memory <= 4 * 1024 ** 3 or not 32 <= pids <= 4096:
            raise ValueError()
        return group
    except (OSError, ValueError, StopIteration):
        raise EnvironmentDecision('Run the runner in a bounded systemd service/scope (MemoryMax up to 4G, TasksMax up to 4096)') from None


def local_base(config, image, job=None):
    # BuildKit treats bare sha256 IDs as registry names. This runner-owned tag is
    # derived only from the inspected immutable ID, never from repository input.
    pinned_image(image)
    tag = 'nexusmind-base:' + image.rsplit('sha256:', 1)[1]
    subprocess.run([runtime_command(config), 'tag', image, tag], check=True, capture_output=True, timeout=15,
                   env=runtime_environment(config, job['run_id']) if job else {'PATH': os.defpath, 'HOME': '/nonexistent', 'DOCKER_CONFIG': '/nonexistent'})
    return tag


def dockerfile(config, text, job=None, api=None):
    if len(text) > 128 * 1024:
        raise EnvironmentDecision('Dockerfile exceeds the size budget')
    if re.search(r'^\s*#\s*(syntax|escape)\s*=', text.lstrip('\ufeff'), re.M | re.I):
        raise EnvironmentDecision('Custom Dockerfile frontends/escapes are not authorized')
    result, stages, args = [], set(), {}
    stage_count = 0
    logical = text.lstrip('\ufeff').replace('\\\n', '')
    for line in logical.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith('#'):
            result.append(line); continue
        tokens = stripped.split(None, 1)
        instruction, body = tokens[0].upper(), tokens[1] if len(tokens) > 1 else ''
        if instruction in ('ADD', 'ONBUILD') or '<<' in body or re.search(r'--(network|security|mount)(?:\s|=)', body, re.I):
            raise EnvironmentDecision('Dockerfile remote additions/privileged mounts are not authorized')
        if instruction == 'ARG':
            key, sep, value = body.partition('=')
            if not sep:
                raise EnvironmentDecision('Declare a non-secret Dockerfile ARG default: ' + key)
            args[key] = value
        if instruction == 'FROM':
            stage_count += 1
            if stage_count > 4: raise EnvironmentDecision('More than four build stages need a reviewed profile')
            parts = shlex.split(body)
            if len(parts) not in (1, 3) or len(parts) == 3 and parts[1].upper() != 'AS':
                raise EnvironmentDecision('Unsupported Dockerfile FROM')
            base = expand(parts[0], args)
            if base != 'scratch' and base.lower() not in stages:
                base, _ = resolve_image(config, base, job, api)
                base = local_base(config, base, job)
            if len(parts) == 3:
                stages.add(parts[2].lower())
            line = 'FROM ' + base + (' AS ' + parts[2] if len(parts) == 3 else '')
        if instruction == 'COPY':
            source = re.search(r'--from=([^\s]+)', body)
            if '--from' in body and not source or source and source[1].lower() not in stages and not source[1].isdigit():
                raise EnvironmentDecision('COPY may reference only local build stages')
        result.append(line)
    return '\n'.join(result) + '\n'


def node_build(config, job, work, run_root, name, service, api):
    from execution import copy_source
    from node_environment import prepare as dependencies
    root = inside(Path(work), service['_root'])
    package = json.loads((root / 'package.json').read_text())
    scripts = package.get('scripts', {})
    script = 'start' if scripts.get('start') else 'dev' if scripts.get('dev') else None
    if not script or not isinstance(scripts[script], str):
        raise EnvironmentDecision('Choose a runnable application manifest: ' + service['_manifest'])
    text = scripts[script]
    explicit = re.search(r'--port[= ]+(\d+)', text)
    port = service['_env'].get('PORT') or package.get('config', {}).get('port') or (explicit[1] if explicit else 5173 if 'vite' in text else 3000 if 'next ' in text or 'react-scripts ' in text else None)
    if not str(port).isdigit() or not 1024 <= int(port) <= 65535:
        raise EnvironmentDecision('Declare the application test PORT: ' + service['_manifest'])
    sdk = config.get('managed_images', {}).get('agent') or config.get('managed_images', {}).get('node')
    if not sdk or sdk not in config.get('allowed_images', []):
        raise EnvironmentDecision('Managed Node SDK must be provisioned on this runner')
    context = run_root / ('application-node-' + name)
    if context.exists(): shutil.rmtree(context)
    context.mkdir(mode=0o700)
    copy_source(root, context / 'app')
    dependencies(work, [{'cwd': service['_root']}], context / 'packages', Path(config['state_dir']) / 'node-package-cache',
                 heartbeat=lambda: api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/lease', {'lease_token': job['lease_token']}), timeout=job['step']['timeout_seconds'])
    shutil.copyfile(Path(__file__).with_name('application_node.py'), context / 'bootstrap.py')
    recipe = context / 'Dockerfile'
    command = ['npm', 'run', script]
    if 'vite' in text: command += ['--', '--host', '0.0.0.0']
    recipe.write_text('FROM ' + local_base(config, sdk, job) + '\nCOPY --chown=1000:1000 app /app\nCOPY --chown=1000:1000 packages /nm-packages\nCOPY bootstrap.py /nm-bootstrap.py\nWORKDIR /app\nUSER 1000:1000\nRUN ["python3", "/nm-bootstrap.py"]\nENTRYPOINT ["python3", "/nm-bootstrap.py", "start"]\nEXPOSE ' + str(port) + '\nCMD ' + json.dumps(command) + '\n')
    service['_env'].setdefault('PORT', str(port))
    service['_env'].setdefault('HOST', '0.0.0.0')
    service['_has_seed'] = 'test:seed' in scripts
    service['_build'] = {'context': str(context), 'dockerfile': str(recipe)}
    service['_generated'] = True


def prepare(config, job, work, api, roots, private_file):
    if not config.get('allow_application_builds'):
        raise EnvironmentDecision('Runner operator must authorize isolated application builds once')
    if runtime_command(config) != 'docker':
        raise EnvironmentDecision('Automatic application builds require Docker; use reviewed profiles for other runtimes')
    cgroup = build_cgroup()
    workspace = job['snapshot']['workspace']['id']
    from execution import environment
    refs = ['application'] if 'application' in config.get('environments', {}).get(workspace, {}) else []
    files, redactions = environment(config, workspace, refs, roots, private_file)
    supplied = {}
    for filename in files:
        for line in private_file(filename).splitlines():
            if line.strip() and not line.lstrip().startswith('#'):
                key, value = line.split('=', 1); supplied[key.strip()] = value
    runtime_config = job['snapshot']['workspace'].get('runtime', {}).get('config', {})
    repositories = job['snapshot']['workspace']['repositories']
    changed = [repo for repo in repositories if job['execution']['heads'][repo['path']] != repo['head_sha']]
    # Do not start every application in a 49-repository workspace for one change.
    # Explicit entrypoints can select an unchanged consumer/integration topology.
    services = plan(work, changed or repositories, supplied, runtime_config.get('environment_entrypoints'))
    runtime = runtime_command(config)
    run_root = Path(config['state_dir']) / job['run_id']
    images = []
    # Persist ownership before each build, so failures/cancellations are cleanable.
    marker = run_root / 'application-images.json'
    if marker.exists():
        raise EnvironmentDecision('Previous application image ownership must be cleaned first')
    marker.write_text(json.dumps({'images': [], 'tags': [], 'created_at': time.time()}))
    try:
        for name, service in services.items():
            if service.get('_node'):
                node_build(config, job, work, run_root, name, service, api)
            build = service.get('_build')
            if build:
                recipe = run_root / ('Dockerfile-' + name)
                text = Path(build['dockerfile']).read_text()
                recipe.write_text(text if service.get('_generated') else dockerfile(config, text, job, api))
                tag = 'nexusmind-app-' + job['run_id'] + '-' + name
                ownership = json.loads(marker.read_text()); ownership['tags'].append(tag); marker.write_text(json.dumps(ownership))
                command = [runtime, 'build', *(['--builder', 'default'] if runtime == 'docker' else []), '--network=none', '--pull=false', '--force-rm', '--label', 'nexusmind.run=' + job['run_id'], '--tag', tag, '--file', str(recipe)]
                if build.get('target'):
                    if not NAME.fullmatch(build['target']): raise EnvironmentDecision('Invalid Dockerfile target')
                    command += ['--target', build['target']]
                command += ['--cgroup-parent', cgroup, '--ulimit', 'nofile=1024:1024', '--ulimit', 'fsize=268435456:268435456', build['context']]
                if shutil.disk_usage(run_root).free < 1024 * 1024 * 1024:
                    raise EnvironmentDecision('Application build needs at least 1 GiB free disk')
                log = run_root / ('build-' + name + '.log')
                build_redactions = redactions + service['_redactions']
                from execution import redact, upload
                status = monitor(command, config, job, api, None, log, build_redactions)
                upload(api, job, 'application-build-' + name + '.log', 'text/plain', redact(log.read_text(errors='replace')[-512 * 1024:], build_redactions).encode()[:1024 * 1024])
                if status != 0:
                    raise EnvironmentDecision('Candidate build failed; inspect the redacted build log (build egress is disabled)')
                metadata = json.loads(subprocess.check_output([runtime, 'image', 'inspect', tag], timeout=15, env=runtime_environment(config, job['run_id'])))[0]
                image = pinned_image(metadata['Id']); images.append(image)
                ownership = json.loads(marker.read_text()); ownership['images'] = images; marker.write_text(json.dumps(ownership))
            else:
                image, metadata = resolve_image(config, service['image'], job, api)
            service['_image'] = image
            service['_metadata'] = metadata.get('Config') or {}
            service['_env_file'] = str(run_root / ('environment-' + name))
            descriptor = os.open(service['_env_file'], os.O_CREAT | os.O_TRUNC | os.O_WRONLY, 0o600)
            os.fchmod(descriptor, 0o600)
            with os.fdopen(descriptor, 'w') as output:
                for key, value in service['_env'].items():
                    if '\n' in value or '\r' in value or '\x00' in value: raise EnvironmentDecision('Multiline environment values require a reviewed profile')
                    output.write(key + '=' + value + '\n')
        return services
    except BaseException:
        cleanup(config, job['run_id'])
        raise


def health_command(config, container, network, name, service):
    test = (service.get('healthcheck') or {}).get('test') or service['_metadata'].get('Healthcheck', {}).get('Test')
    if test and test != ['NONE']:
        if isinstance(test, str): test = ['CMD-SHELL', test]
        if not isinstance(test, list) or len(test) < 2 or any(not isinstance(v, str) for v in test):
            raise EnvironmentDecision('Invalid healthcheck: ' + name)
        if test[0] == 'CMD': command = test[1:]
        elif test[0] == 'CMD-SHELL': command = ['sh', '-c', test[1]]
        else: raise EnvironmentDecision('Unsupported healthcheck: ' + name)
        return [runtime_command(config), 'exec', container, *command]
    if service.get('_postgres'):
        return [runtime_command(config), 'exec', container, 'pg_isready', '-U', service['_env']['POSTGRES_USER'], '-d', service['_env']['POSTGRES_DB']]
    ports = service.get('expose') or service.get('ports') or list(service['_metadata'].get('ExposedPorts', {}))
    found = set()
    for port in ports or []:
        port = port.get('target') if isinstance(port, dict) else str(port).split(':')[-1].split('/')[0]
        if str(port).isdigit() and 1 <= int(port) <= 65535: found.add(int(port))
    if len(found) != 1:
        raise EnvironmentDecision('Declare one healthcheck or exposed test port: ' + name)
    sdk = config.get('managed_images', {}).get('agent')
    if not sdk or sdk not in config.get('allowed_images', []):
        raise EnvironmentDecision('Provision the managed SDK for isolated TCP health probes')
    from execution import restrictions
    return [runtime_command(config), 'run', '--rm', '--name', network + '-health-' + uuid.uuid4().hex[:12], *restrictions(), '--network', network, '--entrypoint', 'python3', pinned_image(sdk), '-c',
            'import socket; socket.create_connection((' + repr(name) + ',' + str(next(iter(found))) + '), 2).close()']


def startup_log(config, job, container, alias, service, api):
    from execution import redact, upload
    result = subprocess.run([runtime_command(config), 'logs', '--tail', '200', container], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=15, env=runtime_environment(config, job['run_id']))
    name = ('test-seed-' if service.get('_has_seed') else 'application-startup-') + alias + '.log'
    upload(api, job, name, 'text/plain', redact(result.stdout.decode(errors='replace')[-512 * 1024:], service['_redactions']).encode()[:1024 * 1024])


def cleanup(config, run):
    root = Path(config['state_dir']) / run
    marker = root / 'application-images.json'
    if marker.exists():
        runtime = runtime_command(config)
        ownership = json.loads(marker.read_text())
        for reference in ownership.get('tags', []) + ownership.get('images', []):
            # Only objects carrying this run's label may be removed.
            result = subprocess.run([runtime, 'image', 'inspect', reference], capture_output=True, timeout=15, env=runtime_environment(config, run))
            if result.returncode == 0:
                metadata = json.loads(result.stdout)[0]
                if (metadata.get('Config', {}).get('Labels') or {}).get('nexusmind.run') == run:
                    subprocess.run([runtime, 'image', 'rm', reference], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30, env=runtime_environment(config, run))
        marker.unlink(missing_ok=True)
    for filename in root.glob('environment-*'):
        filename.unlink(missing_ok=True)
    for directory in root.glob('application-node-*'):
        if directory.is_symlink(): directory.unlink()
        elif directory.is_dir(): shutil.rmtree(directory)
