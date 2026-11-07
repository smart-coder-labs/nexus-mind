"""Run: python3 -m unittest discover -s scripts/factory -p test_agent_runtime.py"""
import json
from pathlib import Path
import tempfile
import unittest

from container_entry import agent_result, configure_runtime


class AgentRuntimeTests(unittest.TestCase):
    def test_bound_harness_snapshot_survives_adapter_compilation_without_enabling_mcp(self):
        from agent_runtime import effective_runtime, stage_runtime
        from runtime_settings import compile_settings
        from container_entry import agent_prompt
        snapshot = {'executor': 'claude', 'plan': None, 'config': {'runtime_resolved': {'components': [
            {'id': 'rules', 'kind': 'instructions', 'files': {'RULES.md': 'Check tenant isolation.'}}
        ]}}}
        profile = {'agents': [{'role': 'reviewer', 'harness': 'claude', 'model': 'sonnet',
                              'image': 'image', 'network': 'model-net', 'model_connection': 'proxy',
                              'runtime_snapshot': snapshot}]}
        adapters, _ = compile_settings(profile)
        adapter = adapters['isolated-reviewer']
        job = {'step': {'id': 'review', 'kind': 'review', 'options': {}}}
        self.assertEqual(effective_runtime(job, adapter), snapshot)
        context = {'step': job['step'], 'harness_markdown': snapshot['config']['runtime_resolved']['components']}
        prompt = agent_prompt(context, adapter)
        self.assertIn('Apply context.harness_markdown', prompt)
        self.assertIn('Check tenant isolation.', prompt)
        with tempfile.TemporaryDirectory() as temp:
            stage_runtime({}, job, adapter, Path(temp))
            self.assertFalse((Path(temp) / 'runtime-plan.json').exists())
        explicit = {'config': {'runtime_resolved': {}}, 'plan': {}}
        job['step']['options']['runtime_snapshot'] = explicit
        self.assertEqual(effective_runtime(job, adapter), explicit)
        job['step']['options'] = {}
        snapshot['config']['runtime_resolved']['components'].append({'kind': 'extension', 'enabled': True})
        with self.assertRaisesRegex(ValueError, 'executable runtime plan'):
            stage_runtime({}, job, adapter, Path('/unused'))

    def test_materializes_only_pinned_resources_and_resolves_container_env(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'runtime'
            plan = {'files': {str(root / 'mcp.json'): json.dumps({'mcpServers': {'docs': {'env': {'KEY': '${DOCS_KEY}'}}}}),
                              str(root / 'resources/rust/SKILL.md'): 'Rust review'},
                    'args': ['--skill', str(root / 'resources/rust')],
                    'env': {'PI_CODING_AGENT_DIR': str(root)}, 'mcp_names': ['nexusmind', 'docs']}
            environment = {'DOCS_KEY': 'private"value\\line'}
            command = configure_runtime(['pi', '--no-extensions'], plan, environment, root)
            self.assertEqual(json.loads((root / 'mcp.json').read_text())['mcpServers']['docs']['env']['KEY'], environment['DOCS_KEY'])
            self.assertEqual(command[-2:], plan['args'])
            self.assertEqual(environment['PI_CODING_AGENT_DIR'], str(root))
            self.assertEqual((root / 'resources/rust/SKILL.md').read_text(), 'Rust review')

    def test_rejects_paths_outside_runtime_and_missing_secrets(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'runtime'
            with self.assertRaisesRegex(ValueError, 'Unsafe'):
                configure_runtime(['pi'], {'files': {str(Path(temp) / 'escape'): 'bad'}}, {}, root)
            other = Path(temp) / 'other'
            with self.assertRaisesRegex(ValueError, 'reference is missing'):
                configure_runtime(['pi'], {'files': {str(other / 'mcp.json'): '"${MISSING}"'}}, {}, other)
            self.assertFalse((Path(temp) / 'escape').exists())

    def test_claude_enables_only_assigned_mcps_and_keeps_builtin_restrictions(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'runtime'
            command = configure_runtime(['claude', '--allowedTools', 'Read,Grep'], {'files': {}, 'args': [], 'mcp_names': ['plugin_nexusmind_nexusmind', 'docs']}, {}, root)
            self.assertEqual(command[-1], 'Read,Grep,mcp__plugin_nexusmind_nexusmind__*,mcp__docs__*')
            self.assertIn('--setting-sources', command)
            self.assertNotIn('Bash', command[-1])

    def test_pi_requires_settled_success_not_partial_messages(self):
        events = [{'type': 'message_end', 'message': {'role': 'assistant', 'stopReason': 'stop', 'content': [{'type': 'text', 'text': '{"success":true,"cost_usd":123}'}]}}]
        raw = '\n'.join(map(json.dumps, events))
        with self.assertRaises(ValueError): agent_result(raw, 'pi')
        events.append({'type': 'agent_settled', 'aborted': False})
        result = agent_result('\n'.join(map(json.dumps, events)), 'pi')
        self.assertTrue(result['success'])
        self.assertEqual(result['cost_usd'], 0)

    def test_codex_uses_observed_usage_not_model_reported_cost(self):
        events = [{'type': 'item.completed', 'item': {'type': 'agent_message', 'text': '{"success":true,"cost_usd":0}'}},
                  {'type': 'turn.completed', 'usage': {'input_tokens': 100, 'cached_input_tokens': 20, 'output_tokens': 5}}]
        raw = '\n'.join(map(json.dumps, events))
        with self.assertRaises(ValueError): agent_result(raw, 'codex')
        self.assertAlmostEqual(agent_result(raw, 'codex', [2, 1, 10])['cost_usd'], .00023)
        events.append({'type': 'turn.failed'})
        with self.assertRaises(ValueError): agent_result('\n'.join(map(json.dumps, events)), 'codex', [2, 1, 10])




class ModelFailureTests(unittest.TestCase):
    def transcript(self,stop,error=None):
        import json as _j
        msg={'role':'assistant','content':[],'stopReason':stop}
        if error:msg['errorMessage']=error
        return '\n'.join(_j.dumps(e) for e in [{'type':'agent_start'},{'type':'message_end','message':msg},{'type':'agent_settled','aborted':False}])
    def test_unavailable_model_is_named_not_invalid_json(self):
        from container_entry import model_failure
        self.assertEqual(model_failure(self.transcript('error','Codex error: Model relay unavailable; inspect execution readiness'))[0],'model_unavailable')
        self.assertEqual(model_failure(self.transcript('error','You exceeded your usage limit'))[0],'model_unavailable')
        self.assertEqual(model_failure(self.transcript('error','context window exceeded'))[0],'agent_model_error')
        self.assertIsNone(model_failure(self.transcript('stop')))
        self.assertIsNone(model_failure('not json'))



class DocumentationScopeTests(unittest.TestCase):
    def test_documentation_scope_restricts_implementation_and_tests_only(self):
        from container_entry import agent_prompt
        base={'step':{'id':'implement','kind':'agent'},'task':{},'task_scope':'documentation'}
        self.assertIn('documentation-only',agent_prompt(base,{'role':'implementation'}))
        self.assertIn('documentation-only',agent_prompt(dict(base,step={'id':'tests','kind':'agent'}),{'role':'test_author'}))
        self.assertNotIn('documentation-only',agent_prompt(dict(base,step={'id':'investigate','kind':'agent'}),{'role':'investigation'}))
        self.assertNotIn('documentation-only',agent_prompt({'step':{'id':'implement','kind':'agent'},'task':{}},{'role':'implementation'}))

if __name__ == '__main__':
    unittest.main()
