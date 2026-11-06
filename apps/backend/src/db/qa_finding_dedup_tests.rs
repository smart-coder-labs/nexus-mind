use super::*;
use crate::db::{connection::connect, migrations};
use crate::models::types::CreateAutonomousAgentRequest;

const TITLE: &str = "[Providers] Import opens a product-catalog dialog";
const OLD_FP: &str = "providers-import-shows-product-catalog-dialog";
const NEW_FP: &str = "providers-import-opens-product-catalog-dialog";
const LOCATION: &str = "/providers → Import";

struct Fixture {
    conn: Connection,
    org: String,
    definition: String,
    runs: [String; 2],
}

fn fixture(template: &str) -> Fixture {
    let conn = connect(":memory:").unwrap();
    migrations::run_all(&conn).unwrap();
    let (org, user, _) = bootstrap(&conn, "Acme", "acme", "qa@acme.test", "QA").unwrap();
    let agent = create_autonomous_agent_definition(&conn, &org.id, &user.id, &CreateAutonomousAgentRequest {
        name: "QA".into(), description: None, template_key: template.into(),
        config: serde_json::json!({"outputs":["nexusmind"],"repository":"acme/app"}),
        budgets: serde_json::json!({"wall_time_seconds":300}),
    }).unwrap();
    validate_autonomous_agent_definition(&conn, &org.id, &user.id, &agent.definition.id).unwrap();
    set_autonomous_agent_status(&conn, &org.id, &agent.definition.id, "enabled").unwrap();
    let mut runs = Vec::new();
    for occurrence in ["first", "second"] {
        runs.push(enqueue_autonomous_agent_run(&conn, &org.id, &agent.definition.id, "manual", occurrence, None, None).unwrap().unwrap().id);
    }
    Fixture { conn, org: org.id, definition: agent.definition.id, runs: [runs.remove(0), runs.remove(0)] }
}

fn evidence(fingerprint: &str, location: &str) -> serde_json::Value {
    serde_json::json!({"fingerprint":fingerprint,"module":"Providers","location":location,"screenshot":"latest.png"})
}

impl Fixture {
    fn detect(&self, run: usize, fingerprint: &str, title: &str, location: &str) -> AutonomousAgentFinding {
        upsert_autonomous_agent_finding(&self.conn, &self.org, &self.definition, &self.runs[run], fingerprint, title, "high", "Latest evidence", &evidence(fingerprint, location)).unwrap()
    }
}

#[test]
fn reuses_open_finding_when_model_rewords_its_fingerprint() {
    let f = fixture("qa");
    let first = f.detect(0, OLD_FP, TITLE, LOCATION);
    let again = f.detect(1, NEW_FP, TITLE, LOCATION);
    assert_eq!(again.id, first.id);
    assert_eq!(again.fingerprint, OLD_FP);
    assert_eq!(again.evidence["fingerprint"], OLD_FP);
    assert_eq!(again.occurrence_count, 2);
    assert_eq!(again.run_id, f.runs[1]);
    assert_eq!(again.created_at, first.created_at);
    assert_eq!(again.status, "open");
    assert_eq!(list_autonomous_agent_findings(&f.conn, &f.org).unwrap().len(), 1);
    let first_delivery = create_autonomous_agent_delivery(&f.conn, &f.org, &f.runs[0], Some(&first.id), "github_issue", &format!("{}:github_issue", first.fingerprint)).unwrap();
    let again_delivery = create_autonomous_agent_delivery(&f.conn, &f.org, &f.runs[1], Some(&again.id), "github_issue", &format!("{}:github_issue", again.fingerprint)).unwrap();
    assert_eq!(first_delivery.id, again_delivery.id, "canonical identity also preserves delivery idempotency");
}

#[test]
fn normalizes_case_separators_and_title_punctuation_without_fuzzy_matching() {
    let f = fixture("qa");
    let first = f.detect(0, "PROVIDERS_Import_Dialog", TITLE, LOCATION);
    assert_eq!(f.detect(1, "providers-import-dialog", "Different wording", LOCATION).id, first.id);
    let f = fixture("qa");
    let first = f.detect(0, OLD_FP, TITLE, LOCATION);
    assert_eq!(f.detect(1, NEW_FP, "[providers] import opens a product catalog dialog!", LOCATION).id, first.id);
}

