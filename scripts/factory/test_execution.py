import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch
import uuid
import runner
import execution
import delivery


class API:
    def __init__(self): self.artifacts = []; self.events = []
    def call(self, path, data=None):
        if path.endswith('/artifacts'):
            content = base64.b64decode(data['content_base64'])
            self.artifacts.append((data['name'], content))
            return {'digest': hashlib.sha256(content).hexdigest(), 'id': str(len(self.artifacts))}
        if path.endswith('/events'): self.events.append(data)
        return {'active': True}


class ExecutionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.root = self.base / 'projects'; self.root.mkdir()
        for name in ['api', 'web']:
            repo = self.root / name; repo.mkdir()
            runner.git(['init', '-q'], repo)
            (repo / 'code.txt').write_text('old\n')
            runner.git(['add', '.'], repo)
            runner.git(['-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','base'], repo)
        repos = runner.discover(self.root, [self.root])
        for repo in repos: repo['writable'] = True
        self.config = {'state_dir':str(self.base/'state'), 'runner_token':'nmr_private_runner_credential', 'environments':{}}
        self.job = {'run_id':str(uuid.uuid4()), 'attempt':1, 'lease_token':'fence', 'lease_until':int(__import__('time').time())+300,
            'task':{'title':'Fix cross-repo bug'}, 'snapshot':{'workspace':{'id':'workspace','root_path':str(self.root),'repositories':repos},'definition':{'max_cost_usd':2}},
            'execution':{'heads':{r['path']:r['head_sha'] for r in repos},'revision':0,'cost_usd':0},'dependency_results':{},
            'step':{'id':'implement','kind':'agent','adapter':'agent','write_repositories':['api','web'],'timeout_seconds':60}}
        runner.prepare(self.job,[self.root],self.config['state_dir'])
        self.api = API()

    def tearDown(self):
        delivery.cleanup(self.config,self.job['run_id'],self.config['state_dir'])
        self.temp.cleanup()

    def test_redaction_and_network_image_guards(self):
        self.assertNotIn('my-private-secret',execution.redact('password=my-private-secret', ['my-private-secret']))
        self.assertNotIn('ghp_abcdefghijklmnop',execution.redact('ghp_abcdefghijklmnop', []))
        for image in ['latest','python:latest','evil@sha256:abc']:
            with self.assertRaises(ValueError): execution.pinned_image(image)
        with self.assertRaises(ValueError): execution.adapter_network({}, {'network':'host'}, self.job['run_id'])
        with self.assertRaises(ValueError): execution.adapter_network({}, {'network':'bridge'}, self.job['run_id'])

    def test_ignored_generated_outputs_are_not_adopted_or_reused_as_inputs(self):
        repo=Path(self.config['state_dir'])/self.job['run_id']/'api'
        (repo/'.gitignore').write_text('dist/\n')
        runner.git(['add','.gitignore'],repo);runner.git(['-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','ignore build output'],repo)
        self.job['execution']['heads']['api']=runner.git(['rev-parse','HEAD'],repo)
        _,work=execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        (work/'api'/'dist').mkdir();(work/'api'/'dist'/'build.js').write_text('generated')
        self.job['step']['write_repositories']=[]
        self.assertEqual(execution.adopt_changes(self.config,self.job,work,runner.git,[]),self.job['execution']['heads'])
        _,work=execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        (work/'api'/'dist').mkdir();(work/'api'/'dist'/'build.js').write_text('generated')
        self.job['step']['write_repositories']=['api'];(work/'api'/'code.txt').write_text('fixed\n')
        self.job['execution']['heads']=execution.adopt_changes(self.config,self.job,work,runner.git,[])
        _,fresh=execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        self.assertFalse((fresh/'api'/'dist').exists());self.assertFalse((repo/'dist').exists())
        self.assertEqual((fresh/'api'/'code.txt').read_text(),'fixed\n')

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Set FACTORY_TEST_IMAGE to run real sandbox integration')
    def test_tests_build_allow_generated_output_but_do_not_publish_it(self):
        self.job['step']={'id':'verify','kind':'check','adapter':'qa','write_repositories':[],'timeout_seconds':60,'options':{'required_checks':['tests','build']}}
        adapter={'kind':'checks','image':os.environ['FACTORY_TEST_IMAGE'],'checks':[{'name':'tests','command':['python3','-c','assert True']},{'name':'build','command':['python3','-c',"from pathlib import Path; p=Path('/work/api/dist');p.mkdir();(p/'build.js').write_text('generated')"]}]}
        success,evidence,outcome=execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(success);self.assertEqual({e['check'] for e in evidence},{'tests','build'});self.assertEqual(outcome['heads'],self.job['execution']['heads'])
        _,fresh=execution.stage_sources(self.job,self.config['state_dir'],runner.git);self.assertFalse((fresh/'api'/'dist').exists())

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Set FACTORY_TEST_IMAGE to run real sandbox integration')
    def test_verification_cannot_auto_fix_committed_source(self):
        self.job['step']={'id':'verify','kind':'check','adapter':'qa','write_repositories':[],'timeout_seconds':60}
        adapter={'kind':'checks','image':os.environ['FACTORY_TEST_IMAGE'],'checks':[{'name':'build','command':['python3','-c',"from pathlib import Path; Path('/work/api/code.txt').write_text('silently fixed')"]}]}
        with self.assertRaises(ValueError):execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        repo=Path(self.config['state_dir'])/self.job['run_id']/'api'
        self.assertEqual((repo/'code.txt').read_text(),'old\n')

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Requires immutable Docker fixture')
    def test_nested_browser_screenshots_are_uploaded_without_name_collisions(self):
        self.job['step']={'id':'visual','kind':'check','adapter':'qa','write_repositories':[],'timeout_seconds':60}
        script="from pathlib import Path;import base64;data=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aD1sAAAAASUVORK5CYII=');[(Path('/output')/d).mkdir(parents=True) for d in ('first','second')];[(Path('/output')/d/'screen.png').write_bytes(data) for d in ('first','second')]"
        adapter={'kind':'checks','image':os.environ['FACTORY_TEST_IMAGE'],'checks':[{'name':'visual','command':['python3','-c',script]}]}
        passed,_,_=execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(passed)
        names=[name for name,_ in self.api.artifacts if name.endswith('.png')]
        self.assertEqual(len(names),2);self.assertEqual(len(set(names)),2)

    def test_workspace_file_diagnostics_preserve_guards_and_reach_completion(self):
        tree = self.base/'tree'; tree.mkdir()
        file = tree/'large.bin'
        with file.open('wb') as stream:
            stream.truncate(10 * 1024 * 1024)
        execution.check_tree(tree)
        with file.open('ab') as stream:
            stream.write(b'x')
        with self.assertRaises(execution.WorkspaceOutputError) as caught:
            execution.check_tree(tree)
        details = caught.exception.details
        self.assertEqual(details['error'], 'workspace_file_too_large')
        self.assertEqual(details['path'], 'large.bin')
        self.assertEqual(details['actual'], details['limit'] + 1)
        self.assertTrue(details['solution'])
        api = Mock()
        api.call.side_effect = lambda path, data=None: ({'workspaces': []} if path == '/workspaces' else {'job': self.job} if path == '/claim' else {})
        config = {**self.config, 'api_url': 'http://localhost', 'adapters': {'agent': {}}}
        with patch.object(runner, 'API', return_value=api), patch.object(runner, 'reconcile'), patch.object(execution, 'isolated_adapter', side_effect=caught.exception):
            runner.tick_locked(config, [self.root])
        completion = next(call.args[1] for call in api.call.call_args_list if call.args[0].endswith('/complete'))
        self.assertFalse(completion['success'])
        self.assertEqual(completion['outcome']['output'], details)
        file.unlink()
        file.write_text('small')
        os.link(file, tree/'linked.bin')
        with self.assertRaises(execution.WorkspaceOutputError) as caught:
            execution.check_tree(tree)
        self.assertEqual(caught.exception.details['error'], 'workspace_hard_link')
        self.assertEqual(caught.exception.details['actual'], 2)
        self.assertEqual(caught.exception.details['limit'], 1)
        (tree/'linked.bin').unlink()
        execution.check_tree(tree)
        destination = self.base/'api-copy'
        with file.open('wb') as stream:
            stream.truncate(10 * 1024 * 1024 + 1)
        with self.assertRaises(execution.WorkspaceOutputError) as caught:
            execution.copy_source(tree, destination)
        self.assertEqual(caught.exception.details['path'], 'api-copy/large.bin')

    def test_symlink_escape_is_rejected(self):
        tree = self.base/'tree'; tree.mkdir(); (tree/'escape').symlink_to('/etc/passwd')
        with self.assertRaises(ValueError): execution.check_tree(tree)

    def test_adoption_and_export_preserve_binary_and_exact_git_objects(self):
        stage, work = execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        (work/'api'/'code.txt').write_bytes(b'fixed\n\x00binary\xff\n')
        (work/'web'/'code.txt').write_text('integrated\n')
        heads = execution.adopt_changes(self.config,self.job,work,runner.git,[])
        self.assertNotEqual(heads,self.job['execution']['heads'])
        self.job['execution']['heads'] = heads
        payload = delivery.publication_payload(self.config,self.job,runner.git)
        self.assertEqual(len(payload['repositories']),2)
        content = payload['repositories'][0]['commits'][0]['files'][0]['content_base64']
        self.assertEqual(base64.b64decode(content),b'fixed\n\x00binary\xff\n')
        for repo in payload['repositories']:
            commit = repo['commits'][0]
            raw = runner.git(['cat-file','commit',commit['sha']],Path(self.config['state_dir'])/self.job['run_id']/repo['path'],binary=True)
            self.assertEqual(hashlib.sha1(b'commit '+str(len(raw)).encode()+b'\0'+raw).hexdigest(),commit['sha'])
            self.assertIn(b'committer NexusMind <factory@nexusmind.local> 946684800 +0000',raw)
        self.assertEqual((self.root/'api'/'code.txt').read_text(),'old\n')

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Set FACTORY_TEST_IMAGE to run real sandbox integration')
    def test_real_container_agent_and_deterministic_checks(self):
        image=os.environ['FACTORY_TEST_IMAGE']
        script="from pathlib import Path; import json; Path('/work/api/code.txt').write_text('fixed\\n'); Path('/work/web/code.txt').write_text('integrated\\n'); print(json.dumps({'success':True,'summary':'fixed','cost_usd':0}))"
        adapter={'kind':'agent','image':image,'command':['python3','-c',script]}
        success,evidence,outcome=execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(success);self.assertNotEqual(outcome['heads'],self.job['execution']['heads'])
        self.assertEqual(evidence[0]['commits'],outcome['heads'])
        self.job['execution']['heads']=outcome['heads'];self.job['execution']['revision']=1
        self.job['step']={'id':'functional','kind':'check','adapter':'qa','write_repositories':[],'timeout_seconds':60}
        check={'kind':'checks','image':image,'checks':[{'name':'functional','cwd':'api','command':['python3','-c',"from pathlib import Path; assert Path('code.txt').read_text()=='fixed\\n'"]}]}
        success,evidence,_=execution.isolated_adapter(self.config,self.job,check,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(success);self.assertEqual(evidence[0]['check'],'functional')
        self.assertTrue(any(name.startswith('agent.stdout') for name,_ in self.api.artifacts))

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Set FACTORY_TEST_IMAGE to run real sandbox integration')
    def test_read_only_agent_cannot_modify_another_repository(self):
        self.job['step']['write_repositories']=['api']
        adapter={'kind':'agent','image':os.environ['FACTORY_TEST_IMAGE'],'command':['python3','-c',"from pathlib import Path; import json; Path('/work/api/code.txt').write_text('bad'); Path('/work/web/code.txt').write_text('unauthorized'); print(json.dumps({'success':True}))"]}
        with self.assertRaises(ValueError): execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        for path,head in self.job['execution']['heads'].items():
            self.assertEqual(runner.git(['rev-parse','HEAD'],Path(self.config['state_dir'])/self.job['run_id']/path),head)

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Set FACTORY_TEST_IMAGE to run real sandbox integration')
    def test_real_integrated_service_and_networked_qa(self):
        image=os.environ['FACTORY_TEST_IMAGE']
        self.config['workspace_profiles']={'workspace':{'services':{'backend':{'image':image,'command':['python3','-m','http.server','8080'],'cwd':'api','healthcheck':['python3','-c',"import urllib.request; urllib.request.urlopen('http://localhost:8080/code.txt')"]}}}}
        self.job['step']={'id':'environment','kind':'check','adapter':'integrated-environment','write_repositories':[],'timeout_seconds':60}
        success,_,_=delivery.services(self.config,self.job,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(success)
        self.job['step']={'id':'functional','kind':'check','adapter':'qa','write_repositories':[],'timeout_seconds':60}
        adapter={'kind':'checks','image':image,'network':'services','checks':[{'name':'functional','command':['python3','-c',"import urllib.request; assert urllib.request.urlopen('http://backend:8080/code.txt').read()==b'old\\n'"]}]}
        success,_,_=execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(success)
        delivery.validate_services(self.config,self.job)


if __name__=='__main__': unittest.main()
