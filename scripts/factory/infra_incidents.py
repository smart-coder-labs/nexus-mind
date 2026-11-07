"""Operator-approved infrastructure incidents for a control-plane outage window.

Disabled unless the private runner configuration contains `infra_collector`:

    "infra_collector": {"command": ["kubectl", "--kubeconfig", "/home/ubuntu/.kube/nexusmind-events",
                                    "get", "events", "-n", "nexusmind", "-o", "json"]}

The command must print Kubernetes events as JSON (an `items` list). Use a
kubeconfig whose role can only *list events* in the backend's namespace: the
runner never receives kubectl, SSH, sudo or Docker-socket authority by default.
Only a small allowlist of reasons is forwarded (restarts, evictions, OOM, disk
pressure, failed probes), with the object kind/name and a bounded message. These
are *coincident* incidents, never presented as the confirmed cause of a failure.
"""
import calendar
import json
import subprocess
import time

REASONS = {'Evicted', 'Evicting', 'EvictionThresholdMet', 'NodeHasDiskPressure', 'FreeDiskSpaceFailed',
           'OOMKilling', 'OOMKilled', 'Killing', 'BackOff', 'Unhealthy', 'FailedScheduling',
           'Preempting', 'NodeNotReady', 'Started', 'Created'}
LIMIT = 20


def _timestamp(item):
    candidates = [item.get('lastTimestamp'), item.get('eventTime'), item.get('firstTimestamp'),
                  (item.get('metadata') or {}).get('creationTimestamp')]
    for value in candidates:
        if isinstance(value, str) and len(value) >= 19:
            try:
                return calendar.timegm(time.strptime(value[:19], '%Y-%m-%dT%H:%M:%S'))
            except ValueError:
                continue
    return None


def parse(output, start, end):
    try:
        items = json.loads(output).get('items', [])
    except (ValueError, AttributeError):
        return []
    found = []
    for item in items if isinstance(items, list) else []:
        if not isinstance(item, dict) or item.get('reason') not in REASONS:
            continue
        at = _timestamp(item)
        if at is None or not start <= at <= end:
            continue
        involved = item.get('involvedObject') or {}
        found.append({'reason': item['reason'], 'at': at, 'count': item.get('count') or 1,
                      'object': str(involved.get('kind', '?'))[:40] + '/' + str(involved.get('name', '?'))[:120],
                      'message': str(item.get('message', ''))[:300], 'source': 'infrastructure',
                      'confirmed_cause': False})
    return sorted(found, key=lambda i: i['at'])[-LIMIT:]


def collect(config, start, end):
    """Never raises: an unavailable collector only means fewer details."""
    if not isinstance(config, dict) or not isinstance(config.get('command'), list) or not config['command']:
        return []
    try:
        result = subprocess.run([str(a) for a in config['command']], stdin=subprocess.DEVNULL, capture_output=True,
                                timeout=20, env={'PATH': '/usr/local/bin:/usr/bin:/bin', 'LANG': 'C'})
    except (OSError, subprocess.SubprocessError):
        return []
    return parse(result.stdout.decode(errors='replace'), start, end) if result.returncode == 0 else []
