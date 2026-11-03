"""Non-secret remote settings, bounded by explicit local runner policy."""
import copy,json,subprocess,time
from pathlib import Path
ROLES={'investigation':'isolated-investigator','test_author':'isolated-test-author','implementation':'isolated-implementer','reviewer':'isolated-reviewer'}
PHASES={'tests','build','lint','typecheck','format'}
class RuntimeSettingsError(ValueError):pass

def _model(config,name,roots,private_file):
    from urllib.parse import urlsplit
    value=config.get('model_connections',{}).get(name)
    if not isinstance(value,dict) or not value.get('env_file'):raise RuntimeSettingsError('runtime_model_connection_missing')
    original=Path(value['env_file']);path=original.resolve(strict=True)
    if original.is_symlink() or any(path==root or root in path.parents for root in roots):raise RuntimeSettingsError('runtime_model_connection_missing')
    values={}
    for line in private_file(path).splitlines():
        if not line.strip() or line.lstrip().startswith('#'):continue
        if '=' not in line:raise RuntimeSettingsError('runtime_model_connection_missing')
        key,text=line.split('=',1);values[key]=text
    # Separate proxy authentication from application environments. Reviewers
    # receive only this model connection, never application environment refs.
    allowed={'ANTHROPIC_BASE_URL','ANTHROPIC_AUTH_TOKEN','ANTHROPIC_API_KEY'}
    url=urlsplit(values.get('ANTHROPIC_BASE_URL',''))
    if url.scheme not in ('http','https') or not url.hostname or url.hostname.rstrip('.').lower().endswith('anthropic.com') or values.get('ANTHROPIC_AUTH_TOKEN','').startswith(('sk-','ghp_','github_pat_')) or url.username or url.password or set(values)-allowed or values.get('ANTHROPIC_API_KEY','sandbox-placeholder-not-a-credential')!='sandbox-placeholder-not-a-credential':raise RuntimeSettingsError('runtime_model_connection_missing')
    return path,list(values.values())

def inventory(config,workspace,roots,private_file):
    report={'remote_enabled':bool(config.get('allow_remote_configuration',False)),'applied_revision':0,'images':[],'networks':[],'model_connections':[],'environments':[]}
    runtime=config.get('container_runtime','docker')
    if runtime not in ('docker','podman'):return report
    def inspect(args,output):
        try:return subprocess.run([runtime,*args],stdout=output,stderr=subprocess.DEVNULL,timeout=8)
        except (OSError,subprocess.TimeoutExpired):return None
    for image in config.get('allowed_images',[])[:100]:
        r=inspect(['image','inspect',image],subprocess.DEVNULL)
        if r is not None and r.returncode==0:report['images'].append(image)
    for network in config.get('allowed_egress_networks',[])[:100]:
        r=inspect(['network','inspect','--format','{{.Internal}}',network],subprocess.PIPE)
        if r is not None and r.returncode==0 and r.stdout.strip()==b'true':report['networks'].append(network)
    for name in config.get('model_connections',{}):
        try:_model(config,name,roots,private_file);report['model_connections'].append(name)
        except (ValueError,OSError):pass
    for name,path in config.get('environments',{}).get(workspace['id'],{}).items():
        try:
            from execution import environment
            environment(config,workspace['id'],[name],roots,private_file);report['environments'].append(name)
        except (ValueError,OSError):pass
    return report

def compile_settings(profile):
    adapters={}
    for agent in profile.get('agents',[]):
        role=agent['role'];tools='Read,Grep,Glob' if role in ('investigation','reviewer') else 'Read,Edit,Write,Grep,Glob,Bash,Task'
        adapters[ROLES[role]]={'kind':'agent','format':'claude','role':role,'image':agent['image'],'network':agent['network'],'model_connection':agent['model_connection'],'command':['claude','--model',agent['model'],'--permission-mode','bypassPermissions','--allowedTools',tools]}
    for phase,alias in [('verify','isolated-verification'),('functional','isolated-functional-qa'),('design','isolated-design-qa'),('security','isolated-security-qa')]:
        checks=[c for c in profile.get('checks',[]) if c['name'] in PHASES] if phase=='verify' else [c for c in profile.get('checks',[]) if c['name']==phase]
        if checks:adapters[alias]={'kind':'suite','checks':checks}
    services={s['name']:{k:v for k,v in s.items() if k!='name'} for s in profile.get('services',[])}
    return adapters,services

