"""Fetch integrity-pinned npm tarballs (public, or private registries authorized by an
operator .npmrc). Never run repository code on the host."""
import base64
import hashlib
import json
import os
import shutil
from pathlib import Path
import tempfile
import time
import urllib.parse
import urllib.request

MAX_PACKAGE = 128 * 1024 * 1024
MAX_TOTAL = 2 * 1024 * 1024 * 1024

class NodeEnvironmentError(ValueError):
    def __str__(self):
        code=super().__str__()
        labels={
            'node_private_or_external_registry_requires_connector':'Hay dependencias de un registro privado o externo sin autorizar. Configura la autenticación npm (.npmrc o token) del repositorio en Workflows → Secretos por repositorio.',
            'node_npmrc_invalid':'El .npmrc del repositorio tiene un registro inválido: usa URLs https sin credenciales en la URL.',
            'node_committed_package_lock_required':'El entorno Node necesita package.json y package-lock.json versionados en el repositorio.',
            'node_lockfile_v2_or_v3_required':'El entorno automático admite package-lock v2/v3. Este lockfile necesita un perfil explícito.',
            'node_workspace_requires_explicit_profile':'Este workspace de paquetes necesita un perfil de instalación explícito; no se ejecutará un instalador improvisado.',
            'node_dependency_integrity_failed':'Una descarga no coincide con el hash del lockfile. Se bloquea la comprobación.',
        }
        return labels.get(code,code)

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise NodeEnvironmentError('node_registry_redirect_rejected')

class PrivateRedirect(urllib.request.HTTPRedirectHandler):
    """Private registries (GitHub Packages) answer downloads with a short-lived
    https redirect. Follow only https, a few hops, and never forward the
    registry credential: it is an unredirected header. The lockfile hash still
    decides what is accepted."""
    max_redirections = 3
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        parsed = urllib.parse.urlsplit(newurl)
        if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password:
            raise NodeEnvironmentError('node_registry_redirect_rejected')
        return super().redirect_request(req, fp, code, msg, headers, newurl)

PUBLIC = 'https://registry.npmjs.org/'

def _registry(url):
    parsed = urllib.parse.urlsplit(url.strip())
    if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise NodeEnvironmentError('node_npmrc_invalid')
    return url.strip().rstrip('/') + '/'

def parse_npmrc(text):
    """Registry routing and registry credentials from an operator-provided
    .npmrc. Never a repository file, never forwarded to a container."""
    config = {'default': PUBLIC, 'scopes': {}, 'auth': {}}
    for line in (text or '').splitlines():
        line = line.strip()
        if not line or line[0] in '#;' or '=' not in line:
            continue
        key, value = (part.strip() for part in line.split('=', 1))
        if key == 'registry':
            config['default'] = _registry(value)
        elif key.startswith('@') and key.endswith(':registry'):
            config['scopes'][key[:-len(':registry')].lower()] = _registry(value)
        elif key.startswith('//') and ':' in key:
            location, setting = key[2:].rsplit(':', 1)
            entry = config['auth'].setdefault(location.rstrip('/') + '/', {})
            entry[setting] = value
    return config

def authorization(config, url):
    """Longest //host/path/ match, as npm does. Returns a header value or None."""
    parsed = urllib.parse.urlsplit(url)
    target = (parsed.hostname or '') + (parsed.path or '/')
    best = None
    for location, entry in (config or {}).get('auth', {}).items():
        if target.startswith(location) and (best is None or len(location) > len(best[0])):
            best = (location, entry)
    if not best:
        return None
    entry = best[1]
    if entry.get('_authToken'):
        return 'Bearer ' + entry['_authToken']
    if entry.get('_auth'):
        return 'Basic ' + entry['_auth']
    if entry.get('username') and entry.get('_password'):
        password = base64.b64decode(entry['_password']).decode()
        return 'Basic ' + base64.b64encode((entry['username'] + ':' + password).encode()).decode()
    return None

def secrets(config):
    values = []
    for entry in (config or {}).get('auth', {}).values():
        values.extend(v for k, v in entry.items() if k in ('_authToken', '_auth', '_password'))
    return values

def _allowed_hosts(config):
    hosts = {'registry.npmjs.org'}
    for url in [config['default'], *config['scopes'].values()] if config else []:
        hosts.add(urllib.parse.urlsplit(url).hostname)
    return hosts

def _derived_url(path, package, config):
    """npm can omit `resolved` (omit-lockfile-registry-resolved). Rebuild the
    registry tarball URL from the name/version; the lockfile hash still binds it."""
    name = package.get('name') or path.rsplit('node_modules/', 1)[-1]
    version = package.get('version')
    if not name or not version:
        raise NodeEnvironmentError('node_sha512_integrity_required')
    scope = name.split('/', 1)[0].lower() if name.startswith('@') else None
    base = (config or {}).get('scopes', {}).get(scope) if scope else None
    base = base or (config or {}).get('default', PUBLIC)
    return base + name + '/-/' + name.rsplit('/', 1)[-1] + '-' + version + '.tgz'

