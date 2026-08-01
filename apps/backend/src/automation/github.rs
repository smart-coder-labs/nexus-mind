use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QaHandoffRequest {
    pub org_id: String,
    pub attempt_id: String,
    pub pr_number: u64,
    pub target_branch: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct QaHandoffResponse {
    pub success: bool,
    pub receipt_id: String,
}

pub fn verify_github_webhook_signature(payload: &[u8], secret: &str, signature_header: &str) -> bool {
    let Some(hex_sig) = signature_header.strip_prefix("sha256=") else {
        return false;
    };

    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(payload);
    let expected = format!("{:x}", hasher.finalize());

    hex_sig.eq_ignore_ascii_case(&expected)
}

pub fn process_qa_handoff(conn: &Connection, req: &QaHandoffRequest) -> anyhow::Result<QaHandoffResponse> {
    let status: String = conn.query_row(
        "SELECT status FROM automation_attempts WHERE id = ?1",
        [&req.attempt_id],
        |row| row.get(0),
    )?;

    if status != "active" {
        anyhow::bail!("qa_handoff_denied: attempt is not active (status: {status})");
    }

    let receipt_id = format!("receipt_qa_{}", req.pr_number);
    let payload_hash = format!("qa_handoff_{}_{}", req.pr_number, req.target_branch);

    crate::db::queries::record_automation_callback(
        conn,
        &req.org_id,
        &req.attempt_id,
        &receipt_id,
        &payload_hash,
    )?;

    Ok(QaHandoffResponse {
        success: true,
        receipt_id,
    })
}
