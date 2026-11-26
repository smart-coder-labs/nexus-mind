pub mod agent;
pub mod config;
pub mod executors;
pub mod ipc;
pub mod learning;
pub mod llm;
pub mod mcp;
pub mod nexusmind;
pub mod policy;
pub mod redact;
pub mod storage;
pub mod triggers;

#[cfg(test)]
mod tests {
    use crate::config::MascotConfig;
    use crate::ipc::{dispatch_rpc_json, DaemonState};
    use serde_json::{json, Value};
    use systemone_client::DecisionMode;
    use tempfile::TempDir;

    fn test_state(dir: &TempDir) -> std::sync::Arc<DaemonState> {
        let mut cfg = MascotConfig {
            db_path: Some(dir.path().join("mascot.db")),
            ipc_socket_path: Some(dir.path().join("mascot.sock")),
            skills_dir: Some(dir.path().join("skills")),
            ..Default::default()
        };
        cfg.decision_engine.mode = DecisionMode::DeterministicOnly;
        DaemonState::new(cfg).unwrap()
    }

    #[tokio::test]
    async fn ipc_lifecycle_privacy_killswitch_and_held_approval() {
        let dir = TempDir::new().unwrap();
        let state = test_state(&dir);

        // 1. Ping
        let ping: Value = serde_json::from_str(
            &dispatch_rpc_json(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#, &state).await,
        )
        .unwrap();
        assert_eq!(ping["result"]["ok"], json!(true));

        // 2. Propose sudo action -> must be Held deterministically
        let held: Value = serde_json::from_str(
            &dispatch_rpc_json(
                r#"{"jsonrpc":"2.0","id":2,"method":"propose_action","params":{"action_kind":"shell","summary":"Restart service","command_preview":"sudo launchctl kickstart system/foo"}}"#,
                &state,
            )
            .await,
        )
        .unwrap();
        assert_eq!(held["result"]["verdict"]["verdict"], json!("hold_for_human"));
        let approval_id = held["result"]["approval"]["id"].as_str().unwrap().to_string();

        // 3. Deny approval -> outcome tape records held + cancelled
        let deny_req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resolve_approval",
            "params": { "id": approval_id, "approved": false }
        })
        .to_string();
        let denied: Value =
            serde_json::from_str(&dispatch_rpc_json(&deny_req, &state).await).unwrap();
        assert_eq!(denied["result"]["approval"]["status"], json!("denied"));

        // 4. Privacy shield blocks passive observation
        let _ = dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":4,"method":"set_privacy_shield","params":{"enabled":true}}"#,
            &state,
        )
        .await;
        let obs: Value = serde_json::from_str(
            &dispatch_rpc_json(
                r#"{"jsonrpc":"2.0","id":5,"method":"record_observation","params":{"source":"active_window","app_name":"Safari","summary":"Viewing docs"}}"#,
                &state,
            )
            .await,
        )
        .unwrap();
        assert_eq!(obs["result"]["recorded"], json!(false));

        // 5. Kill switch blocks actions
        let _ = dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":6,"method":"kill_switch","params":{"active":true}}"#,
            &state,
        )
        .await;
        let blocked: Value = serde_json::from_str(
            &dispatch_rpc_json(
                r#"{"jsonrpc":"2.0","id":7,"method":"propose_action","params":{"action_kind":"shell","summary":"List files","command_preview":"ls"}}"#,
                &state,
            )
            .await,
        )
        .unwrap();
        assert_eq!(blocked["result"]["verdict"]["verdict"], json!("block"));
    }
}