def apply_settings(config,workspace,profile,report,roots,private_file):
    if not report['remote_enabled']:raise RuntimeSettingsError('runtime_invalid')
    for item in [*profile.get('agents',[]),*profile.get('checks',[]),*profile.get('services',[])]:
        if item['image'] not in report['images']:raise RuntimeSettingsError('runtime_image_not_allowed')
        if any(ref not in report['environments'] for ref in item.get('environment_refs',[])):raise RuntimeSettingsError('runtime_environment_missing')
    for agent in profile.get('agents',[]):
        if agent['network'] not in report['networks'] or config.get('model_connections',{}).get(agent['model_connection'],{}).get('network')!=agent['network']:raise RuntimeSettingsError('runtime_network_not_allowed')
        if agent['model_connection'] not in report['model_connections']:raise RuntimeSettingsError('runtime_model_connection_missing')
    adapters,services=compile_settings(profile)
    value=copy.deepcopy(config);value.setdefault('adapters',{}).update(adapters)
    if services:value.setdefault('workspace_profiles',{})[workspace['id']]={'services':services}
    return value

def sync(config,workspace,api,roots,private_file):
    report=inventory(config,workspace,roots,private_file)
    runtime=workspace.get('runtime',{})
    if runtime.get('revision',0)>0 and report['remote_enabled']:
        try:apply_settings(config,workspace,runtime['config'],report,roots,private_file);report['applied_revision']=runtime['revision']
        except RuntimeSettingsError as e:report['error']=str(e)
    api.call('/workspaces/'+workspace['id']+'/runtime-report',report)
    return report

def for_job(config,job,roots,private_file):
    workspace=job['snapshot']['workspace'];runtime=workspace.get('runtime',{})
    if not runtime.get('revision') or job['step']['kind']=='prepare':return config
    report=inventory(config,workspace,roots,private_file)
    return apply_settings(config,workspace,runtime['config'],report,roots,private_file)

def suite(config,job,profile,roots,api,git,private_file):
    from execution import isolated_adapter
    from delivery import validate_services
    evidence=[];start=time.monotonic();success=True
    required=set(job['step'].get('options',{}).get('required_checks',[]))
    if {'tests','build'}.issubset(required):
        changed=[r['path'] for r in job['snapshot']['workspace']['repositories'] if job['execution']['heads'][r['path']]!=r['head_sha']]
        if any(not any(c['name']==role and c['cwd']==repo for c in profile['checks']) for repo in changed for role in ('tests','build')):
            raise RuntimeSettingsError('Tests and build must cover every changed repository')
    for index,check in enumerate(profile['checks']):
        if time.monotonic()-start>=job['step']['timeout_seconds']:raise RuntimeSettingsError('Suite time budget exceeded')
        role=check['name'];unique=role+'-'+str(index);child=copy.deepcopy(job)
        child['step'].setdefault('options',{})['required_checks']=[unique]
        child['step']['timeout_seconds']=max(1,int(job['step']['timeout_seconds']-(time.monotonic()-start)))
        adapter={'kind':'checks','image':check['image'],'environment_refs':check.get('environment_refs',[]),'checks':[{'name':unique,'cwd':check['cwd'],'command':check['command']}]}
        if role in ('functional','design','security'):adapter['network']='services';validate_services(config,job)
        class NamedArtifacts:
            def call(self,path,data=None):
                if path.endswith('/artifacts'):data=dict(data,name=unique+'-'+data['name'])
                return api.call(path,data)
        passed,items,_=isolated_adapter(config,child,adapter,roots,NamedArtifacts(),git,private_file)
        for item in items:item['check']=role
        evidence.extend(items);success=success and passed
        if adapter.get('network')=='services':validate_services(config,job)
    required=set(job['step'].get('options',{}).get('required_checks',[]))
    if not required.issubset({e.get('check') for e in evidence}):success=False
    return success,evidence,{'input_revision':job['execution']['revision'],'heads':job['execution']['heads'],'output':{'checks':evidence},'cost_usd':0}
