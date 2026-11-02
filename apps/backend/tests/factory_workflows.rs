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
fn repo(writable: bool) -> Repository {
    Repository {
        path: "api".into(),
        remote: "git@github.com:acme/api.git".into(),
        head_sha: "a".repeat(40),
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
    db::start_run(c, org, d, "linear:issue-1", &json!({"title":"Fix"})).unwrap()["id"]
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
fn inventory_requires_confirmation_and_runner_scope() {
    let (c, o, _, r, w) = setup();
    assert!(db::workspace_by_id(&c, "other", &w).is_err());
    assert!(db::report_inventory(&c, &o, "other-runner", &w, &[repo(false)]).is_err());
    assert!(db::report_inventory(&c, &o, &r, &w, &[repo(true)]).is_err());
    assert!(db::confirm_inventory(&c, &o, &w, 0, &[repo(true)]).is_err());
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
        db::start_run(&c, &o, &d, "linear:issue-1", &json!({})).unwrap()["id"]
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
