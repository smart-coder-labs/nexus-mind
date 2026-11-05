"""Trusted SDK build helper. Runs INSIDE the image build, never on the runner."""
import json
import os
import sys
from pathlib import Path
import subprocess


def main():
    cache = '/tmp/nexusmind-npm-cache'
    script = """const fs=require('fs');const c=require('/usr/local/lib/node_modules/npm/node_modules/cacache');(async()=>{for(const p of JSON.parse(fs.readFileSync('/nm-packages/manifest.json')).packages){await c.put(process.argv[1]+'/_cacache',p.url,fs.readFileSync('/nm-packages/'+p.file),{integrity:p.integrity})}})().catch(()=>process.exit(1))"""
    subprocess.run(['node', '-e', script, cache], check=True)
    subprocess.run(['npm', 'ci', '--offline', '--no-audit', '--no-fund', '--cache', cache], check=True)
    package = json.loads(Path('package.json').read_text())
    if 'build' in package.get('scripts', {}):
        subprocess.run(['npm', 'run', 'build'], check=True)


if __name__ == '__main__':
    if sys.argv[1:2] == ['start']:
        package = json.loads(Path('package.json').read_text())
        if 'test:seed' in package.get('scripts', {}):
            subprocess.run(['npm', 'run', 'test:seed'], check=True)
            print('NexusMind: declared test:seed completed', flush=True)
        command = sys.argv[2:]
        if not command: raise SystemExit('Missing application start command')
        os.execvp(command[0], command)
    else:
        main()
