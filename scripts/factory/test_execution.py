import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
import io
import urllib.error
from unittest.mock import Mock, patch
import uuid
import runner
import execution
import delivery
from container_entry import agent_prompt


class API:
    def __init__(self): self.artifacts = []; self.events = []
    def call(self, path, data=None):
        if path.endswith('/artifacts'):
            content = base64.b64decode(data['content_base64'])
            self.artifacts.append((data['name'], content))
            return {'digest': hashlib.sha256(content).hexdigest(), 'id': str(len(self.artifacts))}
        if path.endswith('/events'):
            assert data['kind'] in {'agent.started', 'agent.tool', 'command.started', 'command.finished', 'service.ready', 'log.chunk', 'dependency.log'}
            assert len(data['message'].encode()) <= 4096
            self.events.append(data)
        return {'active': True}


class RepoSecretsTests(unittest.TestCase):
    def test_env_is_written_privately_inside_the_staged_copy_and_values_are_redacted(self):
        with tempfile.TemporaryDirectory() as directory:
            work=Path(directory);(work/'api').mkdir()
            repos={'api':{'env':'DATABASE_URL=postgres://u:secretpass@db/x\n# c\nTOKEN="abcd1234"','npmrc':'//npm.pkg.github.com/:_authToken=ghp_fixture123\n'}}
            execution.write_env_files(work,repos)
            env=work/'api'/'.env'
            self.assertEqual(env.stat().st_mode & 0o777,0o600)
            self.assertIn('DATABASE_URL=',env.read_text())
            self.assertTrue(execution.ignored_source('.env'),'never published')
            values=execution.secret_values(repos)
            for value in ('postgres://u:secretpass@db/x','abcd1234','ghp_fixture123'):self.assertIn(value,values)
            self.assertNotIn('ghp_fixture123',execution.redact('token ghp_fixture123 ok',values))
            with self.assertRaises(ValueError):execution.write_env_files(work,{'../escape':{'env':'A=1'}})
    def test_trivial_values_never_break_structured_results(self):
        repos={'api':{'env':'SYNC=true\nPORT=3000\nDEBUG=false\nRATIO=0.25\nNULLISH=null\nAPI_KEY=realsecret1'}}
        values=execution.secret_values(repos)
        self.assertEqual(sorted(set(values)),['realsecret1'])
        result={'success':True,'cost_usd':0,'port':3000,'summary':'uses realsecret1 on 3000','items':[None,False]}
        redacted=execution.redact_structure(result,values+['true','3000'])
        self.assertEqual(redacted['success'],True);self.assertEqual(redacted['port'],3000)
        self.assertEqual(redacted['summary'],'uses [REDACTED] on 3000')
        json.dumps(redacted)
    def test_older_backend_without_repo_secrets_is_tolerated(self):
        api=Mock();api.call.side_effect=execution.ControlPlaneError('POST','/repo-secrets','rid',status=404)
        self.assertEqual(execution.repo_secrets(api,{'run_id':'r','step':{'id':'s'},'lease_token':'t'}),{})
        api.call.side_effect=execution.ControlPlaneError('POST','/repo-secrets','rid',status=409)
        with self.assertRaises(execution.ControlPlaneError):execution.repo_secrets(api,{'run_id':'r','step':{'id':'s'},'lease_token':'t'})


