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
    allowed={'ANTHROPIC_BASE_URL','ANTHROPIC_AUTH_TOKEN','ANTHROPIC_API_KEY','OPENAI_BASE_URL','OPENAI_API_KEY','OPENAI_AUTH_TOKEN'}
    urls=[urlsplit(values[k]) for k in ('ANTHROPIC_BASE_URL','OPENAI_BASE_URL') if k in values]
    if not urls or any(u.scheme not in ('http','https') or not u.hostname or u.hostname.rstrip('.').lower().endswith(('anthropic.com','openai.com')) or u.username or u.password for u in urls) or any(values.get(k,'').startswith(('sk-','ghp_','github_pat_')) for k in ('ANTHROPIC_AUTH_TOKEN','OPENAI_API_KEY','OPENAI_AUTH_TOKEN')) or set(values)-allowed or values.get('ANTHROPIC_API_KEY','sandbox-placeholder-not-a-credential')!='sandbox-placeholder-not-a-credential':raise RuntimeSettingsError('runtime_model_connection_missing')
    return path,list(values.values())

def inventory(config,workspace,roots,private_file):
    provisioning_error=None
    if config.get('allow_application_builds'): config.setdefault('auto_provision_agent_runtime', True)
    try:
        from managed_agent import provision
        provision(config)
    except (OSError,ValueError,subprocess.SubprocessError):provisioning_error='runtime_image_provision_failed'
    report={'remote_enabled':bool(config.get('allow_remote_configuration',False)),'applied_revision':0,'images':[],'networks':[],'model_connections':[],'environments':[],'harnesses':['claude'],'managed_images':{k:v for k,v in config.get('managed_images',{}).items() if k in ('node','agent') and v in config.get('allowed_images',[])}}
    if provisioning_error:report['error']=provisioning_error
    report['auto_environment']=False
    runtime=config.get('container_runtime','docker')
    if runtime not in ('docker','podman'):return report
    def inspect(args,output):
        try:return subprocess.run([runtime,*args],stdout=output,stderr=subprocess.DEVNULL,timeout=8)
        except (OSError,subprocess.TimeoutExpired):return None
    for image in config.get('allowed_images',[])[:100]:
        r=inspect(['image','inspect',image],subprocess.DEVNULL)
        requested={c.get('image') for c in workspace.get('runtime',{}).get('config',{}).get('checks',[])}
        if (r is None or r.returncode!=0) and report['remote_enabled'] and image in report['managed_images'].values() and image in requested:
            try:
                from execution import pinned_image
                subprocess.run([runtime,'pull',pinned_image(image)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=240,check=True)
                r=inspect(['image','inspect',image],subprocess.DEVNULL)
            except (ValueError,OSError,subprocess.SubprocessError):report['error']='runtime_image_pull_failed'
        if r is not None and r.returncode==0:report['images'].append(image)
    if config.get('allow_application_builds') and report['managed_images'].get('agent') in report['images']:
        try:
            import yaml
            from application_environment import build_cgroup
            build_cgroup()
            report['auto_environment']=True
        except (ImportError,OSError,ValueError,subprocess.SubprocessError): pass
    if config.get('allow_backend_models') and report['managed_images'].get('agent') in report['images']:
        report['harnesses'].append('pi');report['model_connections'].append('backend');report['networks'].append('none')
    for network in config.get('allowed_egress_networks',[])[:100]:
        r=inspect(['network','inspect','--format','{{.Internal}}',network],subprocess.PIPE)
        if r is not None and r.returncode==0 and r.stdout.strip()==b'true':report['networks'].append(network)
    for name,spec in config.get('harnesses',{}).items():
        if spec.get('image') in report['images'] and spec.get('format') in ('json','claude') and isinstance(spec.get('command'),list) and spec['command'] and spec.get('driver','container')=='container':report['harnesses'].append(name)
    for name in config.get('model_connections',{}):
        try:_model(config,name,roots,private_file);report['model_connections'].append(name)
        except (ValueError,OSError):pass
    for name,path in config.get('environments',{}).get(workspace['id'],{}).items():
        try:
            from execution import environment
            environment(config,workspace['id'],[name],roots,private_file);report['environments'].append(name)
        except (ValueError,OSError):pass
    return report

def compile_settings(profile,config=None):
    adapters={}
    for agent in profile.get('agents',[]):
        role=agent['role'];tools='Read,Grep,Glob' if role in ('investigation','reviewer') else 'Read,Edit,Write,Grep,Glob,Bash,Task'
        harness=agent.get('harness','claude');local=(config or {}).get('harnesses',{}).get(harness)
        if agent.get('model_connection')=='backend':
            if harness!='pi' or not (config or {}).get('allow_backend_models') or agent['image']!=(config or {}).get('managed_images',{}).get('agent') or agent['network']!='none':raise RuntimeSettingsError('runtime_harness_missing')
            pi_tools='read,grep,find,ls' if role in ('investigation','reviewer') else 'read,bash,edit,write,grep,find,ls'
            command=['pi','--mode','json','--print','--no-session','--no-extensions','--no-skills','--no-prompt-templates','--no-themes','--no-context-files','--no-approve','--no-mcp','--tools',pi_tools,'--model','workflow/'+agent['model']];format_='pi'
        elif local:
            if local.get('driver','container')!='container' or local.get('format') not in ('json','claude') or local.get('image')!=agent['image'] or not isinstance(local.get('command'),list) or not local['command'] or any(not isinstance(x,str) for x in local['command']):raise RuntimeSettingsError('runtime_harness_missing')
            if any(x.split('=',1)[0] in ('--resume','--continue','--session','--session-id','--session-dir') for x in local['command']):raise RuntimeSettingsError('runtime_harness_missing')
            command=[x.replace('{model}',agent['model']) for x in local['command']];format_=local['format']
        elif harness=='claude':command=['claude','--model',agent['model'],'--permission-mode','bypassPermissions','--allowedTools',tools];format_='claude'
        else:raise RuntimeSettingsError('runtime_harness_missing')
        adapters[ROLES[role]]={'kind':'agent','format':format_,'role':role,'harness':harness,'model':agent['model'],'agent_binding':agent.get('binding'),'instructions':agent.get('instructions',''),'runtime_snapshot':agent.get('runtime_snapshot'),'image':agent['image'],'network':agent['network'],'model_connection':agent['model_connection'],'command':command}
    for phase,alias in [('verify','isolated-verification'),('functional','isolated-functional-qa'),('design','isolated-design-qa'),('security','isolated-security-qa')]:
        checks=[c for c in profile.get('checks',[]) if c['name'] in PHASES] if phase=='verify' else [c for c in profile.get('checks',[]) if c['name']==phase]
        if checks:adapters[alias]={'kind':'suite','checks':checks}
    services={s['name']:{k:v for k,v in s.items() if k!='name'} for s in profile.get('services',[])}
    return adapters,services

def apply_settings(config,workspace,profile,report,roots,private_file):
    if not report['remote_enabled']:raise RuntimeSettingsError('runtime_invalid')
    if profile.get('auto_environment') and not profile.get('services') and not report.get('auto_environment'):
        raise RuntimeSettingsError('runtime_auto_environment_not_allowed')
    for item in [*profile.get('agents',[]),*profile.get('checks',[]),*profile.get('services',[])]:
        if item['image'] not in report['images']:raise RuntimeSettingsError('runtime_image_not_allowed')
        if any(ref not in report['environments'] for ref in item.get('environment_refs',[])):raise RuntimeSettingsError('runtime_environment_missing')
    for agent in profile.get('agents',[]):
        if not agent.get('model'):raise RuntimeSettingsError('runtime_invalid')
        if agent.get('harness','claude') not in report.get('harnesses',['claude']):raise RuntimeSettingsError('runtime_harness_missing')
        if agent['model_connection']=='backend':
            if not config.get('allow_backend_models') or agent['network']!='none':raise RuntimeSettingsError('runtime_network_not_allowed')
        elif agent['network'] not in report['networks'] or config.get('model_connections',{}).get(agent['model_connection'],{}).get('network')!=agent['network']:raise RuntimeSettingsError('runtime_network_not_allowed')
        if agent['model_connection'] not in report['model_connections']:raise RuntimeSettingsError('runtime_model_connection_missing')
    adapters,services=compile_settings(profile,config)
    value=copy.deepcopy(config);value.setdefault('adapters',{}).update(adapters)
    if services:value.setdefault('workspace_profiles',{})[workspace['id']]={'services':services}
    return value

def sync(config,workspace,api,roots,private_file):
    report=inventory(config,workspace,roots,private_file)
    runtime=workspace.get('runtime',{})
    if config.get('allow_backend_models'):
        try:
            if 'pi' not in api.call('/model-runtime').get('harnesses',[]):
                report['harnesses']=[h for h in report['harnesses'] if h!='pi'];report['model_connections']=[n for n in report['model_connections'] if n!='backend']
        except (ValueError,OSError):
            report['harnesses']=[h for h in report['harnesses'] if h!='pi'];report['model_connections']=[n for n in report['model_connections'] if n!='backend']
    if runtime.get('revision',0)>0 and report['remote_enabled'] and not report.get('error'):
        try:apply_settings(config,workspace,runtime['config'],report,roots,private_file);report['applied_revision']=runtime['revision']
        except RuntimeSettingsError as e:report['error']=str(e)
    api.call('/workspaces/'+workspace['id']+'/runtime-report',report)
    return report

def for_job(config,job,roots,private_file):
    workspace=job['snapshot']['workspace'];runtime=workspace.get('runtime',{})
    if not runtime.get('revision') or job['step']['kind']=='prepare':return config
    report=inventory(config,workspace,roots,private_file)
    return apply_settings(config,workspace,runtime['config'],report,roots,private_file)

UNPROFILED_NODE={'node_committed_package_lock_required','node_lockfile_v2_or_v3_required','node_workspace_requires_explicit_profile'}

def repository_facts(config,job,git):
    """Per repository: package identity, internal dependencies and changed paths
    since the run's base. Facts only; no file contents leave the runner."""
    import json as _json
    run_root=Path(config['state_dir'])/job['run_id'];facts=[]
    for repo in job['snapshot']['workspace']['repositories']:
        path=repo['path'];root=run_root/path;package={}
        manifest=root/'package.json'
        if manifest.is_file() and not manifest.is_symlink() and manifest.stat().st_size<=1024*1024:
            try:package=_json.loads(manifest.read_text())
            except ValueError:package={}
        changed=[]
        head=job['execution']['heads'].get(path)
        if head and head!=repo['head_sha']:
            try:changed=[l for l in git(['diff','--name-only',repo['head_sha'],head],root).splitlines() if l][:200]
            except (OSError,ValueError,subprocess.SubprocessError):changed=['<unknown>']
        deps=sorted({*package.get('dependencies',{}),*package.get('devDependencies',{}),*package.get('peerDependencies',{})}) if isinstance(package,dict) else []
        facts.append({'path':path,'package':package.get('name') if isinstance(package.get('name'),str) else None,
                      'description':package.get('description')[:300] if isinstance(package.get('description'),str) else None,
                      'dependencies':[d for d in deps if d.startswith('@')][:200],'changed_paths':changed})
    return facts

def validation_plan(config,job,profile,api,git):
    """Backend + Jev decide which checks this change needs. Older backends: all."""
    from control_plane import ControlPlaneError
    checks=[{'name':c['name'],'cwd':c['cwd']} for c in profile['checks']]
    try:
        plan=api.call('/runs/'+job['run_id']+'/steps/'+job['step']['id']+'/validation-plan',{'lease_token':job['lease_token'],'checks':checks,'repositories':repository_facts(config,job,git)})
    except ControlPlaneError as error:
        if error.status==404:return None
        raise
    decisions=plan.get('decisions') if isinstance(plan,dict) else None
    return decisions if isinstance(decisions,list) else None

def skip_evidence(api,job,name,payload):
    """A not_applicable evidence backed by a stored artifact explaining why."""
    import base64,json as _json
    data=_json.dumps(payload,ensure_ascii=False).encode()
    stored=api.call('/runs/'+job['run_id']+'/steps/'+job['step']['id']+'/artifacts',{'lease_token':job['lease_token'],'name':name,'mime':'application/json','content_base64':base64.b64encode(data).decode()})
    return stored['digest']

def suite(config,job,profile,roots,api,git,private_file):
    from execution import isolated_adapter, node_bootstrap_image
    from delivery import validate_services
    evidence=[];start=time.monotonic();success=True
    required=set(job['step'].get('options',{}).get('required_checks',[]))
    if {'tests','build'}.issubset(required):
        changed=[r['path'] for r in job['snapshot']['workspace']['repositories'] if job['execution']['heads'][r['path']]!=r['head_sha']]
        if any(not any(c['name']==role and c['cwd']==repo for c in profile['checks']) for repo in changed for role in ('tests','build')):
            raise RuntimeSettingsError('Tests and build must cover every changed repository')
    # Coverage is checked statically first; backend floors never skip a changed
    # repository, so the plan below can only narrow unchanged ones.
    decisions=validation_plan(config,job,profile,api,git)
    skipped=set()
    if decisions is not None:
        planned={(d['check']['name'],d['check']['cwd']):d for d in decisions if isinstance(d,dict) and isinstance(d.get('check'),dict)}
        skipped={key for key,d in planned.items() if d.get('run') is False}
        if skipped:
            digest=skip_evidence(api,job,'validation-plan.json',{'decisions':decisions})
            for name,cwd in sorted(skipped):
                evidence.append({'check':name,'status':'not_applicable','commits':job['execution']['heads'],'artifact_digest':digest})
    for index,check in enumerate(profile['checks']):
        if time.monotonic()-start>=job['step']['timeout_seconds']:raise RuntimeSettingsError('Suite time budget exceeded')
        if (check['name'],check['cwd']) in skipped:continue
        role=check['name'];unique=role+'-'+str(index);child=copy.deepcopy(job)
        child['step'].setdefault('options',{})['required_checks']=[unique]
        child['step']['timeout_seconds']=max(1,int(job['step']['timeout_seconds']-(time.monotonic()-start)))
        adapter={'kind':'checks','image':check['image'],'environment_refs':check.get('environment_refs',[]),'checks':[{'name':unique,'cwd':check['cwd'],'command':check['command']}]}
        if node_bootstrap_image(config, check['image']):adapter['node_bootstrap']=True
        if role in ('functional','design','security'):adapter['network']='services';validate_services(config,job)
        class NamedArtifacts:
            def call(self,path,data=None):
                if path.endswith('/artifacts'):data=dict(data,name=unique+'-'+data['name'])
                return api.call(path,data)
        try:
            passed,items,_=isolated_adapter(config,child,adapter,roots,NamedArtifacts(),git,private_file)
        except ValueError as error:
            # A repository Node cannot install deterministically (no committed
            # lockfile, v1 lockfile, workspaces) is not verifiable here: record
            # it as not applicable with its reason instead of aborting the suite.
            if not (error.args and error.args[0] in UNPROFILED_NODE):raise
            digest=skip_evidence(api,job,unique+'-not-applicable.json',{'check':unique,'cwd':check['cwd'],'code':error.args[0],'reason':str(error)})
            evidence.append({'check':role,'status':'not_applicable','commits':job['execution']['heads'],'artifact_digest':digest})
            continue
        for item in items:item['check']=role
        evidence.extend(items);success=success and passed
        if adapter.get('network')=='services':validate_services(config,job)
    required=set(job['step'].get('options',{}).get('required_checks',[]))
    if not required.issubset({e.get('check') for e in evidence}):success=False
    return success,evidence,{'input_revision':job['execution']['revision'],'heads':job['execution']['heads'],'output':{'checks':evidence},'cost_usd':0}