def packages(lock, config=None):
    if lock.get('lockfileVersion') not in (2, 3) or not isinstance(lock.get('packages'), dict):
        raise NodeEnvironmentError('node_lockfile_v2_or_v3_required')
    if len(lock['packages']) > 5000:
        raise NodeEnvironmentError('node_dependency_limit_exceeded')
    result = {}
    allowed = _allowed_hosts(config)
    for path, package in lock['packages'].items():
        if not path:
            continue
        if package.get('link') or not path.startswith('node_modules/'):
            raise NodeEnvironmentError('node_workspace_requires_explicit_profile')
        if package.get('inBundle'):
            # Shipped inside its parent's verified tarball; never fetched alone.
            continue
        url = package.get('resolved') or _derived_url(path, package, config)
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme != 'https' or parsed.hostname not in allowed or parsed.port not in (None,443) or parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise NodeEnvironmentError('node_private_or_external_registry_requires_connector')
        # npm lockfiles carry Subresource Integrity, never trust just a URL.
        digest = next((s[7:] for s in package.get('integrity','').split() if s.startswith('sha512-')), None)
        try:
            expected = base64.b64decode(digest or '', validate=True)
        except ValueError:
            expected = b''
        if len(expected) != 64:
            raise NodeEnvironmentError('node_sha512_integrity_required')
        result[url] = (expected, package['integrity'])
    return result

def _repository_config(cwd, npmrc):
    """Operator .npmrc for the repository that owns a check directory."""
    owner = max((repo for repo in (npmrc or {}) if cwd == repo or cwd.startswith(repo.rstrip('/') + '/')), key=len, default=None)
    return parse_npmrc(npmrc[owner]) if owner else None

def prepare(work, checks, destination, cache, heartbeat=lambda: None, timeout=300, npmrc=None):
    manifests = []
    all_packages = {}
    configs = {}
    scopes = {}
    for check in checks:
        root = (Path(work) / check['cwd']).resolve()
        if root != Path(work).resolve() and Path(work).resolve() not in root.parents:
            raise NodeEnvironmentError('node_repository_invalid')
        package = root / 'package.json'; lockfile = root / 'package-lock.json'
        if not package.is_file() or not lockfile.is_file() or package.is_symlink() or lockfile.is_symlink():
            raise NodeEnvironmentError('node_committed_package_lock_required')
        if max(package.stat().st_size,lockfile.stat().st_size) > 16*1024*1024:
            raise NodeEnvironmentError('node_manifest_limit_exceeded')
        lock = json.loads(lockfile.read_text())
        config = _repository_config(check['cwd'], npmrc)
        found = packages(lock, config)
        for url in found:
            configs[url] = config
        if config:
            scopes.update(config['scopes'])
        all_packages.update(found)
        manifests.append(check['cwd'])
    cache = Path(cache); cache.mkdir(parents=True,exist_ok=True,mode=0o700)
    destination = Path(destination); destination.mkdir(mode=0o700)
    opener = urllib.request.build_opener(NoRedirect())
    private_opener = urllib.request.build_opener(PrivateRedirect())
    started = time.monotonic(); total = 0; files = []
    for url,(digest,integrity) in all_packages.items():
        heartbeat()
        if time.monotonic()-started > timeout:
            raise NodeEnvironmentError('node_dependency_timeout')
        name = digest.hex()+'.tgz'; path = cache/name
        if path.is_symlink():
            raise NodeEnvironmentError('node_cache_invalid')
        if path.exists() and (path.stat().st_size > MAX_PACKAGE or hashlib.sha512(path.read_bytes()).digest()!=digest):
            path.unlink()
        if path.exists():
            os.utime(path)  # Retention evicts only tarballs no recent step reused.
        if not path.exists():
            # Shared content cache; runner.prune_checkouts evicts unused versions.
            if shutil.disk_usage(cache).free < MAX_PACKAGE + 1024*1024*1024:
                raise NodeEnvironmentError('node_disk_reserve_required')
            request = urllib.request.Request(url,headers={'User-Agent':'NexusMind-locked-dependencies'})
            public = urllib.parse.urlsplit(url).hostname == 'registry.npmjs.org'
            header = None if public else authorization(configs.get(url), url)
            if header:
                request.add_unredirected_header('Authorization', header)
            with (opener if public else private_opener).open(request,timeout=min(30,max(1,timeout-(time.monotonic()-started)))) as response:
                data = response.read(MAX_PACKAGE+1)
            if len(data)>MAX_PACKAGE or hashlib.sha512(data).digest()!=digest:
                raise NodeEnvironmentError('node_dependency_integrity_failed')
            with tempfile.NamedTemporaryFile(dir=cache,delete=False) as temp:
                temp.write(data); temporary=Path(temp.name)
            temporary.replace(path)
        total += path.stat().st_size
        if total > MAX_TOTAL:
            raise NodeEnvironmentError('node_dependency_size_limit_exceeded')
        # Same-filesystem hardlinks expose no host path and avoid duplicate tarballs.
        os.link(path,destination/name)
        files.append({'file':name,'url':url,'integrity':integrity})
    # Scope routing only (no credentials) so offline npm resolves private names.
    (destination/'manifest.json').write_text(json.dumps({'packages':files,'repositories':sorted(set(manifests)),'scopes':scopes}))
