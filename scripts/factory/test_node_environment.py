import base64,copy,hashlib,io,json,os,tarfile,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
import node_environment as node

class NodeEnvironmentTests(unittest.TestCase):
    def fixture(self):
        stream=io.BytesIO()
        with tarfile.open(fileobj=stream,mode='w:gz') as tar:
            for name,data in {'package/package.json':b'{"name":"nm-fixture","version":"1.0.0"}','package/index.js':b'module.exports=42'}.items():
                info=tarfile.TarInfo(name);info.size=len(data);tar.addfile(info,io.BytesIO(data))
        data=stream.getvalue();integrity='sha512-'+base64.b64encode(hashlib.sha512(data).digest()).decode()
        package={'name':'fixture','version':'1.0.0','dependencies':{'nm-fixture':'1.0.0'},'scripts':{'preinstall':'node -e "require(\'fs\').writeFileSync(\'SHOULD_NOT_EXIST\',\'bad\')"'}}
        lock={'name':'fixture','version':'1.0.0','lockfileVersion':3,'packages':{'':package,'node_modules/nm-fixture':{'version':'1.0.0','resolved':'https://registry.npmjs.org/nm-fixture/-/nm-fixture-1.0.0.tgz','integrity':integrity}}}
        return data,package,lock
    def test_private_external_redirected_or_unpinned_dependencies_block(self):
        data,package,lock=self.fixture();self.assertEqual(len(node.packages(lock)),1)
        for url in ('https://npm.pkg.github.com/private.tgz','http://registry.npmjs.org/p.tgz','https://registry.npmjs.org@127.0.0.1/p','https://registry.npmjs.org/p?token=fixture','file:/tmp/package'):
            bad=copy.deepcopy(lock);bad['packages']['node_modules/nm-fixture']['resolved']=url
            with self.assertRaises(node.NodeEnvironmentError):node.packages(bad)
        bad=copy.deepcopy(lock);bad['packages']['node_modules/nm-fixture']['integrity']='sha512-invalid'
        with self.assertRaises(node.NodeEnvironmentError):node.packages(bad)
        with self.assertRaises(node.NodeEnvironmentError):node.NoRedirect().redirect_request(None,None,None,None,None,None)
    def test_fetch_is_integrity_bound_cached_and_validated_before_network(self):
        data,package,lock=self.fixture()
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);repo=root/'web';repo.mkdir();(repo/'package.json').write_text(json.dumps(package));(repo/'package-lock.json').write_text(json.dumps(lock))
            with patch.object(node.urllib.request,'build_opener') as build:
                build.return_value.open.return_value.__enter__.return_value.read.return_value=data
                node.prepare(root,[{'cwd':'web'}],root/'input',root/'cache')
                self.assertEqual(build.return_value.open.call_count,1)
                node.prepare(root,[{'cwd':'web'}],root/'input2',root/'cache');self.assertEqual(build.return_value.open.call_count,1)
            with patch.object(node.urllib.request,'build_opener') as build:
                build.return_value.open.return_value.__enter__.return_value.read.return_value=b'altered'
                with self.assertRaises(node.NodeEnvironmentError):node.prepare(root,[{'cwd':'web'}],root/'tampered',root/'other-cache')
            (repo/'package-lock.json').write_text(json.dumps({'lockfileVersion':1}))
            with patch.object(node.urllib.request,'build_opener') as build:
                with self.assertRaises(node.NodeEnvironmentError):node.prepare(root,[{'cwd':'web'}],root/'bad',root/'cache')
                build.assert_not_called()
    @unittest.skipUnless(os.environ.get('FACTORY_NODE_IMAGE'),'Requires the installed immutable Node SDK')
    def test_real_offline_node_check_prepares_dependencies_without_running_install_scripts(self):
        import test_execution as fixtures,runner,execution
        f=fixtures.ExecutionTests();f.setUp()
        try:
            data,package,lock=self.fixture();repo=Path(f.config['state_dir'])/f.job['run_id']/'web'
            (repo/'package.json').write_text(json.dumps(package));(repo/'package-lock.json').write_text(json.dumps(lock));runner.git(['add','.'],repo);runner.git(['-c','user.name=Test','-c','user.email=test@example.com','commit','-qm','Node fixture'],repo)
            f.job['execution']['heads']['web']=runner.git(['rev-parse','HEAD'],repo)
            f.job['step']={'id':'verify','kind':'check','adapter':'qa','write_repositories':[],'timeout_seconds':90,'options':{'required_checks':['tests']}}
            image=os.environ['FACTORY_NODE_IMAGE'];f.config['managed_images']={'node':image}
            adapter={'kind':'checks','image':image,'node_bootstrap':True,'checks':[{'name':'tests','cwd':'web','command':['node','-e',"const a=require('assert');a.equal(require('nm-fixture'),42);a(!require('fs').existsSync('SHOULD_NOT_EXIST'));a(require('fs').existsSync('node_modules'))"]}]}
            with patch.object(node.urllib.request,'build_opener') as build:
                build.return_value.open.return_value.__enter__.return_value.read.return_value=data
                passed,_,_=execution.isolated_adapter(f.config,f.job,adapter,[f.root],f.api,runner.git,runner.private_file)
            self.assertTrue(passed,repr(f.api.artifacts))
            self.assertTrue(any('dependencies.log' in name for name,_ in f.api.artifacts))
            self.assertFalse((repo/'node_modules').exists(),'No dependencies written to authoritative clone')
        finally:f.tearDown()

if __name__=='__main__':unittest.main()
