"""Apply immutable per-agent resource plans inside the existing isolated adapters."""
import json
import re
from pathlib import Path


def configure_runtime(command, plan, environ, root=Path('/tmp/nexusmind-agent-runtime')):
    """Apply only the server-pinned plan in this container, never global resources."""
    root.mkdir(mode=0o700, exist_ok=False)
    for filename, content in plan['files'].items():
        path = Path(filename)
        if root not in path.parents or '..' in path.parts:
            raise ValueError('Unsafe agent runtime path')
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    environ.update(plan.get('env', {}))
    # Pi/Claude expand these natively. Codex does not, so resolve references
    # strictly from this container's operator-provisioned environment.
    import re
    for filename in ('config.toml', 'mcp.json'):
        path = root / filename
        if not path.exists(): continue
        def substitute(match):
            key = match.group(1)
            if key not in environ: raise ValueError('Agent runtime environment reference is missing')
            # Both JSON and TOML basic strings use JSON-compatible escaping.
            return json.dumps(environ[key])[1:-1]
        path.write_text(re.sub(r'\$\{([A-Z_][A-Z0-9_]*)\}', substitute, path.read_text()))
    if command and Path(command[0]).name == 'pi':
        base = next((command[i + 1] for i in reversed(range(len(command) - 1)) if command[i] == '--tools'), 'read,bash,edit,write,grep,find,ls')
        command += ['--tools', ','.join([base, *plan.get('tools', []), 'mcp__*'])]
    if command and Path(command[0]).name == 'nexus':
        if '--allowed-tools' not in command: raise ValueError('Nexus adapter requires an explicit tool allowlist')
        index = command.index('--allowed-tools') + 1
        command[index] += ',' + ','.join('mcp__' + name + '__*' for name in plan['mcp_names'])
    if command and Path(command[0]).name == 'claude':
        base = next((command[i + 1] for i, arg in enumerate(command[:-1]) if arg == '--allowedTools'), 'Read,Edit,Write,Grep,Glob,Skill')
        tools = ','.join('mcp__' + name + '__*' for name in plan['mcp_names'])
        command += ['--setting-sources', '', '--allowedTools', base + ',' + tools]
    return command + plan['args']


def codex_prices(prices):
    if not isinstance(prices, list) or len(prices) != 3 or any(not isinstance(p, (int, float)) or isinstance(p, bool) or not 0 <= p < float('inf') for p in prices):
        raise ValueError('Codex adapter requires input/cached/output USD prices')
    return prices


def agent_result(raw, format, prices=None):
    if format not in ('claude', 'pi', 'codex'): return json.loads(raw)
    events = [json.loads(line) for line in raw.splitlines() if line.strip()]
    if format == 'claude':
        envelope = next(e for e in reversed(events) if e.get('type') == 'result')
        result = json.loads(envelope['result'])
        result['cost_usd'] = envelope.get('total_cost_usd', 0)
        return result
    if format == 'pi':
        if not any(e.get('type') == 'agent_settled' and e.get('aborted') is False for e in events):
            raise ValueError('Pi did not settle')
        message = next(e['message'] for e in reversed(events) if e.get('type') == 'message_end' and e.get('message', {}).get('role') == 'assistant')
        if message.get('stopReason') != 'stop': raise ValueError('Pi failed')
        result = json.loads(''.join(c['text'] for c in message['content'] if c.get('type') == 'text'))
        result['cost_usd'] = 0  # subscription usage is not an API invoice
        return result
    if any(e.get('type') == 'turn.failed' for e in events) or not any(e.get('type') == 'turn.completed' for e in events):
        raise ValueError('Codex failed')
    message = next(e['item']['text'] for e in reversed(events) if e.get('type') == 'item.completed' and e.get('item', {}).get('type') == 'agent_message')
    result = json.loads(message)
    usage = [e.get('usage', {}) for e in events if e.get('type') == 'turn.completed']
    prices = codex_prices(prices)
    result['cost_usd'] = sum((max(0, u.get('input_tokens', 0) - u.get('cached_input_tokens', 0)) * prices[0] + u.get('cached_input_tokens', 0) * prices[1] + u.get('output_tokens', 0) * prices[2]) / 1_000_000 for u in usage)
    return result



