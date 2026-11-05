"""Fetch integrity-pinned public npm tarballs. Never run repository code on the host."""
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
            'node_private_or_external_registry_requires_connector':'Hay dependencias de un registro privado o externo. Falta autorizar una conexión de lectura de paquetes; no se usarán credenciales personales o de producción.',
            'node_committed_package_lock_required':'El entorno Node necesita package.json y package-lock.json versionados en el repositorio.',
            'node_lockfile_v2_or_v3_required':'El entorno automático admite package-lock v2/v3. Este lockfile necesita un perfil explícito.',
            'node_workspace_requires_explicit_profile':'Este workspace de paquetes necesita un perfil de instalación explícito; no se ejecutará un instalador improvisado.',
            'node_dependency_integrity_failed':'Una descarga no coincide con el hash del lockfile. Se bloquea la comprobación.',
        }
        return labels.get(code,code)

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise NodeEnvironmentError('node_registry_redirect_rejected')

def packages(lock):
    if lock.get('lockfileVersion') not in (2, 3) or not isinstance(lock.get('packages'), dict):
        raise NodeEnvironmentError('node_lockfile_v2_or_v3_required')
    if len(lock['packages']) > 5000:
        raise NodeEnvironmentError('node_dependency_limit_exceeded')
    result = {}
    for path, package in lock['packages'].items():
        if not path:
            continue
        if package.get('link') or not path.startswith('node_modules/'):
            raise NodeEnvironmentError('node_workspace_requires_explicit_profile')
        url = package.get('resolved', '')
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme != 'https' or parsed.hostname != 'registry.npmjs.org' or parsed.port not in (None,443) or parsed.username or parsed.password or parsed.query or parsed.fragment:
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

def prepare(work, checks, destination, cache, heartbeat=lambda: None, timeout=300):
    manifests = []
    all_packages = {}
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
        all_packages.update(packages(lock))
        manifests.append(check['cwd'])
    cache = Path(cache); cache.mkdir(parents=True,exist_ok=True,mode=0o700)
    destination = Path(destination); destination.mkdir(mode=0o700)
    opener = urllib.request.build_opener(NoRedirect())
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
        if not path.exists():
            # ponytail: shared content cache, reserve host disk; add eviction if versions accumulate.
            if shutil.disk_usage(cache).free < MAX_PACKAGE + 1024*1024*1024:
                raise NodeEnvironmentError('node_disk_reserve_required')
            request = urllib.request.Request(url,headers={'User-Agent':'NexusMind-locked-dependencies'})
            with opener.open(request,timeout=min(30,max(1,timeout-(time.monotonic()-started)))) as response:
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
    (destination/'manifest.json').write_text(json.dumps({'packages':files,'repositories':sorted(set(manifests))}))
