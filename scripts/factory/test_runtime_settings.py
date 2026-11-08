import copy,json,os,tempfile,unittest
from pathlib import Path
from unittest.mock import Mock, patch
import runtime_settings as settings
from runner import private_file

class RuntimeSettingsTests(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.base=Path(self.tmp.name);self.root=self.base/'repos';self.root.mkdir();self.env=self.base/'model.env';self.env.write_text('ANTHROPIC_BASE_URL=http://model-proxy:8080\nANTHROPIC_AUTH_TOKEN=lease-fixture\nANTHROPIC_API_KEY=sandbox-placeholder-not-a-credential\n');self.env.chmod(0o600)
  self.image='sha256:'+'a'*64
  self.config={'runner_token':'nmr_private','allowed_images':[self.image],'allow_remote_configuration':True,'model_connections':{'model':{'env_file':str(self.env),'network':'model-net'}},'environments':{},'adapters':{}}
  self.profile={'agents':[{'role':'reviewer','model':'opus','image':self.image,'network':'model-net','model_connection':'model'}],'checks':[{'name':'tests','image':self.image,'cwd':'web','command':['npm','test'],'environment_refs':[]}],'services':[]}
  self.report={'remote_enabled':True,'images':[self.image],'networks':['model-net'],'model_connections':['model'],'environments':[]}
 def tearDown(self):self.tmp.cleanup()
 def test_local_opt_in_and_image_network_reference_allowlists_are_required(self):
  for changed,code in [({'remote_enabled':False},'runtime_invalid'),({'images':[]},'runtime_image_not_allowed'),({'networks':[]},'runtime_network_not_allowed'),({'model_connections':[]},'runtime_model_connection_missing')]:
   report=dict(self.report,**changed)
   with self.assertRaisesRegex(settings.RuntimeSettingsError,code):settings.apply_settings(self.config,{'id':'w'},self.profile,report,[self.root],private_file)
  result=settings.apply_settings(self.config,{'id':'w'},self.profile,self.report,[self.root],private_file)
  reviewer=result['adapters']['isolated-reviewer'];self.assertEqual(reviewer['model_connection'],'model');self.assertNotIn('environment_refs',reviewer);self.assertEqual(reviewer['command'][-1],'Read,Grep,Glob');self.assertNotIn('lease-fixture',json.dumps(result['adapters']))
 def test_model_credentials_are_distinct_from_application_environment_and_master_keys(self):
  path,secrets=settings._model(self.config,'model',[self.root],private_file);self.assertEqual(path,self.env);self.assertIn('lease-fixture',secrets)
  for text in ['ANTHROPIC_BASE_URL=http://proxy\nDATABASE_PASSWORD=private\n','ANTHROPIC_BASE_URL=https://api.anthropic.com\n','ANTHROPIC_BASE_URL=http://proxy\nANTHROPIC_API_KEY=sk-master-fixture\n']:
   self.env.write_text(text)
   with self.assertRaises(settings.RuntimeSettingsError):settings._model(self.config,'model',[self.root],private_file)
 def test_run_uses_immutable_snapshot_not_newest_remote_profile(self):
  job={'step':{'kind':'agent'},'snapshot':{'workspace':{'id':'w','runtime':{'revision':1,'config':self.profile}}}}
  with patch.object(settings,'inventory',return_value=self.report):
   value=settings.for_job(self.config,job,[self.root],private_file)
  self.assertIn('opus',value['adapters']['isolated-reviewer']['command'])
  self.assertEqual(self.config['adapters'],{})
  job['step']['kind']='prepare';self.assertIs(settings.for_job(self.config,job,[self.root],private_file),self.config)
 def test_harnesses_use_only_operator_registered_commands_and_never_resume_sessions(self):
  profile=copy.deepcopy(self.profile);profile['agents'][0].update(harness='codex',model='gpt-example',binding={'id':'agent','revision':2})
  with self.assertRaisesRegex(settings.RuntimeSettingsError,'runtime_harness_missing'):settings.compile_settings(profile,self.config)
  self.config['harnesses']={'codex':{'image':self.image,'format':'json','command':['reviewed-wrapper','--model','{model}']}}
  adapters,_=settings.compile_settings(profile,self.config);adapter=adapters['isolated-reviewer'];self.assertEqual(adapter['command'],['reviewed-wrapper','--model','gpt-example']);self.assertEqual(adapter['format'],'json');self.assertEqual(adapter['agent_binding']['revision'],2)
  self.config['harnesses']['codex']['command'].append('--resume=old-session')
  with self.assertRaisesRegex(settings.RuntimeSettingsError,'runtime_harness_missing'):settings.compile_settings(profile,self.config)
 def test_partial_profiles_never_apply_without_a_model(self):
  self.profile['agents'][0]['model']=''
  with self.assertRaisesRegex(settings.RuntimeSettingsError,'runtime_invalid'):settings.apply_settings(self.config,{'id':'w'},self.profile,self.report,[self.root],private_file)
 def test_openai_compatible_proxy_is_not_a_provider_master_key_or_app_environment(self):
  self.env.write_text('OPENAI_BASE_URL=http://model-proxy:8080/v1\nOPENAI_API_KEY=proxy-lease-fixture\n')
  self.assertEqual(settings._model(self.config,'model',[self.root],private_file)[0],self.env)
  self.env.write_text('OPENAI_BASE_URL=https://api.openai.com/v1\nOPENAI_API_KEY=proxy-lease-fixture\n')
  with self.assertRaises(settings.RuntimeSettingsError):settings._model(self.config,'model',[self.root],private_file)
  self.env.write_text('OPENAI_BASE_URL=http://model-proxy/v1\nOPENAI_API_KEY=sk-provider-master-fixture\n')
  with self.assertRaises(settings.RuntimeSettingsError):settings._model(self.config,'model',[self.root],private_file)
 def test_browser_sdk_dependency_preparation_requires_local_operator_opt_in(self):
  from execution import node_bootstrap_image
  qa='sha256:'+'b'*64
  self.config['managed_images']={'node':self.image}
  self.assertTrue(node_bootstrap_image(self.config,self.image));self.assertFalse(node_bootstrap_image(self.config,qa))
  self.config['node_bootstrap_images']=[qa]
  self.assertTrue(node_bootstrap_image(self.config,qa));self.assertFalse(node_bootstrap_image(self.config,'sha256:'+'c'*64))
 def test_multiple_repositories_keep_separate_checks_and_images(self):
  self.profile['checks'] += [{'name':'build','image':'sha256:'+'b'*64,'cwd':'api','command':['cargo','build'],'environment_refs':[]}]
  adapters,_=settings.compile_settings(self.profile);self.assertEqual(adapters['isolated-verification']['kind'],'suite');self.assertEqual(len(adapters['isolated-verification']['checks']),2)
 def test_missing_tests_or_build_for_a_changed_repository_blocks_before_execution(self):
  job={'step':{'options':{'required_checks':['tests','build']}},'snapshot':{'workspace':{'repositories':[{'path':'api','head_sha':'base'}]}},'execution':{'heads':{'api':'candidate'}}}
  with self.assertRaisesRegex(settings.RuntimeSettingsError,'every changed repository'):settings.suite({},job,{'checks':self.profile['checks']},[],None,None,None)
 @unittest.skipUnless(os.environ.get('FACTORY_TEST_IMAGE'),'Requires a locally installed immutable Docker fixture image')
 def test_real_multi_repository_suite_verifies_final_commits_and_keeps_unique_artifacts(self):
  import test_execution as fixtures,runner,delivery
  f=fixtures.ExecutionTests();f.setUp()
  try:
   checks=[]
   for repo in ['api','web']:
    directory=Path(f.config['state_dir'])/f.job['run_id']/repo;(directory/'module.py').write_text('VALUE=2\n');runner.git(['add','module.py'],directory);runner.git(['-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','candidate'],directory);f.job['execution']['heads'][repo]=runner.git(['rev-parse','HEAD'],directory)
    checks += [{'name':'tests','cwd':repo,'image':os.environ['FACTORY_TEST_IMAGE'],'command':['python3','-c',f"from pathlib import Path;ns={{}};exec(Path('/work/{repo}/module.py').read_text(),ns);assert ns['VALUE']==2"],'environment_refs':[]},{'name':'build','cwd':repo,'image':os.environ['FACTORY_TEST_IMAGE'],'command':['python3','-c',f"import py_compile;py_compile.compile('/work/{repo}/module.py',cfile='/output/{repo}.pyc',doraise=True)"],'environment_refs':[]}]
   f.job['step']={'id':'verify','kind':'check','adapter':'qa','write_repositories':[],'timeout_seconds':60,'options':{'required_checks':['tests','build']}}
   passed,evidence,result=settings.suite(f.config,f.job,{'checks':checks},[f.root],f.api,runner.git,runner.private_file)
   self.assertTrue(passed,repr(f.api.artifacts));self.assertEqual(len(evidence),4);self.assertEqual(result['heads'],f.job['execution']['heads']);names=[name for name,_ in f.api.artifacts];self.assertEqual(len(names),len(set(names)))
  finally:f.tearDown()
 def test_only_requested_operator_managed_digest_images_are_pulled_automatically(self):
  from types import SimpleNamespace
  self.config['managed_images']={'node':self.image};workspace={'id':'w','runtime':{'config':self.profile}}
  with patch.object(settings.subprocess,'run',side_effect=[SimpleNamespace(returncode=1),SimpleNamespace(returncode=0),SimpleNamespace(returncode=0)]) as run:
   report=settings.inventory(self.config,workspace,[self.root],private_file)
  self.assertEqual(report['images'],[self.image]);self.assertEqual(report['managed_images'],{'node':self.image});self.assertEqual(run.call_args_list[1].args[0],['docker','pull',self.image])
  with patch.object(settings.subprocess,'run',return_value=SimpleNamespace(returncode=1)) as run:
   settings.inventory(self.config,{'id':'w'},[self.root],private_file)
  self.assertFalse(any('pull' in call.args[0] for call in run.call_args_list))
 def test_report_contains_only_aliases_and_no_environment_contents_or_paths(self):
  self.config['allowed_images']=[]
  report=settings.inventory(self.config,{'id':'w'},[self.root],private_file)
  self.assertEqual(report['model_connections'],['model']);self.assertNotIn(str(self.env),json.dumps(report));self.assertNotIn('lease-fixture',json.dumps(report))


class ValidationPlanSuiteTests(unittest.TestCase):
    def job(self):
        return {'run_id':'r','lease_token':'t','step':{'id':'verify','timeout_seconds':60,'options':{'required_checks':[]}},
                'execution':{'revision':1,'heads':{'api':'b'*40,'web':'c'*40}},
                'snapshot':{'workspace':{'repositories':[{'path':'api','head_sha':'a'*40},{'path':'web','head_sha':'c'*40}]}}}
    def profile(self):
        return {'kind':'suite','checks':[{'name':'tests','cwd':'api','image':'i','command':['x']},{'name':'tests','cwd':'web','image':'i','command':['x']},{'name':'build','cwd':'web','image':'i','command':['x']}]}
    def run_suite(self,decisions,adapter_effect=None):
        import runtime_settings as rs
        calls=[];api=Mock()
        def call(path,data=None):
            if path.endswith('/validation-plan'):
                if decisions is None:
                    from control_plane import ControlPlaneError
                    raise ControlPlaneError('POST','/validation-plan','rid',status=404)
                return {'decisions':decisions}
            if path.endswith('/artifacts'):return {'digest':'d'*64}
            return {}
        api.call.side_effect=call
        def adapter(config,child,adapter,roots,named,git,private_file):
            calls.append((child['step']['options']['required_checks'][0],adapter['checks'][0]['cwd']))
            if adapter_effect:adapter_effect(adapter)
            return True,[{'check':adapter['checks'][0]['name'],'status':'pass'}],{}
        with patch('execution.isolated_adapter',side_effect=adapter),patch.object(rs,'repository_facts',return_value=[]):
            return rs.suite({'state_dir':'/tmp'},self.job(),self.profile(),[],api,Mock(),Mock()),calls
    def test_skipped_checks_do_not_run_and_are_reported_as_not_applicable(self):
        decisions=[{'check':{'name':'tests','cwd':'api'},'run':True},{'check':{'name':'tests','cwd':'web'},'run':False},{'check':{'name':'build','cwd':'web'},'run':False}]
        (success,evidence,_),calls=self.run_suite(decisions)
        self.assertTrue(success)
        self.assertEqual(calls,[('tests-0','api')])
        self.assertEqual(sorted(e['status'] for e in evidence),['not_applicable','not_applicable','pass'])
    def test_output_stays_bounded_with_many_skipped_checks(self):
        import runtime_settings as rs
        heads={'repo-%02d'%i:'a'*40 for i in range(60)}
        evidence=[{'check':'build','status':'not_applicable','commits':heads,'artifact_digest':'d'*64} for _ in range(200)]
        evidence.append({'check':'tests','status':'fail','commits':heads})
        out=rs.suite_output(evidence,[{'check':'tests','cwd':'api'}]*80)
        self.assertLess(len(json.dumps(out)),16384)
        self.assertEqual(out['checks'],{'not_applicable':200,'fail':1})
        self.assertEqual(out['failed_total'],80);self.assertEqual(len(out['failed']),50)
    def test_failed_checks_are_named_in_output(self):
        decisions=[{'check':{'name':'tests','cwd':'api'},'run':True},{'check':{'name':'tests','cwd':'web'},'run':False},{'check':{'name':'build','cwd':'web'},'run':False}]
        import runtime_settings as rs
        api=Mock()
        api.call.side_effect=lambda path,data=None:{'decisions':decisions} if path.endswith('/validation-plan') else {'digest':'d'*64}
        def adapter(config,child,adapter,roots,named,git,private_file):
            return False,[{'check':adapter['checks'][0]['name'],'status':'fail'}],{}
        with patch('execution.isolated_adapter',side_effect=adapter),patch.object(rs,'repository_facts',return_value=[]):
            success,_,outcome=rs.suite({'state_dir':'/tmp'},self.job(),self.profile(),[],api,Mock(),Mock())
        self.assertFalse(success)
        self.assertEqual(outcome['output']['failed'],[{'check':'tests','cwd':'api'}])
        self.assertEqual(outcome['output']['checks'],{'not_applicable':2,'fail':1})
    def test_older_backend_runs_every_check(self):
        (_,evidence,_),calls=self.run_suite(None)
        self.assertEqual(len(calls),3)
    def test_repository_without_lockfile_is_not_applicable_instead_of_aborting(self):
        def effect(adapter):
            if adapter['checks'][0]['cwd']=='web':raise ValueError('node_committed_package_lock_required')
        (success,evidence,_),calls=self.run_suite(None,effect)
        self.assertEqual(len(calls),3)
        self.assertEqual([e['status'] for e in evidence].count('not_applicable'),2)
        def other(adapter):raise ValueError('something_else')
        with self.assertRaises(ValueError):self.run_suite(None,other)



class CommentOnlyTests(unittest.TestCase):
    def test_comment_only_diffs(self):
        import runtime_settings as rs
        diff='diff --git a/x.ts b/x.ts\n--- a/x.ts\n+++ b/x.ts\n@@ -1,0 +2,3 @@\n+// note\n+/* block\n+ * more */\n+\n'
        self.assertTrue(rs.comment_only(diff,'src/x.ts'))
        self.assertFalse(rs.comment_only(diff.replace('+// note','+const a = 1'),'src/x.ts'))
        self.assertFalse(rs.comment_only('@@ -1 +1 @@\n+#private = 1\n','src/x.ts'),'# is code in TypeScript')
        self.assertTrue(rs.comment_only('@@ -1 +1 @@\n+# setting docs\n','config.yml'))
        self.assertFalse(rs.comment_only('Binary files a/x and b/x differ','x.png'))



class EnvironmentPlanTests(unittest.TestCase):
    def job(self):
        return {'run_id':'r','lease_token':'t','step':{'id':'environment'},'execution':{'revision':2,'heads':{'api':'b'*40}},
                'snapshot':{'workspace':{'id':'w','repositories':[{'path':'api','head_sha':'a'*40}]}}}
    def test_environment_is_skipped_only_when_floors_find_no_code_change(self):
        import delivery, runtime_settings as rs
        skip=[{'check':{'name':'environment','cwd':'api'},'run':False,'source':'floor'}]
        with patch.object(rs,'validation_plan',return_value=skip),patch.object(rs,'skip_evidence',return_value='d'*64):
            ok,evidence,out=delivery.services({},self.job(),[],Mock(),Mock(),Mock())
        self.assertTrue(ok);self.assertEqual(evidence[0]['status'],'not_applicable');self.assertEqual(out['output'],{'skipped':'no_code_changes'})
        jev_skip=[{'check':{'name':'environment','cwd':'api'},'run':False,'source':'jev'}]
        with patch.object(rs,'validation_plan',return_value=jev_skip),patch.object(delivery,'cleanup',side_effect=RuntimeError('normal setup reached')),self.assertRaisesRegex(RuntimeError,'normal setup'):
            delivery.services({'state_dir':'/tmp'},self.job(),[],Mock(),Mock(),Mock())  # a Jev-only skip never skips the environment

if __name__=='__main__':unittest.main()
