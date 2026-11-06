"""Run: python3 -m unittest discover -s scripts/factory -p test_agent_runtime.py"""
import json
from pathlib import Path
import tempfile
import unittest

from container_entry import agent_result, configure_runtime


class AgentRuntimeTests(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
