use nexusmind::{
    db::{connection, migrations, queries, workflows as db},
    factory::workflows::{Definition, Evidence, EvidenceStatus, Repository, Step, StepKind},
};
use serde_json::json;
use std::collections::BTreeMap;

fn setup() -> (rusqlite::Connection, String, String, String, String) {
    seed(connection::connect(":memory:").unwrap())
}
fn seed(c: rusqlite::Connection) -> (rusqlite::Connection, String, String, String, String) {
    migrations::run(&c).unwrap();
    let (org, user, _) =
        queries::bootstrap(&c, "Test", "test", "test@example.com", "Owner").unwrap();
    let runner = db::register_runner(&c, &org.id, "Local").unwrap();
    let rid = runner["id"].as_str().unwrap().to_string();
    let w = db::create_workspace(&c, &org.id, &rid, "App", "/work/app").unwrap();
    let wid = w["id"].as_str().unwrap().to_string();
    let r = repo(false);
    db::report_inventory(&c, &org.id, &rid, &wid, &[r]).unwrap();
    db::confirm_inventory(&c, &org.id, &wid, 1, &[repo(true)]).unwrap();
    (c, org.id, user.id, rid, wid)
}
#[test]
fn schema_97_upgrades_existing_96_intakes_without_losing_rows_or_indexes() {
    let (c, org, user, _, w) = setup();
    let d = define(
        &c,
        &org,
        &user,
        &w,
        vec![step("prepare", &[], StepKind::Prepare)],
    );
    let sql: String = c
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='factory_workflow_intakes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    c.execute_batch("DROP TABLE factory_workflow_intakes")
        .unwrap();
    c.execute_batch(&sql.replace("'github',", "")).unwrap();
    c.execute_batch("CREATE INDEX workflow_intake_due ON factory_workflow_intakes(org_id,enabled,next_poll);PRAGMA user_version=96").unwrap();
    c.execute("INSERT INTO factory_workflow_intakes(id,org_id,definition_id,kind,connector_id,config_json,enabled,next_poll,last_error,created_by) VALUES ('legacy',?1,?2,'linear','private-reference','{\"team_id\":\"TEAM\"}',0,123,'held',?3)",rusqlite::params![org,d,user]).unwrap();
    migrations::run(&c).unwrap();
    let row:(String,i64,String)=c.query_row("SELECT config_json,next_poll,last_error FROM factory_workflow_intakes WHERE id='legacy'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(row, ("{\"team_id\":\"TEAM\"}".into(), 123, "held".into()));
    c.execute_batch("INSERT INTO factory_workflow_intakes SELECT 'github-source',org_id,definition_id,'github',connector_id,config_json,enabled,next_poll,last_error,created_by FROM factory_workflow_intakes WHERE id='legacy'").unwrap();
    migrations::run(&c).unwrap();
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0))
            .unwrap(),
        migrations::LATEST_USER_VERSION
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM factory_workflow_intakes", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        2
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='workflow_intake_due'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert!(c
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query([])
        .unwrap()
        .next()
        .unwrap()
        .is_none());
}
#[test]
fn shared_factory_pause_fences_active_workflows_and_preserves_queued_work() {
    let (c, org, user, runner, w) = setup();
    let d = define(
        &c,
        &org,
        &user,
        &w,
        vec![step("prepare", &[], StepKind::Prepare)],
    );
    let active = run(&c, &org, &d);
    let capabilities = vec!["adapter:test".into()];
    let job = db::claim(&c, &org, &runner, &capabilities, 100)
        .unwrap()
        .unwrap();
    let queued = db::start_pinned_run(&c, &org, &d, "queued:2", &json!({"title":"Queued"})).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    queries::patch_autonomous_agent_org_settings(&c, &org, Some(false), None).unwrap();
    assert_eq!(
        db::run_detail(&c, &org, &active).unwrap()["status"],
        "cancelled"
    );
    assert_eq!(
        db::run_detail(&c, &org, &queued).unwrap()["status"],
        "queued"
    );
    assert!(db::claim(&c, &org, &runner, &capabilities, 101)
        .unwrap()
        .is_none());
    assert!(db::start_pinned_run(&c, &org, &d, "paused:3", &json!({})).is_err());
    assert!(nexusmind::db::workflow_execution::lease(
        &c,
        &org,
        &runner,
        &active,
        "prepare",
        job["lease_token"].as_str().unwrap(),
        101
    )
    .is_err());
    queries::patch_autonomous_agent_org_settings(&c, &org, Some(true), None).unwrap();
    assert!(db::claim(&c, &org, &runner, &capabilities, 102)
        .unwrap()
        .is_some());
}
fn repo(writable: bool) -> Repository {
    Repository {
        path: "api".into(),
        remote: "git@github.com:acme/api.git".into(),
        head_sha: "a".repeat(40),
        branch: None,
        writable,
    }
}
fn step(id: &str, deps: &[&str], kind: StepKind) -> Step {
    Step {
        id: id.into(),
        name: id.into(),
        kind,
        depends_on: deps.iter().map(|d| d.to_string()).collect(),
        adapter: "test".into(),
        capabilities: vec![],
        write_repositories: vec![],
        max_attempts: 2,
        timeout_seconds: 30,
        options: serde_json::Value::Null,
    }
}
fn define(c: &rusqlite::Connection, org: &str, user: &str, w: &str, steps: Vec<Step>) -> String {
    db::create_definition(
        c,
        org,
        user,
        &Definition {
            name: "Fix".into(),
            workspace_id: w.into(),
            steps,
            repair_step: None,
            max_repairs: 2,
            max_cost_usd: 20.0,
        },
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn run(c: &rusqlite::Connection, org: &str, d: &str) -> String {
    db::start_pinned_run(c, org, d, "linear:issue-1", &json!({"title":"Fix"})).unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn claim(c: &rusqlite::Connection, org: &str, runner: &str, now: i64) -> serde_json::Value {
    db::claim(c, org, runner, &["adapter:test".into()], now)
        .unwrap()
        .unwrap()
}
#[test]
fn ordinary_new_runs_never_silently_use_a_legacy_inventory() {
    let (c, org, user, _, workspace) = setup();
    let mut prepare = step("prepare", &[], StepKind::Prepare);
    prepare.adapter = "prepare-workspace".into();
    let definition = define(&c, &org, &user, &workspace, vec![prepare]);
    let task = json!({"title":"Refresh", "pinned_sources":true});
    assert_eq!(db::start_run(&c, &org, &definition, "fresh:missing-branch", &task).unwrap_err().to_string(), "source_branch_not_confirmed");
    assert!(db::list_runs(&c, &org).unwrap().is_empty());
    let offline = db::start_pinned_run(&c, &org, &definition, "offline:explicit", &task).unwrap();
    assert_eq!(offline["snapshot"]["source_policy"], "pinned");
    assert!(offline["snapshot"]["sources_pending"].is_null());
}

#[test]
fn fresh_sources_seal_only_during_initial_fenced_preparation() {
    use nexusmind::{db::workflow_execution as execution, factory::workflows::Outcome};
    let (c, org, user, runner, workspace) = setup();
    let mut discovered = repo(false);
    discovered.remote = "https://github.com/acme/api.git".into();
    discovered.branch = Some("release/accounting".into());
    db::report_inventory(&c, &org, &runner, &workspace, &[discovered.clone()]).unwrap();
    discovered.writable = true;
    db::confirm_inventory(&c, &org, &workspace, 2, &[discovered]).unwrap();
    let mut prepare = step("prepare", &[], StepKind::Prepare);
    prepare.adapter = "prepare-workspace".into();
    // Even an independent root check cannot see the old inventory commits.
    let definition = define(&c, &org, &user, &workspace, vec![step("check", &[], StepKind::Check), prepare]);
    let run = db::start_run(&c, &org, &definition, "fresh:1", &json!({"title":"Refresh"})).unwrap()["id"].as_str().unwrap().to_string();
    assert_eq!(db::run_detail(&c, &org, &run).unwrap()["snapshot"]["sources_pending"], true);
    assert!(execution::approve(&c, &org, &user, &run, "publish", 0).is_err());
    execution::approve(&c, &org, &user, &run, "fix", 0).unwrap();
    assert!(db::claim(&c, &org, &runner, &["adapter:test".into(), "adapter:prepare-workspace".into()], 100).unwrap().is_none());
    let caps = vec!["adapter:prepare-workspace".into(), "workspace-sources:v1".into(), "workspace-git-read:v1".into()];
    let job = db::claim(&c, &org, &runner, &caps, 101).unwrap().unwrap();
    assert_eq!(job["step"]["id"], "prepare");
    assert!(db::claim(&c, &org, &runner, &["adapter:test".into()], 102).unwrap().is_none());
    let fence = job["lease_token"].as_str().unwrap();
    let artifact = execution::put_artifact(&c, &org, &runner, &run, "prepare", fence, "checkout-manifest.json", "application/json", b"{}", 103).unwrap();
    let commits: BTreeMap<String, String> = [("api".into(), "b".repeat(40))].into();
    let evidence = vec![Evidence { check: "committed-checkouts-prepared".into(), status: EvidenceStatus::Pass,
        commits: commits.clone(), artifact_digest: artifact["digest"].as_str().unwrap().into() }];
    let mut outcome = Outcome { input_revision: 0, heads: commits.clone(), output: json!({"sources_refreshed":true}), ..Default::default() };
    assert!(db::complete_with_outcome(&c, &org, &runner, &run, "prepare", "wrong-fence", true, &evidence, Some(&outcome), 104).is_err());
    outcome.output = json!({});
    assert!(db::complete_with_outcome(&c, &org, &runner, &run, "prepare", fence, true, &evidence, Some(&outcome), 104).is_err());
    assert_eq!(db::run_detail(&c, &org, &run).unwrap()["snapshot"]["sources_pending"], true);
    outcome.output = json!({"sources_refreshed":true});
    db::complete_with_outcome(&c, &org, &runner, &run, "prepare", fence, true, &evidence, Some(&outcome), 105).unwrap();
    let pinned = db::run_detail(&c, &org, &run).unwrap();
    assert_eq!(pinned["snapshot"]["sources_pending"], false);
    assert_eq!(pinned["snapshot"]["workspace"]["repositories"][0]["head_sha"], "b".repeat(40));
    assert_eq!(pinned["execution"]["heads"]["api"], "b".repeat(40));
    assert_eq!(pinned["execution"]["revision"], 0);
    assert!(execution::approved(&c, &run, "fix", 0).unwrap());
    assert_eq!(db::workspace_by_id(&c, &org, &workspace).unwrap()["repositories"][0]["head_sha"], "a".repeat(40));
    c.execute("UPDATE factory_workflow_steps SET status='blocked' WHERE run_id=?1 AND step_id='prepare'", [&run]).unwrap();
    c.execute("UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1", [&run]).unwrap();
    db::control_run(&c, &org, &run, "retry", Some("prepare")).unwrap();
    let retry = db::claim(&c, &org, &runner, &caps, 106).unwrap().unwrap();
    outcome.heads.insert("api".into(), "c".repeat(40));
    assert!(db::complete_with_outcome(&c, &org, &runner, &run, "prepare", retry["lease_token"].as_str().unwrap(), true, &[], Some(&outcome), 107).is_err());
    assert_eq!(db::run_detail(&c, &org, &run).unwrap()["snapshot"], pinned["snapshot"]);
}

#[test]
fn inventory_requires_confirmation_and_runner_scope() {
    let (c, o, _, r, w) = setup();
    assert!(db::workspace_by_id(&c, "other", &w).is_err());
    assert!(db::report_inventory(&c, &o, "other-runner", &w, &[repo(false)]).is_err());
    assert!(db::report_inventory(&c, &o, &r, &w, &[repo(true)]).is_err());
    assert!(db::confirm_inventory(&c, &o, &w, 0, &[repo(true)]).is_err());
    let mut unconfirmed_branch = repo(true);
    unconfirmed_branch.branch = Some("another-branch".into());
    assert!(db::confirm_inventory(&c, &o, &w, 1, &[unconfirmed_branch]).is_err());
    db::report_inventory(&c, &o, &r, &w, &[repo(false)]).unwrap();
    assert_eq!(
        db::workspace_by_id(&c, &o, &w).unwrap()["confirmed_revision"],
        1
    );
    let mut changed = repo(false);
    changed.head_sha = "b".repeat(40);
    db::report_inventory(&c, &o, &r, &w, &[changed]).unwrap();
    assert_eq!(
        db::workspace_by_id(&c, &o, &w).unwrap()["inventory_revision"],
        2
    );
}
#[test]
fn runner_tokens_are_hashed_and_revocable() {
    let (c, o, _, _, _) = setup();
    let registered = db::register_runner(&c, &o, "Second").unwrap();
    let token = registered["token"].as_str().unwrap();
    let rid = registered["id"].as_str().unwrap();
    assert_eq!(
        db::authenticate_runner(&c, token).unwrap(),
        (rid.into(), o.clone())
    );
    let stored: String = c
        .query_row(
            "SELECT token_hash FROM factory_runners WHERE id=?1",
            [rid],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(stored, token);
    assert!(!db::list_runners(&c, &o)
        .unwrap()
        .iter()
        .any(|r| r.to_string().contains(token)));
    db::revoke_runner(&c, &o, rid).unwrap();
    assert!(db::authenticate_runner(&c, token).is_err());
}
#[test]
fn dependencies_deduplication_and_fences() {
    let (c, o, u, r, w) = setup();
    let d = define(
        &c,
        &o,
        &u,
        &w,
        vec![
            step("prepare", &[], StepKind::Prepare),
            step("qa", &["prepare"], StepKind::Check),
        ],
    );
    let run = run(&c, &o, &d);
    assert_eq!(
        run,
        db::start_pinned_run(&c, &o, &d, "linear:issue-1", &json!({})).unwrap()["id"]
    );
    assert!(db::claim(&c, &o, &r, &[], 100).unwrap().is_none());
    let j = claim(&c, &o, &r, 100);
    assert_eq!(j["step"]["id"], "prepare");
    assert!(db::claim(&c, &o, &r, &["adapter:test".into()], 101)
        .unwrap()
        .is_none());
    assert!(db::complete(&c, &o, &r, &run, "prepare", "wrong", true, &[], 101).is_err());
    db::complete(
        &c,
        &o,
        &r,
        &run,
        "prepare",
        j["lease_token"].as_str().unwrap(),
        true,
        &[],
        101,
    )
    .unwrap();
    assert!(db::complete(
        &c,
        &o,
        &r,
        &run,
        "prepare",
        j["lease_token"].as_str().unwrap(),
        true,
        &[],
        101
    )
    .is_err());
    let j = claim(&c, &o, &r, 102);
    assert_eq!(j["step"]["id"], "qa");
    db::complete(
        &c,
        &o,
        &r,
        &run,
        "qa",
        j["lease_token"].as_str().unwrap(),
        true,
        &[],
        103,
    )
    .unwrap();
    assert_eq!(db::run_detail(&c, &o, &run).unwrap()["status"], "blocked");
    db::control_run(&c, &o, &run, "retry", Some("qa")).unwrap();
    let j = claim(&c, &o, &r, 104);
    let evidence = Evidence {
        check: "functional".into(),
        status: EvidenceStatus::Pass,
        commits: BTreeMap::from([("api".into(), "a".repeat(40))]),
        artifact_digest: "b".repeat(64),
    };
    let mut stale = evidence.clone();
    stale.commits.insert("api".into(), "c".repeat(40));
    assert!(db::complete(
        &c,
        &o,
        &r,
        &run,
        "qa",
        j["lease_token"].as_str().unwrap(),
        true,
        &[stale],
        105
    )
    .is_err());
    db::complete(
        &c,
        &o,
        &r,
        &run,
        "qa",
        j["lease_token"].as_str().unwrap(),
        true,
        &[evidence],
        105,
    )
    .unwrap();
    assert_eq!(db::run_detail(&c, &o, &run).unwrap()["status"], "succeeded");
    assert!(db::run_detail(&c, "another-org", &run).is_err());
}
#[test]
fn expired_lease_requires_reconciliation_and_cancellation_fences_old_worker() {
    let (c, o, u, r, w) = setup();
    let d = define(&c, &o, &u, &w, vec![step("a", &[], StepKind::Prepare)]);
    let run = run(&c, &o, &d);
    let j = claim(&c, &o, &r, 100);
    assert!(db::complete(
        &c,
        &o,
        &r,
        &run,
        "a",
        j["lease_token"].as_str().unwrap(),
        true,
        &[],
        200
    )
    .is_err());
    db::expire_leases(&c, &o, 200).unwrap();
    db::expire_leases(&c, &o, 200).unwrap();
    assert_eq!(
        db::run_detail(&c, &o, &run).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "step.lease_expired")
            .count(),
        1
    );
    assert!(db::claim(&c, &o, &r, &["adapter:test".into()], 200)
        .unwrap()
        .is_none());
    assert_eq!(db::run_detail(&c, &o, &run).unwrap()["status"], "blocked");
    db::control_run(&c, &o, &run, "retry", Some("a")).unwrap();
    let j2 = claim(&c, &o, &r, 201);
    assert_ne!(j["lease_token"], j2["lease_token"]);
    db::control_run(&c, &o, &run, "cancel", None).unwrap();
    assert!(db::complete(
        &c,
        &o,
        &r,
        &run,
        "a",
        j2["lease_token"].as_str().unwrap(),
        true,
        &[],
        202
    )
    .is_err());
}
#[test]
fn reopen_preserves_leases_and_concurrent_claims_are_exclusive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workflow.db");
    let path = path.to_str().unwrap();
    let (c, o, u, r, w) = seed(connection::connect(path).unwrap());
    let d = define(&c, &o, &u, &w, vec![step("a", &[], StepKind::Prepare)]);
    let run = run(&c, &o, &d);
    drop(c);
    let mut threads = Vec::new();
    for _ in 0..2 {
        let path = path.to_string();
        let o = o.clone();
        let r = r.clone();
        threads.push(std::thread::spawn(move || {
            let c = connection::connect(&path).unwrap();
            db::claim(&c, &o, &r, &["adapter:test".into()], 100).unwrap()
        }));
    }
    let jobs: Vec<_> = threads
        .into_iter()
        .filter_map(|t| t.join().unwrap())
        .collect();
    assert_eq!(jobs.len(), 1);
    let c = connection::connect(path).unwrap();
    assert!(db::claim(&c, &o, &r, &["adapter:test".into()], 101)
        .unwrap()
        .is_none());
    db::complete(
        &c,
        &o,
        &r,
        &run,
        "a",
        jobs[0]["lease_token"].as_str().unwrap(),
        true,
        &[],
        102,
    )
    .unwrap();
    drop(c);
    let c = connection::connect(path).unwrap();
    assert_eq!(db::run_detail(&c, &o, &run).unwrap()["status"], "succeeded");
}
#[test]
fn parallel_steps_can_finish_while_a_sibling_is_blocked() {
    let (c, o, u, r, w) = setup();
    let d = define(
        &c,
        &o,
        &u,
        &w,
        vec![
            step("a", &[], StepKind::Prepare),
            step("b", &[], StepKind::Prepare),
        ],
    );
    let run = run(&c, &o, &d);
    let a = claim(&c, &o, &r, 100);
    let b = claim(&c, &o, &r, 101);
    db::complete(
        &c,
        &o,
        &r,
        &run,
        "a",
        a["lease_token"].as_str().unwrap(),
        false,
        &[],
        102,
    )
    .unwrap();
    db::complete(
        &c,
        &o,
        &r,
        &run,
        "b",
        b["lease_token"].as_str().unwrap(),
        true,
        &[],
        103,
    )
    .unwrap();
    assert_eq!(db::run_detail(&c, &o, &run).unwrap()["status"], "blocked");
    db::control_run(&c, &o, &run, "retry", Some("a")).unwrap();
    let retry = claim(&c, &o, &r, 104);
    db::complete(
        &c,
        &o,
        &r,
        &run,
        "a",
        retry["lease_token"].as_str().unwrap(),
        true,
        &[],
        105,
    )
    .unwrap();
    assert_eq!(db::run_detail(&c, &o, &run).unwrap()["status"], "succeeded");
}
#[test]
fn moved_bases_restart_atomically_with_fresh_sources_bounded_budget_and_no_reused_evidence() {
    use nexusmind::{db::factory_queries, factory::contracts::ActionPolicy};
    fn stale(c: &rusqlite::Connection, run: &str, spent: f64) {
        // Simulate completed fenced work followed by the server's stale-base rejection.
        c.execute("UPDATE factory_workflow_runs SET status='blocked',cost_usd=?2,heads_json=?3,snapshot_json=json_set(snapshot_json,'$.sources_pending',json('false')) WHERE id=?1",rusqlite::params![run,spent,json!({"api":"b".repeat(40)}).to_string()]).unwrap();
        c.execute("UPDATE factory_workflow_steps SET status='succeeded',attempt=1,result_json=?2 WHERE run_id=?1",rusqlite::params![run,json!({"heads":{"api":"b".repeat(40)},"evidence":[{"check":"tests","status":"pass"}]}).to_string()]).unwrap();
        c.execute("UPDATE factory_workflow_steps SET status='blocked',result_json=?2 WHERE run_id=?1 AND step_id='publish'",rusqlite::params![run,json!({"output":{"error":"base_moved_reverify","exception_type":"ControlPlaneError","hold":true}}).to_string()]).unwrap();
    }
    let (c, org, user, runner, workspace) = setup();
    let mut r=repo(false);r.branch=Some("main".into());
    db::report_inventory(&c,&org,&runner,&workspace,&[r.clone()]).unwrap();
    let revision=db::workspace_by_id(&c,&org,&workspace).unwrap()["inventory_revision"].as_i64().unwrap();
    r.writable=true;
    db::confirm_inventory(&c,&org,&workspace,revision,&[r]).unwrap();
    let mut prepare=step("prepare",&[],StepKind::Prepare);prepare.adapter="prepare-workspace".into();
    let mut publish=step("publish",&["verify"],StepKind::Publish);publish.adapter="policy-publisher".into();
    let definition=define(&c,&org,&user,&workspace,vec![prepare,step("verify",&["prepare"],StepKind::Check),publish]);
    let root=run(&c,&org,&definition);stale(&c,&root,3.0);
    assert!(db::restart_after_base_move(&c,"other-org",&root,"publish").unwrap().is_none());
    assert!(db::restart_after_base_move(&c,&org,&root,"verify").unwrap().is_none());
    assert!(db::restart_after_base_move(&c,&org,&root,"publish").unwrap().is_none(),"recovery needs policy authorization");
    for action in ["recover","fix"] {
        let policy:ActionPolicy=serde_json::from_value(json!({"schema_version":1,"action":action,"mode":"always","scope":{},"allow":[],"stop":[],"version":1})).unwrap();
        factory_queries::upsert_factory_policy(&c,&org,&user,&policy).unwrap();
    }
    let previous_verify=db::run_detail(&c,&org,&root).unwrap()["steps"].as_array().unwrap().iter().find(|s|s["id"]=="verify").unwrap().clone();
    db::control_run(&c,&org,&root,"resume",None).unwrap();
    let refreshed=db::run_detail(&c,&org,&root).unwrap();
    let first=refreshed["events"].as_array().unwrap().iter().find(|e|e["kind"]=="run.base_refreshed").unwrap()["payload"]["replacement_run"].as_str().unwrap().to_string();
    assert!(db::restart_after_base_move(&c,&org,&root,"publish").unwrap().is_none(),"a handoff is idempotent");
    let retired=db::run_detail(&c,&org,&root).unwrap();
    assert_eq!(retired["status"],"cancelled");
    assert_eq!(retired["steps"].as_array().unwrap().iter().find(|s|s["id"]=="verify").unwrap(),&previous_verify);
    let next=db::run_detail(&c,&org,&first).unwrap();
    assert_eq!(next["snapshot"]["source_policy"],"fresh");
    assert_eq!(next["snapshot"]["sources_pending"],true);
    assert_eq!(next["snapshot"]["definition"]["max_cost_usd"],17.0);
    assert_eq!(next["execution"]["heads"],json!({"api":"a".repeat(40)}));
    assert!(next["steps"].as_array().unwrap().iter().all(|s|s["status"]=="pending"&&s["result"].is_null()));
    assert_eq!(next["task"],retired["task"]);
    assert_eq!(next["execution"]["repairs"],0);
    stale(&c,&first,5.0);
    let second=db::restart_after_base_move(&c,&org,&first,"publish").unwrap().unwrap();
    assert_eq!(db::run_detail(&c,&org,&second).unwrap()["snapshot"]["definition"]["max_cost_usd"],12.0);
    stale(&c,&second,1.0);
    assert!(db::restart_after_base_move(&c,&org,&second,"publish").unwrap().is_none(),"never refresh without a bound");
    for (key,spent,partial,attempt) in [("budget",20.0,false,1),("partial",0.0,true,1),("uncertain",0.0,false,2)] {
        let id=db::start_pinned_run(&c,&org,&definition,key,&json!({"title":"Fix"})).unwrap()["id"].as_str().unwrap().to_string();
        stale(&c,&id,spent);
        if partial {c.execute("INSERT INTO factory_workflow_publications(run_id,repository_path,head_sha,state) VALUES (?1,'api',?2,'awaiting_ci')",rusqlite::params![id,"b".repeat(40)]).unwrap();}
        c.execute("UPDATE factory_workflow_steps SET attempt=?2 WHERE run_id=?1 AND step_id='publish'",rusqlite::params![id,attempt]).unwrap();
        assert!(db::restart_after_base_move(&c,&org,&id,"publish").unwrap().is_none(),"{key}");
        assert_eq!(db::run_detail(&c,&org,&id).unwrap()["status"],"blocked");
    }
    let collision=db::start_pinned_run(&c,&org,&definition,"collision",&json!({"title":"Fix"})).unwrap()["id"].as_str().unwrap().to_string();
    stale(&c,&collision,3.0);
    let reserved=format!("base-refresh:{collision}:1");
    let unrelated=db::start_pinned_run(&c,&org,&definition,&reserved,&json!({"title":"Unrelated"})).unwrap();
    assert!(db::restart_after_base_move(&c,&org,&collision,"publish").unwrap_err().to_string().contains("base_refresh_key_conflict"));
    assert_eq!(db::run_detail(&c,&org,&collision).unwrap()["status"],"blocked");
    assert_eq!(db::run_detail(&c,&org,unrelated["id"].as_str().unwrap()).unwrap(),unrelated);
    let manual:ActionPolicy=serde_json::from_value(json!({"schema_version":1,"action":"fix","mode":"manual","scope":{},"allow":[],"stop":[],"version":2})).unwrap();
    factory_queries::upsert_factory_policy(&c,&org,&user,&manual).unwrap();
    let held=db::start_pinned_run(&c,&org,&definition,"manual-fix",&json!({"title":"Fix"})).unwrap()["id"].as_str().unwrap().to_string();
    stale(&c,&held,0.0);
    let pending=db::restart_after_base_move(&c,&org,&held,"publish").unwrap().unwrap();
    assert_eq!(db::run_detail(&c,&org,&pending).unwrap()["status"],"blocked","do not carry old fix approval into a new run");
    let invalid=db::start_pinned_run(&c,&org,&definition,"invalid-config",&json!({"title":"Fix"})).unwrap()["id"].as_str().unwrap().to_string();
    stale(&c,&invalid,0.0);
    let mut r=repo(false); // An operator removed branch confirmation after the old run.
    db::report_inventory(&c,&org,&runner,&workspace,&[r.clone()]).unwrap();
    let revision=db::workspace_by_id(&c,&org,&workspace).unwrap()["inventory_revision"].as_i64().unwrap();
    r.writable=true;db::confirm_inventory(&c,&org,&workspace,revision,&[r]).unwrap();
    let before:i64=c.query_row("SELECT count(*) FROM factory_workflow_runs",[],|r|r.get(0)).unwrap();
    assert!(db::restart_after_base_move(&c,&org,&invalid,"publish").unwrap_err().to_string().contains("source_branch_not_confirmed"));
    let after:i64=c.query_row("SELECT count(*) FROM factory_workflow_runs",[],|r|r.get(0)).unwrap();
    assert_eq!(before,after,"a failed handoff cannot leave an orphan replacement");
    assert_eq!(db::run_detail(&c,&org,&invalid).unwrap()["status"],"blocked");
}

#[test]
fn heartbeat_infrastructure_hold_never_asks_for_code_repair() {
    use nexusmind::factory::workflows::Outcome;
    let (c, org, user, runner, workspace) = setup();
    let definition = define(&c, &org, &user, &workspace, vec![step("implement", &[], StepKind::Agent)]);
    let run = run(&c, &org, &definition);
    let job = db::claim(&c, &org, &runner, &["adapter:test".into()], 100).unwrap().unwrap();
    let outcome = Outcome {
        input_revision: 0,
        heads: BTreeMap::from([("api".into(), "a".repeat(40))]),
        output: json!({"error":"adapter_http_500","exception_type":"ControlPlaneError","hold":true,"request":"POST /runs/:run/steps/:step/lease"}),
        cost_usd: 0.0,
    };
    db::complete_with_outcome(&c, &org, &runner, &run, "implement", job["lease_token"].as_str().unwrap(), false, &[], Some(&outcome), 101).unwrap();
    assert!(db::recovery_context(&c, &org, &run, "implement").unwrap().is_none());
    let detail = db::run_detail(&c, &org, &run).unwrap();
    assert_eq!(detail["status"], "blocked");
    assert_eq!(detail["execution"]["repairs"], 0);
    assert!(detail["events"].as_array().unwrap().iter().any(|e| e["kind"] == "recovery.skipped" && e["payload"]["reason"] == "infrastructure" && e["payload"]["code"] == "adapter_http_500"));
}

#[test]
fn qa_findings_continue_with_failed_evidence_but_builds_and_infrastructure_still_block() {
    use nexusmind::{db::workflow_execution as execution, factory::workflows::Outcome};
    let cases = [
        ("isolated-functional-qa", "functional", Some(EvidenceStatus::Fail), json!({}), true),
        ("isolated-design-qa", "design", Some(EvidenceStatus::Fail), json!({}), true),
        ("isolated-security-qa", "security", Some(EvidenceStatus::Fail), json!({}), true),
        ("isolated-verification", "tests", Some(EvidenceStatus::Fail), json!({}), false),
        ("isolated-functional-qa", "functional", Some(EvidenceStatus::Blocked), json!({}), false),
        ("isolated-functional-qa", "functional", Some(EvidenceStatus::Fail), json!({"hold":true}), false),
        ("isolated-functional-qa", "functional", Some(EvidenceStatus::Fail), json!({"error":"node_dependency_install_failed"}), false),
        ("isolated-functional-qa", "tests", Some(EvidenceStatus::Fail), json!({}), false),
        ("isolated-functional-qa", "functional", None, json!({}), false),
    ];
    for (adapter, check, state, output, reports_only) in cases {
        let (c, org, user, runner, workspace) = setup();
        let mut qa = step("qa", &[], StepKind::Check);
        qa.adapter = adapter.into();
        qa.options = json!({"required_checks":[check]});
        let definition = define(&c, &org, &user, &workspace, vec![qa, step("gate", &["qa"], StepKind::Gate)]);
        let run = run(&c, &org, &definition);
        let capabilities = vec![format!("adapter:{adapter}"), "adapter:test".into()];
        let job = db::claim(&c, &org, &runner, &capabilities, 100).unwrap().unwrap();
        let fence = job["lease_token"].as_str().unwrap();
        let artifact = execution::put_artifact(&c, &org, &runner, &run, "qa", fence, "qa.json", "application/json", b"{}", 101).unwrap();
        let heads = BTreeMap::from([("api".into(), "a".repeat(40))]);
        let evidence: Vec<_> = state.into_iter().map(|status| Evidence {
            check: check.into(), status, commits: heads.clone(), artifact_digest: artifact["digest"].as_str().unwrap().into(),
        }).collect();
        let outcome = Outcome { input_revision: 0, heads, output, cost_usd: 0.0 };
        db::complete_with_outcome(&c, &org, &runner, &run, "qa", fence, false, &evidence, Some(&outcome), 102).unwrap();
        let detail = db::run_detail(&c, &org, &run).unwrap();
        let qa = detail["steps"].as_array().unwrap().iter().find(|s| s["id"] == "qa").unwrap();
        assert_eq!(qa["status"], if reports_only { "succeeded" } else { "blocked" }, "{adapter} {check}");
        assert_eq!(qa["result"]["qa_report_only"] == true, reports_only);
        assert_eq!(detail["execution"]["repairs"], 0);
        if reports_only {
            assert_eq!(qa["result"]["evidence"][0]["status"], "fail", "QA must not be relabeled as passed");
            let next = db::claim(&c, &org, &runner, &capabilities, 103).unwrap().unwrap();
            assert_eq!(next["step"]["id"], "gate");
            assert_eq!(next["dependency_results"]["qa"]["qa_report_only"], true);
        } else {
            assert_eq!(detail["status"], "blocked");
        }
    }
}

#[test]
fn changed_heads_are_fenced_and_review_failure_queues_a_fresh_bounded_repair() {
    use nexusmind::db::workflow_execution as execution;
    use nexusmind::factory::workflows::Outcome;
    let (c, o, u, r, w) = setup();
    let mut implementation = step("implement", &[], StepKind::Agent);
    implementation.write_repositories = vec!["api".into()];
    implementation.max_attempts = 3;
    let mut review = step("review", &["implement"], StepKind::Review);
    review.max_attempts = 3;
    let definition = Definition {
        name: "Delivery".into(),
        workspace_id: w,
        steps: vec![implementation, review],
        repair_step: Some("implement".into()),
        max_repairs: 1,
        max_cost_usd: 2.0,
    };
    let d = db::create_definition(&c, &o, &u, &definition).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let run = run(&c, &o, &d);
    execution::approve(&c, &o, &u, &run, "fix", 0).unwrap();
    let job = claim(&c, &o, &r, 100);
    let fence = job["lease_token"].as_str().unwrap();
    let artifact = execution::put_artifact(
        &c,
        &o,
        &r,
        &run,
        "implement",
        fence,
        "result.json",
        "application/json",
        b"{}",
        101,
    )
    .unwrap();
    let heads = BTreeMap::from([("api".into(), "b".repeat(40))]);
    let evidence = Evidence {
        check: "implementation".into(),
        status: EvidenceStatus::Pass,
        commits: heads.clone(),
        artifact_digest: artifact["digest"].as_str().unwrap().into(),
    };
    let outcome = Outcome {
        input_revision: 0,
        heads: heads.clone(),
        output: json!({"summary":"author rationale must not enter review"}),
        cost_usd: 0.1,
    };
    db::complete_with_outcome(
        &c,
        &o,
        &r,
        &run,
        "implement",
        fence,
        true,
        &[evidence],
        Some(&outcome),
        102,
    )
    .unwrap();
    let review = claim(&c, &o, &r, 103);
    assert_eq!(review["execution"]["revision"], 1);
    assert!(review["dependency_results"].get("implement").is_none());
    let fence = review["lease_token"].as_str().unwrap();
    let a = execution::put_artifact(
        &c,
        &o,
        &r,
        &run,
        "review",
        fence,
        "review.json",
        "application/json",
        b"{}",
        104,
    )
    .unwrap();
    let evidence = Evidence {
        check: "review".into(),
        status: EvidenceStatus::Fail,
        commits: heads.clone(),
        artifact_digest: a["digest"].as_str().unwrap().into(),
    };
    let outcome = Outcome {
        input_revision: 1,
        heads,
        output: json!({"verdict":"changes_requested","findings":["Missing authorization"]}),
        cost_usd: 0.2,
    };
    db::complete_with_outcome(
        &c,
        &o,
        &r,
        &run,
        "review",
        fence,
        false,
        &[evidence],
        Some(&outcome),
        105,
    )
    .unwrap();
    let detail = db::run_detail(&c, &o, &run).unwrap();
    assert_eq!(detail["status"], "queued");
    assert_eq!(detail["execution"]["repairs"], 1);
    let next = claim(&c, &o, &r, 106);
    assert_eq!(next["step"]["id"], "implement");
    assert_eq!(next["attempt"], 2);
    assert_eq!(
        next["execution"]["repair_feedback"]["output"]["verdict"],
        "changes_requested"
    );
    let stale = Outcome {
        input_revision: 0,
        ..Default::default()
    };
    assert!(db::complete_with_outcome(
        &c,
        &o,
        &r,
        &run,
        "implement",
        next["lease_token"].as_str().unwrap(),
        true,
        &[],
        Some(&stale),
        107
    )
    .is_err());
    execution::approve(&c, &o, &u, &run, "publish", 1).unwrap();
    assert!(!execution::approved(&c, &run, "publish", 2).unwrap());
}
#[test]
fn artifact_scope_rotation_and_backup_invalidate_execution_authority() {
    use nexusmind::db::workflow_execution as execution;
    let (mut c, o, u, r, w) = setup();
    let d = define(&c, &o, &u, &w, vec![step("a", &[], StepKind::Prepare)]);
    let run = run(&c, &o, &d);
    let job = claim(&c, &o, &r, 100);
    let fence = job["lease_token"].as_str().unwrap();
    let a = execution::put_artifact(
        &c,
        &o,
        &r,
        &run,
        "a",
        fence,
        "log.txt",
        "text/plain",
        b"hello",
        101,
    )
    .unwrap();
    let id = a["id"].as_str().unwrap();
    assert!(execution::get_artifact(&c, "other-org", id).is_err());
    assert_eq!(execution::get_artifact(&c, &o, id).unwrap().1, b"hello");
    let rotated = execution::rotate_runner(&c, &o, &r).unwrap();
    let token = rotated["token"].as_str().unwrap();
    assert!(db::authenticate_runner(&c, token).is_ok());
    assert!(execution::lease(&c, &o, &r, &run, "a", fence, 102).is_err());
    let dumps = nexusmind::backup::serializer::BACKUP_TABLES
        .iter()
        .map(|table| {
            let dump = nexusmind::backup::serializer::dump_table(&c, table).unwrap();
            (dump.table_name, dump.rows)
        })
        .collect::<Vec<_>>();
    nexusmind::backup::restore::restore_from_dump(&mut c, &dumps).unwrap();
    assert!(db::authenticate_runner(&c, token).is_err());
    assert_eq!(execution::get_artifact(&c, &o, id).unwrap().1, b"hello");
}
#[test]
fn publisher_never_bypasses_control_plane_policies() {
    let (c, o, u, r, w) = setup();
    let d = define(
        &c,
        &o,
        &u,
        &w,
        vec![step("publish", &[], StepKind::Publish)],
    );
    let run = run(&c, &o, &d);
    assert!(db::claim(&c, &o, &r, &["adapter:test".into()], 100)
        .unwrap()
        .is_none());
    assert_eq!(db::run_detail(&c, &o, &run).unwrap()["status"], "blocked");
}

#[test]
fn observable_history_is_attributed_paginated_budgeted_and_retained() {
    use nexusmind::db::workflow_logs as logs;
    let (c, org, user, runner, workspace) = setup();
    let d = define(&c, &org, &user, &workspace, vec![step("prepare", &[], StepKind::Prepare)]);
    let run = run(&c, &org, &d);
    let job = claim(&c, &org, &runner, 100);
    let fence = job["lease_token"].as_str().unwrap().to_string();
    let ingest = |body: serde_json::Value| -> anyhow::Result<serde_json::Value> {
        let mut body = body;
        body["lease_token"] = fence.clone().into();
        let e: logs::Ingest = serde_json::from_value(body).unwrap();
        logs::ingest(&c, &org, &runner, &run, "prepare", &e, Some("rn-0123456789abcdef"), 101)
    };
    // Identity comes from the lease; a producer cannot claim another attempt.
    assert!(serde_json::from_value::<logs::Ingest>(json!({"lease_token":"x","kind":"log.chunk","message":"m","attempt":9})).is_err());
    ingest(json!({"kind":"log.chunk","message":"line 0 Authorization: Bearer abcdefghijklmnop\n","stream":"combined","source":"agent","offset":0})).unwrap();
    assert_eq!(ingest(json!({"kind":"log.chunk","message":"line 0 again","stream":"combined","source":"agent","offset":0})).unwrap()["duplicate"], true);
    assert_eq!(ingest(json!({"kind":"log.chunk","message":"x","source":"elsewhere"})).unwrap_err().to_string(), "invalid_event");
    let page = logs::list(&c, &org, &run, &logs::Query { step: Some("prepare".into()), attempt: Some(1), ..Default::default() }).unwrap();
    let first = &page["events"].as_array().unwrap().iter().find(|e| e["kind"] == "log.chunk").unwrap()["payload"];
    assert_eq!(first["attempt"], 1);
    assert_eq!(first["input_revision"], 0);
    assert_eq!(first["request_id"], "rn-0123456789abcdef");
    assert!(!first["message"].as_str().unwrap().contains("abcdefghijklmnop"));
    // More than 1000 events stay reachable, oldest included, by cursor.
    for i in 1..1300 {
        ingest(json!({"kind":"log.chunk","message":format!("line {i}\n"),"offset":i * 10})).unwrap();
    }
    let mut before = None;
    let mut seen = Vec::new();
    loop {
        let page = logs::list(&c, &org, &run, &logs::Query { before, limit: Some(500), class: Some("logs".into()), ..Default::default() }).unwrap();
        let events = page["events"].as_array().unwrap().clone();
        before = events.first().and_then(|e| e["seq"].as_i64());
        seen.splice(0..0, events);
        if page["has_older"] != true { break }
    }
    assert_eq!(seen.len(), 1300);
    assert!(seen[0]["payload"]["message"].as_str().unwrap().starts_with("line 0"));
    let newer = logs::list(&c, &org, &run, &logs::Query { after: seen[1298]["seq"].as_i64(), ..Default::default() }).unwrap();
    assert_eq!(newer["events"].as_array().unwrap().len(), 1);
    // Output exhausting its budget never blocks control events, and is marked.
    for i in 1300..logs::LOG_LIMIT {
        c.execute("INSERT INTO factory_workflow_events(run_id,step_id,kind,payload_json) VALUES (?1,'prepare','log.chunk',?2)", rusqlite::params![run, json!({"message":i.to_string(),"attempt":1}).to_string()]).unwrap();
    }
    assert_eq!(ingest(json!({"kind":"log.chunk","message":"over"})).unwrap_err().to_string(), "event_quota");
    ingest(json!({"kind":"command.finished","message":"exit 0","data":{"exit_code":0,"gap_bytes":0,"dropped_events":2}})).unwrap();
    let coverage = logs::coverage(&c, &run, Some("prepare"), Some(1)).unwrap();
    assert_eq!(coverage["truncated"], json!(["logs"]));
    assert_eq!(coverage["dropped_events"], 2);
    assert_eq!(coverage["complete"], false);
    // The run detail stays light: output is served by the paginated history.
    let detail = db::run_detail(&c, &org, &run).unwrap();
    assert!(detail["events"].as_array().unwrap().iter().all(|e| e["kind"] != "log.chunk"));
    // Other organizations and other runners cannot read or attribute.
    assert_eq!(logs::list(&c, "another-organization", &run, &logs::Query::default()).unwrap_err().to_string(), "not_found");
    assert!(!logs::record_rejection(&c, &db::token_hash("nmr_wrong"), &run, "prepare", "/x", 409, "lease_conflict", None).unwrap());
    let hash: String = c.query_row("SELECT token_hash FROM factory_runners WHERE id=?1", [&runner], |r| r.get(0)).unwrap();
    assert!(logs::record_rejection(&c, &hash, &run, "prepare", "/v1/factory/runner/runs/:run/steps/:step/lease", 409, "lease_conflict", Some("rn-1")).unwrap());
    // Retention removes only bulky output of finished runs and says so.
    c.execute("UPDATE factory_workflow_runs SET status='succeeded' WHERE id=?1", [&run]).unwrap();
    c.execute("UPDATE factory_workflow_events SET created_at=datetime('now','-30 days') WHERE run_id=?1", [&run]).unwrap();
    assert_eq!(logs::prune(&c, 14).unwrap(), logs::LOG_LIMIT);
    let coverage = logs::coverage(&c, &run, None, None).unwrap();
    assert_eq!(coverage["log_events"], 0);
    assert_eq!(coverage["pruned_events"], logs::LOG_LIMIT);
    assert!(db::run_detail(&c, &org, &run).unwrap()["events"].as_array().unwrap().iter().any(|e| e["kind"] == "backend.rejected"));
}

#[test]
fn intake_starts_new_work_beside_blocked_runs_bounded_and_without_duplicates() {
    use nexusmind::automation::workflow_intake::{start_intake_runs, MAX_RUNS_PER_POLL};
    let (c, org, user, _, w) = setup();
    let d = define(
        &c,
        &org,
        &user,
        &w,
        vec![step("prepare", &[], StepKind::Prepare)],
    );
    let stuck = run(&c, &org, &d);
    c.execute(
        "UPDATE factory_workflow_runs SET status='blocked' WHERE id=?1",
        [&stuck],
    )
    .unwrap();
    let tasks = |n: usize| {
        (0..n)
            .map(|i| (format!("github:acme/app#{i}"), json!({"title":format!("Fix {i}")})))
            .collect::<Vec<_>>()
    };
    let first = start_intake_runs(&c, &org, &d, &w, "source", "github", tasks(7), db::start_pinned_run)
        .unwrap();
    assert_eq!(first.len(), MAX_RUNS_PER_POLL, "a blocked run must not hold new issues back");
    let second = start_intake_runs(&c, &org, &d, &w, "source", "github", tasks(7), db::start_pinned_run)
        .unwrap();
    assert_eq!(second.len(), 2, "the next poll starts only the issues not seen yet");
    assert!(start_intake_runs(&c, &org, &d, &w, "source", "github", tasks(7), db::start_pinned_run)
        .unwrap()
        .is_empty());
    let status: String = c
        .query_row("SELECT status FROM factory_workflow_runs WHERE id=?1", [&stuck], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "blocked");
}

#[test]
fn qa_only_claims_fill_extra_slots_with_read_only_qa_checks() {
    let (c, org, user, runner, workspace) = setup();
    let mut functional = step("functional", &[], StepKind::Check);
    functional.adapter = "isolated-functional-qa".into();
    functional.options = json!({"required_checks": ["functional"]});
    let d = define(&c, &org, &user, &workspace, vec![step("build", &[], StepKind::Check), functional]);
    run(&c, &org, &d);
    let caps = vec!["adapter:test".to_string(), "adapter:isolated-functional-qa".to_string()];
    let qa = db::claim_where(&c, &org, &runner, &caps, 100, true).unwrap().unwrap();
    assert_eq!(qa["step"]["id"], "functional");
    // The pending build is never handed to a QA-only slot.
    assert!(db::claim_where(&c, &org, &runner, &caps, 101, true).unwrap().is_none());
    assert_eq!(db::claim(&c, &org, &runner, &caps, 102).unwrap().unwrap()["step"]["id"], "build");
}