class AdvisoryQAGateTests(unittest.TestCase):
    def test_reported_qa_failure_passes_gate_without_relabeling_evidence(self):
        heads={'api':'a'*40}
        result={'heads':heads,'qa_report_only':True,'evidence':[{'check':'functional','status':'fail','commits':heads}]}
        job={'run_id':'r','lease_token':'t','step':{'id':'gate','depends_on':['qa']},
             'execution':{'heads':heads,'revision':1},'dependency_results':{'qa':result}}
        passed,evidence,outcome=delivery.gate(job,API())
        self.assertTrue(passed)
        self.assertEqual(outcome['output']['qa_findings'],['qa'])
        self.assertEqual(result['evidence'][0]['status'],'fail')
        self.assertEqual(evidence[0]['status'],'pass','the gate passed, not the QA check')
        for marker,status,commits in ((False,'fail',heads),(True,'blocked',heads),(True,'fail',{'api':'b'*40})):
            result.update(qa_report_only=marker,evidence=[{'check':'functional','status':status,'commits':commits}])
            with self.assertRaises(ValueError):delivery.gate(job,API())
        result.update(qa_report_only=True,evidence=[])
        with self.assertRaises(ValueError):delivery.gate(job,API())
        result['evidence']=[{'check':'functional','status':'fail','commits':heads}]
        job['step']['depends_on'].append('verify')
        job['dependency_results']['verify']={'heads':heads,'evidence':[{'check':'tests','status':'fail','commits':heads}]}
        with self.assertRaises(ValueError):delivery.gate(job,API())


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
        with self.assertRaises(execution.WorkspaceOutputError) as caught:
            execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertEqual(caught.exception.details['error'], 'verification_source_mutation')
        self.assertEqual(caught.exception.details['paths'], ['api/code.txt'])
        self.assertEqual(caught.exception.details['limit'], 0)
        self.assertTrue(caught.exception.details['hold'])
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

    def test_zip_exclusion_preserves_tracked_archives_when_adopting_source(self):
        repo = Path(self.config['state_dir'])/self.job['run_id']/'api'
        (repo/'layer').mkdir()
        archive = repo/'layer'/'chromium.zip'
        with archive.open('wb') as stream:
            stream.write(b'PK-original')
            stream.truncate(10 * 1024 * 1024 + 1)
        (repo/'root.zip').write_bytes(b'PK-root')
        runner.git(['add', '.'], repo)
        runner.git(['-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','add layers'], repo)
        self.job['execution']['heads']['api'] = runner.git(['rev-parse', 'HEAD'], repo)
        original_blob = runner.git(['hash-object', str(archive)], repo)
        _, work = execution.stage_sources(self.job, self.config['state_dir'], runner.git)
        self.assertFalse((work/'api'/'layer'/'chromium.zip').exists())
        self.assertFalse((work/'api'/'root.zip').exists())
        generated = work/'api'/'generated.zip'
        with generated.open('wb') as stream:
            stream.truncate(11 * 1024 * 1024)
        os.link(generated, work/'api'/'linked.zip')
        execution.check_tree(work)
        generated.unlink()
        generated.symlink_to('/etc/passwd')
        execution.check_tree(work)
        self.assertNotIn('api/generated.zip', execution.tree_fingerprint(work))
        self.job['step']['write_repositories'] = []
        self.assertEqual(execution.adopt_changes(self.config,self.job,work,runner.git,[]), self.job['execution']['heads'])
        self.job['step']['write_repositories'] = ['api']
        _, work = execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        (work/'api'/'generated.zip').symlink_to('/etc/passwd')
        (work/'api'/'code.txt').write_text('fixed\n')
        heads = execution.adopt_changes(self.config,self.job,work,runner.git,[])
        self.assertNotEqual(heads['api'], self.job['execution']['heads']['api'])
        self.assertEqual(runner.git(['hash-object', str(archive)], repo), original_blob)
        self.assertEqual((repo/'root.zip').read_bytes(), b'PK-root')
        self.assertFalse((repo/'generated.zip').exists())
        self.assertEqual(runner.git(['diff', '--name-only', self.job['execution']['heads']['api'], heads['api']], repo), 'code.txt')
        self.job['execution']['heads'] = heads
        _, fresh = execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        self.assertFalse((fresh/'api'/'layer'/'chromium.zip').exists())
        self.assertEqual((fresh/'api'/'code.txt').read_text(), 'fixed\n')

    def test_tracked_env_and_caches_are_excluded_without_blocking_or_deletion(self):
        repo = Path(self.config['state_dir'])/self.job['run_id']/'api'
        excluded = ['.env', '.env.bak-1787936750', 'node_modules/cache.js', 'target/build.bin', '.next/cache', '__pycache__/module.pyc', '.claude/settings.json']
        templates = ['.env.example', '.env.local.sample', '.env.local.template']
        for name in excluded + templates:
            file = repo/name
            file.parent.mkdir(parents=True,exist_ok=True)
            file.write_text('original '+name)
        runner.git(['add', '-f', '.'],repo)
        runner.git(['-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','existing environment and caches'],repo)
        self.job['execution']['heads']['api'] = runner.git(['rev-parse','HEAD'],repo)
        _, work = execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        for name in excluded:
            self.assertFalse((work/'api'/name).exists(),name)
        for name in templates:
            self.assertTrue((work/'api'/name).exists(),name)
        (work/'api'/'.env').write_text('agent-generated secret')
        (work/'api'/'.env.new').write_text('must not be published')
        (work/'api'/'code.txt').write_text('fixed\n')
        heads = execution.adopt_changes(self.config,self.job,work,runner.git,[])
        for name in excluded:
            self.assertEqual((repo/name).read_text(),'original '+name)
        self.assertFalse((repo/'.env.new').exists())
        self.assertEqual(runner.git(['diff','--name-only',self.job['execution']['heads']['api'],heads['api']],repo),'code.txt')
        self.job['execution']['heads'] = heads
        _, fresh = execution.stage_sources(self.job,self.config['state_dir'],runner.git)
        self.assertFalse((fresh/'api'/'.env').exists())
        self.assertEqual((fresh/'api'/'code.txt').read_text(),'fixed\n')

    def test_agent_prompts_separate_phase_responsibilities(self):
        for role in ['investigation', 'test_author', 'implementation', 'reviewer']:
            with self.subTest(role=role):
                prompt = agent_prompt(self.job, {'role': role})
                self.assertEqual('Do not edit files.' in prompt, role in ('investigation', 'reviewer'))
                self.assertIn('step.write_repositories', prompt)
                self.assertIn('Do not claim tests or QA passed unless you actually executed them.', prompt)
                self.assertIn('Never publish, push, merge, access credentials', prompt)
        prompt = agent_prompt(self.job, {'role': 'test_author'})
        self.assertIn('documentation-only', prompt)
        self.assertIn('not_applicable_reason', prompt)
        self.assertIn('return success=true', prompt)
        self.assertNotIn('Implement the complete integrated change', prompt)
        self.assertIn('Implement the complete integrated change', agent_prompt(self.job, {'role': 'implementation'}))
        self.job['step']['kind'] = 'review'
        self.assertIn('Do not edit files.', agent_prompt(self.job, {'role': 'implementation'}))

    @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'), 'Requires immutable Docker fixture')
    def test_documentation_test_authoring_can_finish_before_implementation(self):
        self.job['task'] = {'title': 'Create docs/requested.md; documentation only'}
        self.job['step'].update(id='tests', adapter='isolated-test-author', write_repositories=['api'])
        script = "import json,sys; p=sys.stdin.read(); assert 'Do not edit files.' not in p; assert 'documentation-only' in p; print(json.dumps({'success':True,'verdict':'accept','findings':[],'not_applicable_reason':'Documentation-only task: no behavioral regression to reproduce.'}))"
        adapter = {'kind':'agent','role':'test_author','image':os.environ['FACTORY_TEST_IMAGE'],'command':['python3','-c',script]}
        success,evidence,outcome = execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(success)
        self.assertEqual(evidence[0]['status'], 'not_applicable')
        self.assertEqual(outcome['heads'], self.job['execution']['heads'])
        self.job['dependency_results'] = {'tests':outcome}
        self.job['step'].update(id='implement', adapter='isolated-implementer')
        script = "import json,sys; from pathlib import Path; p=sys.stdin.read(); assert 'Do not edit files.' not in p; assert 'including requested documentation' in p; d=Path('/work/api/docs'); d.mkdir(); (d/'requested.md').write_text('# Requested document\\n'); print(json.dumps({'success':True}))"
        adapter.update(role='implementation', command=['python3','-c',script])
        success,_,outcome = execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertTrue(success)
        self.assertNotEqual(outcome['heads']['api'], self.job['execution']['heads']['api'])
        self.assertEqual((Path(self.config['state_dir'])/self.job['run_id']/'api'/'docs'/'requested.md').read_text(), '# Requested document\n')

    def test_node_checks_mount_disposable_disk_scratch(self):
        self.job['step'].update(id='verify',kind='check',write_repositories=[])
        self.config['node_memory_mb']=8192
        adapter={'kind':'checks','node_bootstrap':True,'image':'sha256:'+'a'*64,
                 'checks':[{'name':'tests','cwd':'api','command':['npm','test']}]}
        captured=[]
        def stop(command,*args,**kwargs):
            captured.extend(command)
            scratch=next(value for value in command if value.endswith(',dst=/scratch'))
            self.assertTrue(Path(scratch.split('src=',1)[1].split(',dst=',1)[0]).is_dir())
            raise RuntimeError('stop at isolated container')
        with patch.object(execution,'node_bootstrap_image',return_value=True),patch('node_environment.prepare'),patch('agent_runtime.stage_runtime'),patch.object(execution,'pinned_image',return_value=adapter['image']),patch.object(execution,'monitor',side_effect=stop),patch.object(execution.os,'getuid',return_value=1000):
            with self.assertRaisesRegex(RuntimeError,'stop at isolated container'):
                execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertIn('--memory=8192m',captured)
        self.assertIn('none',captured)
        self.assertFalse(list((Path(self.config['state_dir'])/self.job['run_id']).glob('step-*')))

    def test_node_resource_budget_is_bounded_and_operator_tunable(self):
        with patch.object(execution.os, 'getuid', return_value=1000):
            self.assertIn('--memory=6144m', execution.restrictions(True))
            self.assertIn('fsize=1073741824:1073741824', execution.restrictions(True))
            self.assertIn('nofile=8192:8192', execution.restrictions(True))
            self.assertIn('core=0:0', execution.restrictions(True))
            self.assertIn('--memory=8192m', execution.restrictions(True, 8192))
            self.assertIn('--memory=2048m', execution.restrictions(False))
            for value in (True, 0, '8192', 32769):
                with self.assertRaises(ValueError): execution.restrictions(True, value)

    def test_dependency_preparation_emits_a_backend_accepted_event(self):
        self.job['step'].update(id='verify',kind='check',write_repositories=[])
        adapter = {'kind':'checks','node_bootstrap':True,'image':'sha256' + ':' + 'a'*64,
                   'checks':[{'name':'tests','cwd':'api','command':['npm','test']}]}
        with patch.object(execution,'node_bootstrap_image',return_value=True), patch('node_environment.prepare',side_effect=ValueError('stop after preparation event')) as prepare:
            with self.assertRaisesRegex(ValueError,'stop after preparation event'):
                execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        prepare.assert_called_once()
        self.assertEqual(self.api.events[-1]['kind'],'command.started')
        self.assertIn('dependencias npm',self.api.events[-1]['message'])

    def test_writable_agent_gets_offline_node_dependencies_best_effort(self):
        repo = Path(self.config['state_dir']) / self.job['run_id'] / 'web'
        (repo / 'package.json').write_text('{}'); (repo / 'package-lock.json').write_text('{}')
        runner.git(['add', '.'], repo); runner.git(['-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','node'], repo)
        self.job['execution']['heads']['web'] = runner.git(['rev-parse','HEAD'], repo)
        adapter = {'kind':'agent','role':'implementation','image':'sha256:' + 'a'*64,'command':['pi']}
        staged = []
        def stop(_config, _job, adapter, inp):
            staged.append((adapter,)); raise RuntimeError('stop before container')
        with patch('node_environment.prepare') as prepare, patch('agent_runtime.stage_runtime', side_effect=stop):
            with self.assertRaisesRegex(RuntimeError, 'stop before container'):
                execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertEqual(prepare.call_args.args[1], [{'cwd':'web'}])
        self.assertEqual(staged[-1][0]['node_dependencies'], ['web'])
        # A repository that cannot be prepared offline does not block the agent.
        with patch('node_environment.prepare', side_effect=ValueError('node_lockfile_v2_or_v3_required')), patch('agent_runtime.stage_runtime', side_effect=stop):
            with self.assertRaisesRegex(RuntimeError, 'stop before container'):
                execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertNotIn('node_dependencies', staged[-1][0])
        self.assertEqual(self.api.events[-1]['kind'], 'dependency.log')
        # Read-only roles never install.
        with patch('node_environment.prepare') as prepare, patch('agent_runtime.stage_runtime', side_effect=stop):
            with self.assertRaisesRegex(RuntimeError, 'stop before container'):
                execution.isolated_adapter(self.config,self.job,dict(adapter,role='investigation'),[self.root],self.api,runner.git,runner.private_file)
        prepare.assert_not_called()
        from container_entry import agent_prompt
        self.assertIn('never run npm install/ci', agent_prompt({**self.job, 'node_dependencies':['web']}, {'role':'implementation'}))
        self.assertNotIn('npm install/ci', agent_prompt(self.job, {'role':'implementation'}))

    def test_step_copy_is_removed_whether_the_step_succeeds_or_fails(self):
        run_root = Path(self.config['state_dir']) / self.job['run_id']
        adapter = {'kind':'agent','role':'implementation','image':'sha256:' + 'a'*64,'command':['pi']}
        def stop(_config, _job, _adapter, inp):
            (inp.parent / 'work' / 'api' / 'node_modules' / 'pkg').mkdir(parents=True)
            (inp.parent / 'work' / 'api' / 'node_modules').chmod(0o500)
            raise RuntimeError('stop before container')
        with patch('agent_runtime.stage_runtime', side_effect=stop):
            with self.assertRaisesRegex(RuntimeError, 'stop before container'):
                execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file)
        self.assertEqual(list(run_root.glob('step-*')), [])
        self.assertTrue((run_root / 'api' / 'code.txt').is_file())
        with patch.object(execution, 'staged_adapter', return_value=(True, [], {})):
            self.assertEqual(execution.isolated_adapter(self.config,self.job,adapter,[self.root],self.api,runner.git,runner.private_file), (True, [], {}))
        self.assertEqual(list(run_root.glob('step-*')), [])

    def test_verified_build_is_reused_only_for_the_same_change_revision(self):
        work = self.base / 'work'
        built = work / 'web' / '.next'
        (built / 'cache').mkdir(parents=True)
        (built / 'BUILD_ID').write_text('b1'); (built / 'cache' / 'big').write_text('x')
        export = {'check': 'build-0', 'cwd': 'web'}
        execution.export_build(self.config, self.job, work, export, [{'name': 'build-0', 'exit_code': 1}])
        self.assertIsNone(execution.verified_build(self.config, self.job, 'web'))
        execution.export_build(self.config, self.job, work, export, [{'name': 'build-0', 'exit_code': 0}])
        reused = execution.verified_build(self.config, self.job, 'web')
        self.assertEqual((reused / 'BUILD_ID').read_text(), 'b1')
        self.assertFalse((reused / 'cache').exists())
        self.assertIsNone(execution.verified_build(self.config, self.job, 'api'))
        moved = dict(self.job, execution=dict(self.job['execution'], revision=1))
        self.assertIsNone(execution.verified_build(self.config, moved, 'web'))

    def test_fast_lane_completes_selected_phase_without_an_agent(self):
        job = dict(self.job, fast_lane={'skip': True, 'classification': {'task_class': 'bugfix', 'risk': 0.4}})
        job['step'] = dict(job['step'], id='tests', adapter='agent')
        api = Mock()
        api.call.side_effect = lambda path, data=None: ({'workspaces': []} if path == '/workspaces' else {'job': job} if path == '/claim'
                                                        else {'digest': 'a' * 64} if path.endswith('/artifacts') else {})
        config = {**self.config, 'api_url': 'http://localhost', 'adapters': {'agent': {}}}
        with patch.object(runner, 'API', return_value=api), patch.object(runner, 'reconcile'), patch.object(execution, 'isolated_adapter') as agent:
            self.assertTrue(runner.tick_locked(config, [self.root]))
        agent.assert_not_called()
        completion = next(call.args[1] for call in api.call.call_args_list if call.args[0].endswith('/complete'))
        self.assertTrue(completion['success'])
        self.assertEqual(completion['evidence'][0]['status'], 'not_applicable')
        self.assertTrue(completion['outcome']['output']['fast_lane'])
        self.assertTrue(completion['outcome']['output']['not_applicable_reason'])
        self.assertEqual(completion['outcome']['heads'], job['execution']['heads'])

    def test_qa_checks_run_in_parallel_and_extra_slots_claim_only_qa(self):
        import threading
        jobs = []
        for step in ('functional', 'design', 'security'):
            job = dict(self.job, lease_token=step)
            job['step'] = {'id': step, 'kind': 'check', 'adapter': 'isolated-' + step + '-qa', 'write_repositories': [], 'timeout_seconds': 60}
            jobs.append(job)
        queue = list(jobs)
        claims = []
        def call(path, data=None):
            if path == '/workspaces': return {'workspaces': []}
            if path == '/claim':
                claims.append(data)
                return {'job': queue.pop(0) if queue else None}
            return {'status': 'running'} if path.endswith('/complete') else {}
        api = Mock(); api.call.side_effect = call
        together = threading.Barrier(3, timeout=10)
        def adapter(config, job, *rest):
            together.wait()  # Fails (BrokenBarrierError) unless all three run at once.
            return True, [], {'input_revision': 0, 'heads': job['execution']['heads'], 'output': {}, 'cost_usd': 0}
        config = {**self.config, 'api_url': 'http://localhost', 'adapters': {j['step']['adapter']: {} for j in jobs}}
        with patch.object(runner, 'API', return_value=api), patch.object(runner, 'reconcile'), patch.object(execution, 'isolated_adapter', side_effect=adapter):
            runner.tick_locked(config, [self.root])
        self.assertEqual([c.get('qa_only', False) for c in claims], [False, True, True])
        completed = sorted(call.args[0].split('/')[-2] for call in api.call.call_args_list if call.args[0].endswith('/complete'))
        self.assertEqual(completed, ['design', 'functional', 'security'])
        run_root = Path(self.config['state_dir']) / self.job['run_id']
        self.assertEqual(list(run_root.glob('pending-completion*.json')), [])

    def test_api_contract_rejections_hold_instead_of_exhausting_attempts(self):
        # Every control-plane failure holds: hold=false on a check/review step queues
        # an agent repair, which an unavailable backend must never trigger.
        for status,code,hold in [(400,'invalid_event',True),(403,'policy_never',True),(429,'temporarily_limited',True),(503,'temporary_upstream_failure',True)]:
            with self.subTest(status=status):
                error = urllib.error.HTTPError('http://localhost',status,'fixture',{},io.BytesIO(json.dumps({'code':code}).encode()))
                api = Mock()
                api.call.side_effect = lambda path,data=None: ({'workspaces':[]} if path=='/workspaces' else {'job':self.job} if path=='/claim' else {})
                config = {**self.config,'api_url':'http://localhost','adapters':{'agent':{}}}
                with patch.object(runner,'API',return_value=api), patch.object(runner,'reconcile'), patch.object(execution,'isolated_adapter',side_effect=error):
                    runner.tick_locked(config,[self.root])
                completion = next(call.args[1] for call in api.call.call_args_list if call.args[0].endswith('/complete'))
                self.assertFalse(completion['success'])
                self.assertEqual(completion['outcome']['output']['error'],code)
                self.assertEqual(completion['outcome']['output']['hold'],hold)
                self.assertEqual(completion['outcome']['output']['retryable'],status in (429,503))
                if code=='invalid_event':
                    self.assertTrue(completion['outcome']['output']['solution'])

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
