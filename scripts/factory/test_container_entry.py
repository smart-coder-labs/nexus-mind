import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import container_entry as entry


class NodeBootstrapTests(unittest.TestCase):
    def test_each_agent_assesses_phase_scope_without_skipping_required_checks(self):
        for role in ('investigation', 'test_author', 'implementation', 'reviewer'):
            prompt = entry.agent_prompt({'step': {'id': role, 'kind': 'agent'}}, {'role': role})
            self.assertIn('Before acting, assess whether this phase is needed', prompt)
            self.assertIn('Execute only applicable work', prompt)
            self.assertIn('never skip a required review', prompt)
            self.assertIn('unavailable services as inapplicability', prompt)

    def test_install_uses_disk_scratch_not_bounded_tmpfs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('package.json', 'package-lock.json'):
                (root / name).write_text('{}')
            with patch.object(type(root), 'read_text', return_value='{}'), \
                 patch.object(type(root), 'relative_to', return_value=Path('api')), \
                 patch.object(entry, 'tee', side_effect=[0, 1]) as tee, \
                 patch.object(entry, 'announce'), \
                 patch.object(entry.tempfile, 'TemporaryDirectory', return_value=tempfile.TemporaryDirectory()) as temporary:
                self.assertEqual(entry.bootstrap_node(root, root), 1)
            self.assertEqual(temporary.call_args.kwargs['dir'], '/scratch')
            self.assertIn('/scratch/nm-npm-cache/_cacache', tee.call_args_list[0].args[0][2])
            self.assertEqual(tee.call_args.kwargs['env']['TMPDIR'], '/scratch')

    def test_dependency_failure_holds_without_running_application_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'input').mkdir(); (root / 'output').mkdir(); (root / 'work').mkdir()
            (root / 'input/context.json').write_text('{}')
            (root / 'input/adapter.json').write_text(json.dumps({
                'kind': 'checks', 'node_bootstrap': True,
                'checks': [{'name': 'tests', 'cwd': 'api', 'command': ['npm', 'test']}]}))
            def path(value):
                return root / str(value).lstrip('/')
            with patch.object(entry, 'Path', side_effect=path), \
                 patch.object(entry, 'bootstrap_node', return_value=1), \
                 patch.object(entry, 'tee') as tee:
                self.assertEqual(entry.main(), 0)
            result = json.loads((root / 'output/result.json').read_text())
            self.assertFalse(result['success'])
            self.assertTrue(result['hold'])
            self.assertEqual(result['error'], 'node_dependency_install_failed')
            tee.assert_not_called()


if __name__ == '__main__':
    unittest.main()
