"""Build reviewed command recipes from committed manifests; never execute them here."""
import copy,json,sys
from pathlib import Path
BASE=Path(sys.argv[1]) if len(sys.argv)>1 else Path.cwd()

def strict(command, kind, *, browser=False):
    # The command still runs the repository's real runner. Reports, not an exit
    # code alone, must show executed tests with no skips, failures or TODOs.
    preflight=""
    if browser:
        preflight="if(!process.env.E2E_EMAIL||!process.env.E2E_PASSWORD)throw Error('Dedicated test-only E2E_EMAIL/E2E_PASSWORD required; credential fallbacks are forbidden');"
    script="const fs=require('node:fs'),cp=require('node:child_process');"+preflight
    script+="const r=cp.spawnSync("+json.dumps(command[0])+","+json.dumps(command[1:]) +",{encoding:'utf8',maxBuffer:16777216,env:{...process.env,CI:'true'}});process.stdout.write(r.stdout||'');process.stderr.write(r.stderr||'');if(r.error)throw r.error;if(r.status!==0)process.exit(r.status||1);"
    if kind=='tap':
        script+="const n={};for(const m of (r.stdout||'').matchAll(/^# (tests|pass|fail|cancelled|skipped|todo) (\\d+)/gm))n[m[1]]=Number(m[2]);if(!n.tests||n.pass!==n.tests||['fail','cancelled','skipped','todo'].some(k=>n[k]>0))throw Error('TAP requires executed tests, all passed, none skipped or TODO');"
    else:
        script+="const j=JSON.parse(fs.readFileSync('/output/nm-test-results.json','utf8'));"
        if kind=='playwright':
            script+="const cases=[];function walk(s){for(const x of s||[]){for(const p of x.specs||[])cases.push(...p.tests||[]);walk(x.suites)}}walk(j.suites);if(!j.stats||!j.stats.expected||j.stats.unexpected||j.stats.skipped||j.stats.flaky||j.errors?.length||!cases.length||cases.some(t=>t.expectedStatus!=='passed'||t.results.some(r=>r.status!=='passed'))||JSON.stringify(j).includes('a11y-finding'))throw Error('Playwright requires executed tests, no skips/expected failures/flakiness/errors or accessibility findings');"
        else:
            script+="if(!j.success||!j.numTotalTests||j.numPassedTests!==j.numTotalTests||j.numFailedTests||j.numPendingTests||j.numTodoTests||j.numPendingTestSuites)throw Error('Tests require executed assertions, all passed, none skipped or TODO');"
    if kind!='tap':script+="console.log(JSON.stringify({runner:"+json.dumps(kind)+",tests:j.numTotalTests||j.stats.expected,skipped:0,report_sha256:require('node:crypto').createHash('sha256').update(fs.readFileSync('/output/nm-test-results.json')).digest('hex')}));"
    assert len(script)<4096
    return ['node','-e',script]

INTEGRATION_SPECS=r'\.integration\.(spec|test)\.[cm]?[jt]sx?$'

def node_tests(package, script='test', paths=None):
    body=package['scripts'][script]
    args=['npm','test'] if script=='test' else ['npm','run',script]
    if 'jest' in body:
        extra=['--ci','--runInBand','--passWithNoTests=false','--json','--outputFile=/output/nm-test-results.json']
        if paths:extra+=['--runTestsByPath',*paths]
        elif script=='test':
            # Integration specs skip themselves without their services (e.g. no
            # DATABASE_URL), which the strict report rejects as a skip. They belong
            # to an environment check, so the unit check leaves them out; any other
            # skip still fails. The CLI flag replaces the repo's configured
            # patterns, so those are kept.
            jest=package.get('jest') if isinstance(package.get('jest'),dict) else {}
            for pattern in [*(jest.get('testPathIgnorePatterns') or ['/node_modules/']),INTEGRATION_SPECS]:
                extra.append('--testPathIgnorePatterns='+pattern)
        return strict(args+['--',*extra],'jest')
    if 'vitest' in body:
        extra=['--passWithNoTests=false','--reporter=json','--outputFile=/output/nm-test-results.json']
        if paths:extra+=paths
        return strict(args+['--',*extra],'vitest')
    if 'tsx --test' in body:
        return strict(args,'tap')
    raise ValueError('No reviewed test runner: '+body)

def playwright(files, image, name, *, grep=None):
    args=['node','node_modules/@playwright/test/cli.js','test',*files,'--project=chromium','--workers=1','--retries=0','--forbid-only','--reporter=json','--output=/output/playwright']
    if grep:args+=['--grep',grep]
    # Reporter file is passed in the environment inside this one sandbox.
    cmd=strict(args,'playwright',browser=True)
    cmd[2]="process.env.PLAYWRIGHT_JSON_OUTPUT_FILE='/output/nm-test-results.json';"+cmd[2]
    return {'name':name,'cwd':'kasymir-app-ui','image':image,'command':cmd,'environment_refs':[]}

