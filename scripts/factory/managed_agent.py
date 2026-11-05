"""Provision only the operator-installed SDK recipe, never repository Dockerfiles."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


def provision(config):
    if not config.get('auto_provision_agent_runtime'):
        return
    from execution import pinned_image, runtime_command
    runtime = runtime_command(config)
    recipe = Path(__file__).with_name('Dockerfile.agent').resolve(strict=True)
    info = recipe.stat()
    import os
    if info.st_uid not in (0, os.getuid()) or info.st_mode & 0o022:
        raise ValueError('SDK recipe must be operator-installed and non-writable')
    digest = hashlib.sha256(recipe.read_bytes()).hexdigest()
    tag = 'nexusmind-managed-agent:' + digest[:24]
    result = subprocess.run([runtime, 'image', 'inspect', tag], capture_output=True, timeout=15)
    metadata = json.loads(result.stdout)[0] if result.returncode == 0 else None
    if not metadata or (metadata.get('Config', {}).get('Labels') or {}).get('org.nexusmind.sdk-recipe') != digest:
        if shutil.disk_usage(config['state_dir']).free < 4 * 1024 ** 3:
            raise ValueError('Insufficient disk for the managed SDK')
        # Empty build context: no checkout, credential, environment or auth store.
        with tempfile.TemporaryDirectory(prefix='nm-sdk-') as context:
            subprocess.run([runtime, 'build', '--pull=false', '--label', 'org.nexusmind.sdk-recipe=' + digest, '--tag', tag, '--file', str(recipe), context], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1200, check=True)
        metadata = json.loads(subprocess.check_output([runtime, 'image', 'inspect', tag], timeout=15))[0]
    image = pinned_image(metadata['Id'])
    config.setdefault('managed_images', {})['agent'] = image
    if image not in config.setdefault('allowed_images', []):
        config['allowed_images'].append(image)
