import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import nexusmind from './index.ts';

// Exercise real HTTP and the published MCP, without credentials or a model provider.
test('Pi lifecycle, profiles, routing and failure handling', async (t) => {
  const original = { ...process.env };
  const cwd = mkdtempSync(join(tmpdir(), 'nexusmind-pi-'));
  const hits: { path: string; body?: Record<string, unknown> }[] = [];
  let offline = false;
  const server = createServer(async (req, res) => {
    let raw = '';
    for await (const chunk of req) raw += chunk;
    hits.push({ path: req.url!, body: raw ? JSON.parse(raw) : undefined });
    assert.equal(req.headers.authorization, 'Bearer test-key');
    if (offline) { res.writeHead(503); res.end('{}'); return; }
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify(req.url?.startsWith('/v1/memory?')
      ? { memories: [{ id: 'm1', title: 'Existing context', content: 'Known decision' }] }
      : req.url === '/v1/code/projects' ? { projects: [{ name: 'team', indexed_files_count: 5, last_indexed: '2099-01-01T00:00:00Z' }] }
      : req.url?.startsWith('/v1/conventions') ? [{ title: 'Convention', content: 'Use native events' }]
      : { id: 'saved' }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  process.env.NEXUSMIND_API_KEY = 'test-key';
  process.env.NEXUSMIND_BASE_URL = `http://127.0.0.1:${address.port}`;
  for (const key of ['NEXUSMIND_STOP_GATE', 'NEXUSMIND_PROMPT_INJECT', 'NEXUSMIND_AUTO_CAPTURE']) delete process.env[key];
  t.after(() => {
    process.env = original;
    server.close();
    rmSync(cwd, { recursive: true, force: true });
  });

  // Only the harness is loosely typed; the extension is checked against Pi 1.4's actual API.
  function harness(trusted = true) {
    const handlers = new Map<string, (event: any, ctx: ExtensionContext) => any>();
    const entries: any[] = [];
    const notices: string[] = [];
    let registration: any;
    const api = {
      on(name: string, handler: any) { handlers.set(name, handler); },
      registerMcpServer(name: string, config: any) { registration = { name, config }; },
      unregisterMcpServer() { registration = undefined; },
      appendEntry(customType: string, data: unknown) { entries.push({ type: 'custom', customType, data }); },
    } as unknown as ExtensionAPI;
    const ctx = {
      cwd, hasUI: true, isProjectTrusted: () => trusted,
      ui: { notify: (text: string) => notices.push(text) },
      sessionManager: { getBranch: () => entries, getSessionId: () => 'session-1' },
    } as unknown as ExtensionContext;
    nexusmind(api);
    return { entries, notices, registration: () => registration,
      emit: (name: string, event: any = {}) => handlers.get(name)!(event, ctx) };
  }
  const config = (caps?: string[]) => writeFileSync(join(cwd, '.nexusmind.yaml'), `version: 1
repository: { id: repo }
defaults: { project: team${caps ? ', agent_profile: custom' : ''} }
projects:
  team: { project_id: uuid, paths: ['**'] }
${caps ? `agents:\n  profiles:\n    custom:\n      capabilities: ${JSON.stringify(caps)}\n` : ''}`);
  config();
  process.env.NEXUSMIND_MCP_TOOL_PROFILE = 'only_context';
  const h = harness();
  await h.emit('session_start');
  assert.equal(h.registration().name, 'nexusmind');
  assert.equal(h.registration().config.cwd, cwd);
  assert.equal(h.registration().config.env.NEXUSMIND_MCP_TOOL_PROFILE, 'only_context');
  const first = await h.emit('before_agent_start', { messageType: 'user', prompt: 'Implement a plugin' });
  assert.match(first.message.content, /project: team/);
  assert.match(first.message.content, /Known decision/);
  assert.match(first.message.content, /Code index is current/);
  assert(hits.some(h => h.path.includes('project=team')));
  assert.equal(await h.emit('before_agent_start', { messageType: 'user', prompt: 'Continue' }), undefined);
  assert(await h.emit('before_agent_start', { messageType: 'user', prompt: 'recuerda lo anterior' }));
  process.env.NEXUSMIND_PROMPT_INJECT = 'off';
  assert.equal(await h.emit('before_agent_start', { messageType: 'user', prompt: 'remember' }), undefined);
  await h.emit('session_compact');
  assert(await h.emit('before_agent_start', { messageType: 'custom' }));
  delete process.env.NEXUSMIND_PROMPT_INJECT;

  const boundary = { outcome: 'completed', continue: false, entries: [], context: { canContinue: true,
    contextMessages: [{ role: 'user', content: 'fix it' }, { role: 'assistant', content: [{ type: 'text', text: 'Fixed the routing issue.' }] }] } };
  await h.emit('tool_result', { toolName: 'mcp__nexusmind__store_memory', isError: true, input: {} });
  assert.equal((await h.emit('agent_before_settle', boundary)).continue, true);
  assert.equal(await h.emit('agent_before_settle', boundary), undefined);
  const saved = harness();
  await saved.emit('session_start');
  await saved.emit('tool_result', { toolName: 'mcp__nexusmind__store_memory', isError: false, input: { type: 'session_summary' } });
  assert.equal(await saved.emit('agent_before_settle', boundary), undefined);
  await saved.emit('session_start'); // /reload reconstructs markers on the active branch.
  assert.equal(await saved.emit('agent_before_settle', boundary), undefined);

  const assistant = { type: 'message', message: { role: 'assistant', content: [
    { type: 'thinking', thinking: 'PRIVATE THOUGHT' },
    { type: 'text', text: 'Implemented native hooks and corrected profile routing. '.repeat(8) },
  ] } };
  h.entries.push(assistant);
  const stores = () => hits.filter(h => h.path === '/v1/memory/store');
  await h.emit('session_before_compact');
  assert.equal(stores().length, 1);
  assert.equal(stores()[0].body!.topic_key, 'session-snapshot/session-1');
  assert.equal(stores()[0].body!.tool, 'pi');
  assert(!JSON.stringify(stores()[0]).includes('PRIVATE THOUGHT'));
  await h.emit('session_shutdown');
  assert.equal(stores().length, 1); // No duplicate when compaction and shutdown see the same text.
  await h.emit('session_start');
  await h.emit('session_shutdown');
  assert.equal(stores().length, 1); // Nor after /reload.
  h.entries.push({ ...assistant, message: { role: 'assistant', content: [{ type: 'text', text: 'New discovery. '.repeat(20) }] } });
  await h.emit('session_shutdown');
  assert.equal(stores().length, 2);
  saved.entries.push(assistant);
  await saved.emit('session_shutdown');
  assert.equal(stores().length, 2); // Explicit summary suppresses fallback.
  await h.emit('tool_result', { toolName: 'subagent', isError: false, content: [{ type: 'text', text: 'Discovered a convention. '.repeat(10) }] });
  assert.equal(stores().at(-1)!.body!.type, 'discovery');

  for (const profile of ['essential', 'only_context', 'reduced_readonly', 'factory_operator', 'legacy']) {
    process.env.NEXUSMIND_MCP_TOOL_PROFILE = profile;
    const plugin = harness();
    await plugin.emit('session_start');
    const { command, args, env } = plugin.registration().config;
    const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, stdio: ['pipe', 'pipe', 'pipe'] });
    const pending = new Map<number, (value: any) => void>();
    const lines = createInterface({ input: child.stdout });
    lines.on('line', line => { const message = JSON.parse(line); if (message.id) pending.get(message.id)?.(message); });
    let id = 0;
    const rpc = (method: string, params: object) => new Promise<any>((resolve, reject) => {
      const requestId = ++id;
      const timer = setTimeout(() => reject(new Error(`MCP timeout: ${profile} ${method}`)), 10000);
      pending.set(requestId, response => { clearTimeout(timer); pending.delete(requestId); resolve(response); });
      child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params }) + '\n');
    });
    try {
      const init = await rpc('initialize', { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'pi-plugin-test', version: '1' } });
      assert(!init.error, JSON.stringify(init.error));
      child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
      const tools = (await rpc('tools/list', {})).result.tools.map((tool: { name: string }) => tool.name);
      assert(tools.length > 0);
      if (profile === 'only_context') { assert(tools.includes('store_memory')); assert(!tools.includes('list_my_tasks')); }
      if (profile === 'reduced_readonly') assert.deepEqual(tools.sort(), ['execute_tool', 'fetch', 'find_tools', 'load_tool']);
      if (profile === 'factory_operator') { assert(tools.includes('submit_factory_task')); assert(!tools.includes('store_memory')); }
    } finally { child.kill(); lines.close(); }
    if (profile === 'reduced_readonly' || profile === 'factory_operator') {
      const before = stores().length;
      plugin.entries.push(assistant);
      await plugin.emit('session_before_compact');
      await plugin.emit('session_shutdown');
      assert.equal(await plugin.emit('agent_before_settle', boundary), undefined);
      assert.equal(stores().length, before);
    }
  }

  process.env.NEXUSMIND_MCP_TOOL_PROFILE = 'essential';
  config(['memory.read']);
  const restricted = harness();
  await restricted.emit('session_start');
  const before = hits.length;
  await restricted.emit('before_agent_start', { messageType: 'user', prompt: 'continue' });
  assert.deepEqual(hits.slice(before).map(h => h.path.split('?')[0]), ['/v1/memory']);
  restricted.entries.push(assistant);
  await restricted.emit('session_before_compact');
  assert.equal(await restricted.emit('agent_before_settle', boundary), undefined);
  config(['memory.write']);
  const writeOnly = harness();
  await writeOnly.emit('session_start');
  assert.equal(await writeOnly.emit('before_agent_start', { messageType: 'user', prompt: 'remember' }), undefined);
  config();

  const untrusted = harness(false);
  await untrusted.emit('session_start');
  assert.equal(untrusted.registration(), undefined);
  writeFileSync(join(cwd, '.nexusmind.yaml'), 'version: 99');
  const invalid = harness();
  await assert.rejects(() => invalid.emit('session_start'), /CONFIG_UNSUPPORTED_VERSION/);
  assert.equal(invalid.registration(), undefined);
  config();
  offline = true;
  const broken = harness();
  await broken.emit('session_start');
  const fallback = await broken.emit('before_agent_start', { messageType: 'user', prompt: 'continue' });
  assert.match(fallback.message.content, /NexusMind/);
  assert(broken.notices.some(n => n.includes('unavailable')));
  broken.entries.push(assistant);
  await broken.emit('session_before_compact');
  assert(broken.notices.some(n => n.includes('snapshot failed')));

  delete process.env.NEXUSMIND_API_KEY;
  const missing = harness();
  await missing.emit('session_start');
  assert.equal(missing.registration(), undefined);
  assert(missing.notices.some(n => n.includes('NOT CONNECTED')));
  process.env.NEXUSMIND_MCP_TOOL_PROFILE = 'only-context';
  assert.throws(() => harness(), /Unknown NEXUSMIND_MCP_TOOL_PROFILE/);
  process.env.NEXUSMIND_MCP_TOOL_PROFILE = 'essential';
  process.env.NEXUSMIND_BASE_URL = 'http://untrusted.example';
  assert.throws(() => harness(), /requires HTTPS/);
});