#[test]
fn does_not_merge_other_locations_modules_or_different_symptoms() {
    let f = fixture("qa");
    let first = f.detect(0, OLD_FP, TITLE, LOCATION);
    assert_ne!(f.detect(1, NEW_FP, TITLE, "/products → Import").id, first.id);
    assert_ne!(f.detect(1, "another-symptom", "[Providers] Import button is disabled", LOCATION).id, first.id);
    let mut other_module = evidence("other-module", LOCATION);
    other_module["module"] = serde_json::json!("Products");
    let other = upsert_autonomous_agent_finding(&f.conn, &f.org, &f.definition, &f.runs[1], "other-module", TITLE, "high", "Different module", &other_module).unwrap();
    assert_ne!(other.id, first.id);
}

#[test]
fn preserves_resolved_and_ignored_status_and_only_fallback_matches_open_rows() {
    for status in ["resolved", "ignored"] {
        let f = fixture("qa");
        let first = f.detect(0, OLD_FP, TITLE, LOCATION);
        patch_autonomous_agent_finding(&f.conn, &f.org, &first.id, status).unwrap();
        assert_eq!(f.detect(1, OLD_FP, TITLE, LOCATION).status, status, "exact-fingerprint behavior is unchanged");
        assert!(open_qa_finding_references(&f.conn, &f.org, &f.definition).unwrap().is_empty());
        let fresh = f.detect(1, NEW_FP, TITLE, LOCATION);
        assert_ne!(fresh.id, first.id);
        assert_eq!(fresh.status, "open");
    }
}

#[test]
fn leaves_non_qa_fingerprints_and_generic_titles_independent() {
    let f = fixture("github_pr_reviewer");
    let first = f.detect(0, OLD_FP, TITLE, LOCATION);
    assert_ne!(f.detect(1, NEW_FP, TITLE, LOCATION).id, first.id);
    let qa = fixture("qa");
    let generic = qa.detect(0, "generic-1", "QA finding", "");
    assert_ne!(qa.detect(1, "generic-2", "QA finding", "").id, generic.id);
}

#[test]
fn reference_context_is_scoped_and_bounded() {
    let f = fixture("qa");
    f.detect(0, OLD_FP, TITLE, LOCATION);
    let refs = open_qa_finding_references(&f.conn, &f.org, &f.definition).unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["fingerprint"], OLD_FP);
    assert_eq!(refs[0]["location"], LOCATION);
    assert!(refs[0].get("screenshot").is_none());
    assert!(open_qa_finding_references(&f.conn, "other-org", &f.definition).unwrap().is_empty());
    assert!(open_qa_finding_references(&f.conn, &f.org, "other-agent").unwrap().is_empty());
}

#[test]
fn concurrent_connections_cannot_insert_two_open_rows_for_the_same_qa_defect() {
    let f = fixture("qa");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qa.db");
    f.conn.execute("VACUUM INTO ?1", [path.to_str().unwrap()]).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = [OLD_FP, NEW_FP].into_iter().enumerate().map(|(i, fp)| {
        let (path, org, definition, run, barrier) = (path.clone(), f.org.clone(), f.definition.clone(), f.runs[i].clone(), barrier.clone());
        std::thread::spawn(move || {
            let conn = connect(path.to_str().unwrap()).unwrap();
            conn.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
            barrier.wait();
            upsert_autonomous_agent_finding(&conn, &org, &definition, &run, fp, TITLE, "high", "Evidence", &evidence(fp, LOCATION)).unwrap().id
        })
    }).collect();
    let ids: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();
    assert_eq!(ids[0], ids[1]);
    let conn = connect(path.to_str().unwrap()).unwrap();
    let rows = list_autonomous_agent_findings(&conn, &f.org).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].occurrence_count, 2);
}
