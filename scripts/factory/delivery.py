"""Integrated services, evidence gate and credential-free publication payloads."""
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time
import urllib.error
from execution import (restrictions, runtime_command, network_name, cleanup, pinned_image,
                       environment, stage_sources, upload, log_event, tree_fingerprint, check_tree)


def services(config, job, roots, api, git, private_file):
    profile = config.get('workspace_profiles', {}).get(job['snapshot']['workspace']['id'], {})
    configured = profile.get('services', {})
    if not configured:
        raise ValueError('Configure integrated services for this workspace')
    cleanup(config, job['run_id'], config['state_dir'])
    working, work = stage_sources(job, config['state_dir'], git)
    runtime = runtime_command(config)
    network = network_name(job['run_id'])
    subprocess.run([runtime, 'network', 'create', '--internal', '--label', 'nexusmind.run=' + job['run_id'], network], check=True, capture_output=True, timeout=30)
    manifest_path = Path(config['state_dir']) / job['run_id'] / 'services.json'
    manifest = {'containers': [], 'revision': job['execution']['revision'], 'work': str(work), 'fingerprint': tree_fingerprint(work), 'created_at': time.time()}
    manifest_path.write_text(json.dumps(manifest))
    try:
        for alias, service in configured.items():
            if not re.fullmatch(r'[a-zA-Z0-9_-]{1,40}', alias):
                raise ValueError('Invalid service alias')
            name = network + '-' + alias
            manifest['containers'].append(name)
            manifest_path.write_text(json.dumps(manifest))
            refs, _ = environment(config, job['snapshot']['workspace']['id'], service.get('environment_refs', []), roots, private_file)
            cwd = service.get('cwd', '')
            if cwd.startswith('/') or '..' in Path(cwd).parts:
                raise ValueError('Invalid service cwd')
            command = [runtime, 'run', '-d', '--pull=never', '--name', name, *restrictions(),
                       '--network', network, '--network-alias', alias, '--workdir', '/work/' + cwd,
                       '--mount', 'type=bind,src=' + str(work) + ',dst=/work']
            if runtime=='podman': command += ['--userns=keep-id']
            for path in refs:
                command += ['--env-file', str(path)]
            command += [pinned_image(service['image']), *service['command']]
            subprocess.run(command, check=True, capture_output=True, timeout=60)
            health = service.get('healthcheck')
            if not isinstance(health, list) or not health:
                raise ValueError('Every integrated service needs an executable healthcheck')
            deadline = time.monotonic() + min(120, job['step']['timeout_seconds'])
            while True:
                api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/lease', {'lease_token': job['lease_token']})
                check = subprocess.run([runtime, 'exec', name, *health], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
                if check.returncode == 0:
                    break
                if time.monotonic() > deadline:
                    raise ValueError('Integrated service failed its healthcheck')
                time.sleep(2)
            log_event(api, job, 'service.ready', alias)
        validate_services(config, job)
    except BaseException:
        cleanup(config, job['run_id'], config['state_dir'])
        raise
    report = {'services': list(configured), 'revision': job['execution']['revision'], 'heads': job['execution']['heads']}
    a = upload(api, job, 'integrated-environment.json', 'application/json', json.dumps(report, sort_keys=True).encode())
    return True, [{'check': 'environment', 'status': 'pass', 'commits': job['execution']['heads'], 'artifact_digest': a['digest']}], outcome(job, report)


def validate_services(config, job):
    manifest_path = Path(config['state_dir']) / job['run_id'] / 'services.json'
    if not manifest_path.exists():
        raise ValueError('Integrated environment is not running')
    manifest = json.loads(manifest_path.read_text())
    if manifest['revision'] != job['execution']['revision'] or time.time() - manifest['created_at'] > 7200:
        raise ValueError('Integrated environment revision/age is stale')
    # Generated build output is not committed source. Every original input must
    # remain byte/mode-identical; modifications or deletions invalidate services.
    check_tree(Path(manifest['work']))
    current = {k: list(v) for k, v in tree_fingerprint(Path(manifest['work'])).items()}
    expected = {k: list(v) for k, v in manifest['fingerprint'].items()}
    if {k:v for k,v in current.items() if k in expected} != expected:
        raise ValueError('Service source changed after the pinned environment was prepared')


def outcome(job, result):
    return {'input_revision': job['execution']['revision'], 'heads': job['execution']['heads'], 'output': result, 'cost_usd': 0}


def gate(job, api):
    heads = job['execution']['heads']
    for name in job['step']['depends_on']:
        result = job['dependency_results'].get(name)
        if not result or result.get('heads') != heads or not result.get('evidence'):
            raise ValueError('Gate dependency has missing or stale evidence')
        if any(e['status'] not in ('pass', 'not_applicable') or e['commits'] != heads for e in result['evidence']):
            raise ValueError('Gate dependency failed')
    report = {'verified': True, 'heads': heads, 'dependencies': job['step']['depends_on']}
    a = upload(api, job, 'verification-gate.json', 'application/json', json.dumps(report, sort_keys=True).encode())
    return True, [{'check': 'gate', 'status': 'pass', 'commits': heads, 'artifact_digest': a['digest']}], outcome(job, report)


def publication_payload(config, job, git):
    repositories = []
    total = 0
    for repo in job['snapshot']['workspace']['repositories']:
        source = Path(config['state_dir']) / job['run_id'] / repo['path']
        head = job['execution']['heads'][repo['path']]
        if head == repo['head_sha']:
            continue
        if git(['rev-parse', 'HEAD'], source) != head:
            raise ValueError('Local publication head differs from verified head')
        shas = git(['rev-list', '--reverse', repo['head_sha'] + '..' + head], source).splitlines()
        if len(shas) > 10:
            raise ValueError('Publication commit budget exceeded')
        commits = []
        for sha in shas:
            raw = git(['cat-file', 'commit', sha], source)
            header, message = raw.split('\n\n', 1)
            parents = [l.split(' ', 1)[1] for l in header.splitlines() if l.startswith('parent ')]
            if len(parents) != 1:
                raise ValueError('Only linear factory-authored commits may be published')
            if not all(l in header for l in ['author NexusMind <factory@nexusmind.local> 946684800 +0000', 'committer NexusMind <factory@nexusmind.local> 946684800 +0000']):
                raise ValueError('Publication commit metadata is not reproducible')
            tree = git(['rev-parse', sha + '^{tree}'], source)
            # -z avoids ambiguity from tabs/newlines in file names. Such paths are
            # rejected below, not interpreted as commands or external URLs.
            paths = git(['diff-tree', '--no-commit-id', '--name-only', '-r', parents[0], sha], source).splitlines()
            files = []
            for path in paths:
                if not path or path.startswith('/') or any(ord(c) < 32 for c in path) or '..' in Path(path).parts or path.startswith('"'):
                    raise ValueError('Unsupported publication path')
                listing = git(['ls-tree', sha, '--', path], source)
                if not listing:
                    files.append({'path': path, 'mode': '100644', 'content_base64': None})
                    continue
                mode, kind, blob = listing.split('\t', 1)[0].split(' ')
                if kind != 'blob' or mode not in ('100644', '100755', '120000'):
                    raise ValueError('Submodules/devices cannot be published')
                data = git(['cat-file', 'blob', blob], source, binary=True)
                total += len(data)
                if total > 1024 * 1024:
                    raise ValueError('Publication data budget exceeded')
                files.append({'path': path, 'mode': mode, 'content_base64': base64.b64encode(data).decode()})
            commits.append({'sha': sha, 'tree_sha': tree, 'parent_sha': parents[0], 'message': message + '\n', 'files': files})
        repositories.append({'path': repo['path'], 'commits': commits})
    return {'lease_token': job['lease_token'], 'revision': job['execution']['revision'], 'repositories': repositories}


def publish(config, job, api, git):
    payload = publication_payload(config, job, git)
    response = api.call('/runs/' + job['run_id'] + '/steps/' + job['step']['id'] + '/publish', payload, timeout=max(30,min(job['step']['timeout_seconds'],job['lease_until']-int(time.time()))))
    a = upload(api, job, 'pull-requests.json', 'application/json', json.dumps(response).encode())
    return True, [{'check': 'publication', 'status': 'pass', 'commits': job['execution']['heads'], 'artifact_digest': a['digest']}], outcome(job, response)