def plan(workspace,audit,details,images):
    config=copy.deepcopy(workspace['runtime']['config']);checks=[];notes=[]
    by_details={r['repository']:r for r in details}
    authorized={r['path']:r['head_sha'] for r in workspace['repositories']}
    def add(name,repo,command,image=None):checks.append({'name':name,'cwd':repo,'image':image or images['node'],'command':command,'environment_refs':[]})
    for repo in audit:
        name=repo['repository'];assert authorized.get(name)==repo['head'], 'Manifest does not match confirmed inventory'
        packages=repo['packages'];note={'repository':name,'head':repo['head'],'blockers':[],'checks':[]}
        if packages:
            assert len(packages)==1 and packages[0]['path']=='package.json'
            p=packages[0]
            add('build',name,['npm','run','build'])
            if 'test' in p['scripts']:
                add('tests',name,node_tests(p))
                if not repo['test_files']:note['blockers'].append('Declared test script has no committed tests; strict check must fail until real tests exist')
            else:note['blockers'].append('No test script or committed test suite; no dummy test was added')
            existing_e2e=[f for f in by_details[name]['test_paths'] if f.endswith(('.ts','.js','.tsx')) and ('.e2e-spec.' in f or '/e2e/' in f or f.startswith(('e2e/','test-e2e/')))]
            if 'test:e2e' in p['scripts'] and 'jest' in p['scripts']['test:e2e'] and existing_e2e:
                add('functional',name,node_tests(p,'test:e2e'))
            if not any(f=='package-lock.json' for f in repo['metadata_files']):note['blockers'].append('Only pnpm lockfile: isolated dependency preparation does not yet support pnpm; do not regenerate a different lockfile')
            if '--passWithNoTests' in p['scripts'].get('test',''):note['blockers'].append('Existing empty-suite success flag is explicitly overridden; empty suites cannot pass')
        else:
            note['blockers'].append('No Node manifest: needs its actual language/infra SDK; no Node build or fake tests substituted')
        notes.append(note)
    add('typecheck','kasymir-app-ui',['node','node_modules/typescript/bin/tsc','--noEmit','--incremental','false'])
    graphql=next(r['packages'][0] for r in audit if r['repository']=='kasymir-graphql-service')
    add('tests','kasymir-graphql-service',strict(['npm','run','test:contract'],'tap'))
    security_files=[
      'src/modules/app/guards/require-auth.middleware.spec.ts',
      'src/modules/app/guards/require-cv-bearer.middleware.spec.ts',
      'src/modules/app/server/ws-ticket.spec.ts',
      'src/modules/app/utils/organization-scope.spec.ts',
      'src/modules/app/utils/organization-scope.adapters.spec.ts',
    ]
    assert all(f in by_details['kasymir-graphql-service']['test_paths'] for f in security_files)
    add('security','kasymir-graphql-service',strict(['node','node_modules/tsx/dist/cli.mjs','--test',*security_files],'tap'))
    app=next(r['packages'][0] for r in audit if r['repository']=='kasymir-app-ui')
    cookie='src/lib/api/Auth/CvSession/index.test.ts'
    assert cookie in by_details['kasymir-app-ui']['test_paths']
    add('security','kasymir-app-ui',node_tests(app,paths=[cookie]))
    # Existing real UI suites. Exclude the diagnostic containing fixed credentials
    # and response dumps; never use it as acceptance evidence.
    safe_ui=[p for p in by_details['kasymir-app-ui']['test_paths'] if p.startswith('e2e/specs/') and p.endswith('.spec.ts') and Path(p).name not in ('login-session.spec.ts','login.spec.ts')]
    checks.append(playwright(safe_ui,images['playwright_1_61_1'],'functional'))
    visual=[f'e2e/specs/{n}.spec.ts' for n in ('customers','notifications','product-create','products','warehouse')]
    assert all(p in safe_ui for p in visual)
    checks.append(playwright(visual,images['playwright_1_61_1'],'design',grep='no horizontal overflow on mobile|page loads and renders content'))
    for repo,paths in {
        'kasymir-cameras-streaming-service':['infra/tests/verify-infra-safety.sh','infra/tests/verify-startup-fail-closed.sh'],
        'kasymir-infraestructure-service':['kubernetes/services/vend-payment-service/configuration.test.sh'],
    }.items():
        for path in paths:add('tests',repo,['bash',path],images['agent'])
    # Retain any independently added user check unless it is one of the three
    # audited entries being corrected/replaced in revision 13.
    known={('kasymir-app-ui','build'),('kasymir-app-ui','typecheck'),('kasymir-graphql-service','build')}
    for old in config['checks']:
        key=(old['cwd'],old['name'])
        if key not in known and old not in checks:checks.append(old)
    assert len(checks)<=256
    assert len({(c['name'],c['cwd'],json.dumps(c['command'])) for c in checks})==len(checks)
    assert all(c['cwd'] in authorized for c in checks)
    config['checks']=checks
    for note in notes:note['checks']=[{'name':c['name'],'command':c['command']} for c in checks if c['cwd']==note['repository']]
    assert config['agents']==workspace['runtime']['config']['agents']
    assert config['services']==workspace['runtime']['config']['services']
    return {'expected_revision':workspace['runtime']['revision'],'config':config,'repositories':notes}

if __name__=='__main__':
    w=json.loads((BASE/'kasymir-workspace-before-checks.json').read_text())
    audit=json.loads((BASE/'kasymir-checks-audit.json').read_text())
    details=json.loads((BASE/'kasymir-check-configs.json').read_text())
    images=json.loads((BASE/'kasymir-check-images.json').read_text())
    result=plan(w,audit,details,images)
    (BASE/'kasymir-checks-plan.json').write_text(json.dumps(result,ensure_ascii=False,indent=2))
    counts={k:sum(c['name']==k for c in result['config']['checks']) for k in ('tests','build','typecheck','functional','design','security')}
    assert counts['build']==44 and counts['tests']>=45
    print(json.dumps({'revision':result['expected_revision'],'checks':counts,'total':len(result['config']['checks']),'repositories':len(result['repositories'])}))