def effective_runtime(job, adapter):
    options = job['step'].get('options')
    selected = options.get('runtime_snapshot') if isinstance(options, dict) else None
    return selected or adapter.get('runtime_snapshot') or {}


def stage_runtime(config, job, adapter, inp):
    runtime = effective_runtime(job, adapter)
    if runtime:
        # Markdown-only catalog bindings are applied in the prompt, without enabling
        # MCP/network access in offline workflow containers.
        if runtime.get('plan') is None:
            components = runtime.get('config', {}).get('runtime_resolved', {}).get('components', [])
            if any(c.get('enabled', True) and c.get('kind') != 'instructions' for c in components):
                raise ValueError('Native agent resources require an executable runtime plan')
            return
        if adapter['kind'] != 'agent': raise ValueError('Agent runtime requires an agent adapter')
        if adapter.get('driver', 'container') != 'container' and not adapter.get('supports_agent_runtime'):
            raise ValueError('VM driver must explicitly support agent-runtime v1')
        executor = adapter.get('executor', adapter.get('format', 'claude'))
        if runtime['executor'] != executor:
            raise ValueError('Selected agent executor does not match the installed adapter executor')
        if adapter.get('model_connection') == 'backend':
            raise ValueError('Native agent resources require an adapter with an approved MCP transport; offline backend-model adapters support markdown harnesses only')
        nexusmind = config.get('nexusmind_mcp')
        if not isinstance(nexusmind, dict) or not nexusmind.get('command'):
            raise ValueError('Configure the runner NexusMind MCP transport before using agent runtimes')
        # Never give the container the runner token. The operator provides a scoped
        # NexusMind credential/proxy through the adapter environment references.
        plan = json.loads(json.dumps(runtime['plan']))
        root = '/tmp/nexusmind-agent-runtime'
        nexusmind = dict(nexusmind)
        for key, value in nexusmind.get('env', {}).items():
            if any(part in key.upper() for part in ('KEY', 'TOKEN', 'SECRET', 'PASSWORD')) and not re.fullmatch(r'\$\{[A-Z_][A-Z0-9_]*\}', value):
                raise ValueError('Runner NexusMind credentials must use scoped environment references')
        nexusmind['env'] = dict(nexusmind.get('env', {}), NEXUSMIND_MCP_TOOL_PROFILE=runtime['config']['runtime_resolved']['nexusmind_profile'])
        if executor == 'nexus':
            nexusmind['env'] = {k: v for k, v in nexusmind['env'].items() if not any(part in k.upper() for part in ('KEY', 'TOKEN', 'SECRET', 'PASSWORD'))}
        if executor == 'codex':
            provider = adapter.get('codex_config')
            if not isinstance(provider, str) or not provider.strip() or 'mcp_servers' in provider:
                raise ValueError('Codex adapter requires provider-only codex_config')
            overlay = plan['files'].pop(root + '/runtime.toml')
            # Replace only the generated NexusMind section; preserve additional MCPs.
            first, *rest = overlay.split('\n[mcp_servers.')
            sections = ['\n[mcp_servers.' + section for section in rest if not section.startswith('nexusmind]') and not section.startswith('nexusmind.')]
            overlay = first + ''.join(sections) + '\n[mcp_servers.nexusmind]\n'
            for key in ('command', 'url', 'args'):
                if key in nexusmind: overlay += key + ' = ' + json.dumps(nexusmind[key]) + '\n'
            for field in ('env', 'headers'):
                if nexusmind.get(field):
                    overlay += '[mcp_servers.nexusmind.' + ('http_headers' if field == 'headers' else field) + ']\n'
                    for key, value in nexusmind[field].items(): overlay += json.dumps(key) + ' = ' + json.dumps(value) + '\n'
            plan['files'][root + '/config.toml'] = provider + '\n' + overlay
        else:
            mcp_path = root + '/mcp.json'
            mcp = json.loads(plan['files'][mcp_path])
            nexus_name = 'nexusmind' if executor == 'pi' else 'plugin_nexusmind_nexusmind'
            mcp['mcpServers'][nexus_name] = nexusmind
            if executor == 'pi': mcp['mcpServers'][nexus_name]['exposure'] = 'direct'
            plan['files'][mcp_path] = json.dumps(mcp)
        (inp / 'runtime-plan.json').write_text(json.dumps(plan))
