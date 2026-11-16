//! Shared per-action policy evaluation, with SHA/version-bound human approvals
//! and short-lived decision-model evidence for explicitly opted-in intake.
use anyhow::{bail, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use crate::factory::{contracts::{Action,ActionPolicy,TaskClass,Verdict,PolicyMode},policy_engine::{self,EvaluationInput,ModelDecision}};

fn fingerprint(policy:&ActionPolicy)->Result<String>{Ok(hex::encode(Sha256::digest(serde_json::to_vec(policy)?)))}
fn record(c:&Connection,run:&str,action:&str)->Result<Option<Value>> {
    let text:Option<String>=c.query_row("SELECT payload_json FROM factory_workflow_events WHERE run_id=?1 AND kind='action.model_decided' AND json_extract(payload_json,'$.action')=?2 AND (created_at>datetime('now','-10 minutes') OR EXISTS(SELECT 1 FROM factory_workflow_runs r WHERE r.id=?1 AND r.change_revision>0)) ORDER BY seq DESC LIMIT 1",[run,action],|r|r.get(0)).optional()?;
    text.map(|t|Ok(serde_json::from_str(&t)?)).transpose()
}
pub fn decision(c:&Connection,org:&str,run:&str,action:&str,paths:&[String],verified:bool)->Result<Value> {
    let action_kind:Action=serde_json::from_value(Value::String(action.into()))?;
    let detail=crate::db::workflows::run_detail(c,org,run)?;
    let policies:Vec<ActionPolicy>=crate::db::factory_queries::list_factory_policies(c,org)?.into_iter().map(|s|s.policy).collect();
    let project=detail["snapshot"]["workspace"]["name"].as_str();
    let recorded=record(c,run,action)?;
    let task_class:Option<TaskClass>=recorded.as_ref().and_then(|r|r.get("task_class")).or_else(||detail["task"].get("task_class")).and_then(|v|serde_json::from_value(v.clone()).ok());
    let selected=policy_engine::select_policy(&policies,action_kind,project,task_class);
    let model=if let (Some(record),Some(policy))=(&recorded,selected) {
        if record["policy_hash"]==fingerprint(policy)? && record["revision"]==if action=="fix" {json!(0)}else{detail["execution"]["revision"].clone()} && record["allowed"]==true {
            ModelDecision::Allow{confidence:record["confidence"].as_f64().unwrap_or(0.0)}
        }else{ModelDecision::NotConfigured}
    }else{ModelDecision::NotConfigured};
    let mut floors:Vec<String>=paths.iter().filter_map(|p|crate::automation::merge_gate::risky_path(p).map(|risk|format!("{risk}:{p}")).or_else(||crate::automation::merge_gate::is_never_eligible(p).then(||format!("verification_or_build_configuration:{p}")))).collect();
    if action=="fix" && crate::automation::merge_gate::risky_title(detail["task"]["title"].as_str().unwrap_or_default()).is_some() {floors.push("sensitive_task_needs_a_person".into());}
    let evaluation=policy_engine::evaluate(&EvaluationInput{action:Some(action_kind),project,task_class,floors,fix_verified:verified,merged:false},&policies,&model,0.8);
    let revision=if action=="fix"{0}else{detail["execution"]["revision"].as_u64().unwrap_or(0) as u32};
    let approved=crate::db::workflow_execution::approved(c,run,action,revision)?;
    let allowed=evaluation.verdict.verdict!=Verdict::Deny && (evaluation.verdict.verdict==Verdict::Allow || approved);
    Ok(json!({"action":action,"allowed":allowed,"never":evaluation.verdict.verdict==Verdict::Deny,"reason":evaluation.verdict.reason,"approved":approved,"revision":revision,"policy_version":evaluation.policy_version}))
}
pub fn require(c:&Connection,org:&str,run:&str,action:&str,paths:&[String],verified:bool)->Result<()> {
    let d=decision(c,org,run,action,paths,verified)?;
    if d["allowed"]!=true {bail!(if d["never"]==true{"policy_never"}else{"policy_approval_required"})}
    Ok(())
}

/// Opt-in only: existing Factory org consent, configured Jev key, matching
/// criteria policy, safe task classes and deterministic floors all apply.
pub async fn assess_intake(store:&crate::store::sqlite::SqliteStore,org:&str,run:&str)->Result<()> {
    if !crate::automation::shadow::shadow_enabled_for(org){return Ok(())}
    let Some(key)=crate::automation::shadow::shadow_key() else{return Ok(())};
    let detail={let h=store.conn();let c=h.lock().map_err(|_|anyhow::anyhow!("storage_unavailable"))?;crate::db::workflows::run_detail(&c,org,run)?};
    if detail["execution"]["revision"]!=0 || detail["status"]!="blocked" {return Ok(())}
    let privacy=detail["task"]["privacy_class"].as_str().unwrap_or("internal");
    if ["restricted","confidential"].contains(&privacy){return Ok(())}
    let title=detail["task"]["title"].as_str().unwrap_or_default();
    if crate::automation::merge_gate::risky_title(title).is_some(){return Ok(())}
    let source_class=detail["task"]["task_class"].as_str().unwrap_or("unknown");
    if source_class!="unknown" && !crate::automation::factory_intake::AUTO_START_CLASSES.contains(&source_class){return Ok(())}
    let policies={let h=store.conn();let c=h.lock().map_err(|_|anyhow::anyhow!("storage_unavailable"))?;crate::db::factory_queries::list_factory_policies(&c,org)?.into_iter().map(|s|s.policy).collect::<Vec<_>>()};
    if !policies.iter().any(|p|p.action==Action::Fix && p.mode==PolicyMode::Criteria && p.allow==["workflow_low_risk_intake_v1"] && p.stop.is_empty() && (p.scope.project.is_none() || p.scope.project.as_deref()==detail["snapshot"]["workspace"]["name"].as_str())){return Ok(())}
    let description=detail["task"]["description"].as_str().unwrap_or_default();
    let(answer,latency)=crate::factory::jev::decide(&reqwest::Client::new(),&key,&crate::factory::jev::Change{title,description,paths:&[]}).await?;
    if crate::automation::factory_intake::floor_after_model(source_class,&answer.task_class,&crate::factory::jev::verdict(&answer)).is_err(){return Ok(())}
    let class:TaskClass=serde_json::from_value(json!(answer.task_class))?;
    let h=store.conn();let c=h.lock().map_err(|_|anyhow::anyhow!("storage_unavailable"))?;
    let current=crate::db::workflows::run_detail(&c,org,run)?;
    if current["status"]!="blocked" || current["execution"]["revision"]!=0{return Ok(())}
    let policies=crate::db::factory_queries::list_factory_policies(&c,org)?.into_iter().map(|s|s.policy).collect::<Vec<_>>();
    let Some(policy)=policy_engine::select_policy(&policies,Action::Fix,current["snapshot"]["workspace"]["name"].as_str(),Some(class)) else{return Ok(())};
    if policy.mode!=PolicyMode::Criteria || policy.allow!=["workflow_low_risk_intake_v1"] || !policy.stop.is_empty(){return Ok(())}
    let starts:i64=c.query_row("SELECT count(*) FROM factory_workflow_events e JOIN factory_workflow_runs r ON r.id=e.run_id WHERE r.org_id=?1 AND e.kind='action.model_decided' AND e.created_at>=datetime('now','start of day')",[org],|r|r.get(0))?;
    if starts+crate::db::factory_intake::auto_starts_today(&c,org)? >=crate::db::factory_intake::MAX_AUTO_STARTS_PER_DAY{return Ok(())}
    crate::db::workflows::event(&c,run,None,"action.model_decided",json!({"action":"fix","revision":0,"task_class":answer.task_class,"confidence":answer.class_confidence.min(answer.risk_confidence),"allowed":true,"policy_hash":fingerprint(policy)?,"latency_ms":latency,"risk":answer.risk}))?;
    if decision(&c,org,run,"fix",&[],false)?["allowed"]==true {crate::db::workflows::control_run(&c,org,run,"resume",None)?;}
    Ok(())
}
