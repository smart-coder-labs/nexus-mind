//! Full End-to-End Integration Tests for NexusMind Mascot Daemon:
//! 1. Multi-step ReAct loop over real Unix Domain Socket IPC with mock Laya `/v1/systemone`,
//!    mock Claude tool-calling API, Held approval pause -> human approve -> automatic resume -> skill_save!
//! 2. Real stdio MCP server handshake, tool discovery, and tool invocation with redaction.
//! 3. Continuous learning pipeline: passive observations -> Laya worthiness -> idle consolidation -> proactive suggestion.

use nexus_mascot_daemon::config::{LlmProviderKind, MascotConfig, McpServerConfig};
use nexus_mascot_daemon::ipc::{dispatch_rpc_json, serve_unix_socket, DaemonState};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use systemone_client::DecisionMode;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixStream};

/// Spawns a tiny HTTP server that reads one HTTP/1.1 request per connection and invokes `handler`.
async fn spawn_http_mock<F>(handler: F) -> String
where
    F: Fn(Value) -> Value + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handler = Arc::new(handler);

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let h = Arc::clone(&handler);
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0_u8; 4096];
                let header_end;
                loop {
                    let n = match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = pos + 4;
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&buf[..header_end]);
                let content_len: usize = headers
                    .lines()
                    .find_map(|l| {
                        let lower = l.to_ascii_lowercase();
                        lower
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
                while buf.len() < header_end + content_len {
                    let n = match stream.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&chunk[..n]);
                }
                let body_val: Value =
                    serde_json::from_slice(&buf[header_end..header_end + content_len])
                        .unwrap_or(Value::Null);
                let reply = h(body_val);
                let payload = serde_json::to_vec(&reply).unwrap();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
                let _ = stream.write_all(&payload).await;
            });
        }
    });

    format!("http://{addr}")
}

async fn unix_rpc(sock_path: &std::path::Path, req: Value) -> Value {
    let stream = UnixStream::connect(sock_path).await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut line = serde_json::to_string(&req).unwrap();
    line.push('\n');
    w.write_all(line.as_bytes()).await.unwrap();
    let mut reader = BufReader::new(r);
    let mut resp = String::new();
    reader.read_line(&mut resp).await.unwrap();
    serde_json::from_str(&resp).unwrap()
}

#[tokio::test]
async fn e2e_react_agent_loop_with_laya_held_approval_resume_and_skill_save() {
    let dir = TempDir::new().unwrap();
    let input_file = dir.path().join("notes.txt");
    let output_file = dir.path().join("summary.md");
    std::fs::write(&input_file, "Meeting note: always deploy on Tuesdays.").unwrap();

    // 1. Mock local Laya /v1/systemone server handling route, risk, and completion_judge
    let laya_base = spawn_http_mock(|req| {
        let q = req.get("questions").and_then(Value::as_object);
        if q.is_some_and(|m| m.contains_key("route")) {
            json!({
                "model": "laya-mlx-1.0",
                "answers": { "route": { "type": "choice", "choice": "direct_os_tool", "confidence": 0.95 } }
            })
        } else if q.is_some_and(|m| m.contains_key("risk")) {
            let summary = req.pointer("/state/summary").and_then(Value::as_str).unwrap_or("");
            // Make fs_write medium-high risk so it triggers Held approval; skill_save low risk!
            let score = if summary.contains("fs_write") { 2.1 } else { 0.2 };
            json!({
                "model": "laya-mlx-1.0",
                "answers": {
                    "action_class": { "type": "choice", "choice": "reversible_local", "confidence": 0.94 },
                    "reversibility": { "type": "choice", "choice": "reversible", "confidence": 0.94 },
                    "risk": { "type": "score", "score": score, "confidence": 0.94 }
                }
            })
        } else {
            json!({
                "model": "laya-mlx-1.0",
                "answers": {
                    "next_action": { "type": "choice", "choice": "finish", "confidence": 0.96 },
                    "task_is_incomplete": { "type": "noul", "noul": 0.04 }
                }
            })
        }
    })
    .await;

    // 2. Mock Claude Messages API driving a 4-step tool-calling loop
    let turn_counter = Arc::new(AtomicUsize::new(0));
    let tc = Arc::clone(&turn_counter);
    let input_path_str = input_file.display().to_string();
    let output_path_str = output_file.display().to_string();

    let claude_base = spawn_http_mock(move |_req| {
        let turn = tc.fetch_add(1, Ordering::SeqCst);
        match turn {
            0 => json!({
                "content": [{
                    "type": "tool_use",
                    "id": "call_1",
                    "name": "fs_read",
                    "input": { "path": input_path_str }
                }]
            }),
            1 => json!({
                "content": [{
                    "type": "tool_use",
                    "id": "call_2",
                    "name": "fs_write",
                    "input": {
                        "path": output_path_str,
                        "content": "# Summary\nDeploy schedule: Tuesdays.\n"
                    }
                }]
            }),
            2 => json!({
                "content": [{
                    "type": "tool_use",
                    "id": "call_3",
                    "name": "skill_save",
                    "input": {
                        "slug": "summarize-notes",
                        "title": "Summarize meeting notes",
                        "trigger_hint": "When user asks to summarize notes.txt",
                        "steps_yaml": "steps:\n  - fs_read\n  - fs_write\n",
                        "scope": "global_user"
                    }
                }]
            }),
            _ => json!({
                "content": [{
                    "type": "text",
                    "text": "Listo: leí tus notas, guardé summary.md tras tu aprobación y aprendí la skill summarize-notes."
                }]
            }),
        }
    })
    .await;

    // 3. Boot real Unix socket IPC daemon
    let sock_path = dir.path().join("mascot.sock");
    let mut cfg = MascotConfig {
        db_path: Some(dir.path().join("mascot.db")),
        ipc_socket_path: Some(sock_path.clone()),
        skills_dir: Some(dir.path().join("skills")),
        ..Default::default()
    };
    cfg.decision_engine.mode = DecisionMode::LayaLocal;
    cfg.decision_engine.laya.endpoint = format!("{laya_base}/v1/systemone");
    cfg.llm.provider = LlmProviderKind::Claude;
    cfg.llm.base_url = Some(format!("{claude_base}/v1/messages"));
    cfg.llm.api_key_override = Some("test-anthropic-key".into());
    cfg.nexusmind.enabled = false;
    cfg.nexusmind.require_connection = false;

    let state = DaemonState::new(cfg).unwrap();
    let srv_state = Arc::clone(&state);
    let srv_sock = sock_path.clone();
    let server_handle = tokio::spawn(async move {
        let _ = serve_unix_socket(&srv_sock, srv_state).await;
    });
    tokio::time::sleep(Duration::from_millis(80)).await;

    // 4. Submit user prompt over real Unix socket -> should read file, then pause Held on fs_write
    let resp1 = unix_rpc(
        &sock_path,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "submit_prompt",
            "params": { "prompt": "Resume notes.txt en summary.md y aprende este flujo", "active_app": "Notes" }
        }),
    )
    .await;
    assert_eq!(resp1["result"]["run"]["status"], json!("waiting_approval"));
    let approval_id = resp1["result"]["run"]["pending_approval"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!output_file.exists(), "fs_write must NOT run before human approval");

    // 5. Human clicks "Permitir una vez" -> daemon executes fs_write, resumes Claude, saves skill & finishes!
    let resp2 = unix_rpc(
        &sock_path,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "resolve_approval",
            "params": { "id": approval_id, "approved": true }
        }),
    )
    .await;
    assert_eq!(resp2["result"]["resumed_run"]["status"], json!("completed"));
    assert_eq!(
        resp2["result"]["resumed_run"]["completion_verdict"],
        json!("finish")
    );
    assert!(output_file.exists(), "fs_write must run after approval");
    assert!(std::fs::read_to_string(&output_file)
        .unwrap()
        .contains("Tuesdays"));

    // 6. Verify reusable YAML skill file & SQLite skill registry & Outcome Tape
    let skill_yaml = dir.path().join("skills").join("summarize-notes.yaml");
    assert!(skill_yaml.exists(), "learned skill YAML file should be created");
    let skills = state.store.list_skills().unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].slug, "summarize-notes");

    let tape = state.store.outcome_tape(14).unwrap();
    assert!(tape.iter().any(|e| e.outcome == "held"));
    assert!(tape.iter().any(|e| e.outcome == "success"));

    server_handle.abort();
}

#[tokio::test]
async fn e2e_mcp_stdio_host_discovers_and_invokes_real_server() {
    let dir = TempDir::new().unwrap();
    let mut cfg = MascotConfig {
        db_path: Some(dir.path().join("mascot.db")),
        ..Default::default()
    };
    cfg.decision_engine.mode = DecisionMode::DeterministicOnly;
    let state = DaemonState::new(cfg).unwrap();

    // Minimal real JSON-RPC stdio MCP server in inline Python
    let py_server = r#"
import json, sys
for line in sys.stdin:
    msg = json.loads(line)
    m = msg.get("method")
    if "id" not in msg:
        continue
    rid = msg["id"]
    if m == "initialize":
        res = {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "demo", "version": "1.0"}}
    elif m == "tools/list":
        res = {"tools": [{"name": "echo_upper", "description": "Uppercase text", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}}}]}
    elif m == "tools/call":
        txt = msg["params"]["arguments"].get("text", "")
        res = {"content": [{"type": "text", "text": txt.upper() + " sk-ant-api03-secretvalue1234567890"}]}
    else:
        res = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": rid, "result": res}) + "\n")
    sys.stdout.flush()
"#;

    let count = state
        .mcp
        .connect_server(&McpServerConfig {
            id: "demo-mcp".into(),
            command: "python3".into(),
            args: vec!["-u".into(), "-c".into(), py_server.into()],
            env: HashMap::new(),
            enabled: true,
        })
        .await
        .unwrap();
    assert_eq!(count, 1);

    let tools = state.mcp.list_tools().await;
    assert_eq!(tools[0].qualified_name, "mcp__demo_mcp__echo_upper");

    let output = state
        .mcp
        .call_tool("demo-mcp", "echo_upper", json!({ "text": "hello nexusmind" }))
        .await
        .unwrap();
    assert!(output.contains("HELLO NEXUSMIND"));
    assert!(
        !output.contains("sk-ant-api03-secretvalue1234567890"),
        "MCP tool output must be redacted locally"
    );
}

#[tokio::test]
async fn e2e_continuous_learning_and_proactive_suggestion_pipeline() {
    let dir = TempDir::new().unwrap();

    let laya_base = spawn_http_mock(|req| {
        let q = req.get("questions").and_then(Value::as_object);
        if q.is_some_and(|m| m.contains_key("worthiness")) {
            let summary = req.pointer("/state/summary").and_then(Value::as_str).unwrap_or("");
            if summary.contains("empty finder") {
                json!({
                    "model": "laya-mlx-1.0",
                    "answers": {
                        "worthiness": { "type": "choice", "choice": "ignore", "confidence": 0.97 },
                        "importance": { "type": "score", "score": 0.0, "confidence": 0.97 }
                    }
                })
            } else {
                json!({
                    "model": "laya-mlx-1.0",
                    "answers": {
                        "worthiness": { "type": "choice", "choice": "procedural_skill", "confidence": 0.92 },
                        "importance": { "type": "score", "score": 2.4, "confidence": 0.91 }
                    }
                })
            }
        } else {
            json!({
                "model": "laya-mlx-1.0",
                "answers": {
                    "interrupt_action": { "type": "choice", "choice": "queue_for_idle", "confidence": 0.93 }
                }
            })
        }
    })
    .await;

    let mut cfg = MascotConfig {
        db_path: Some(dir.path().join("mascot.db")),
        ..Default::default()
    };
    cfg.decision_engine.mode = DecisionMode::LayaLocal;
    cfg.decision_engine.laya.endpoint = format!("{laya_base}/v1/systemone");
    cfg.nexusmind.enabled = false;
    cfg.nexusmind.require_connection = false;
    let state = DaemonState::new(cfg).unwrap();

    // Noise event -> Laya returns ignore -> must NOT be stored
    let noise: Value = serde_json::from_str(
        &dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":1,"method":"record_observation","params":{"source":"active_window","app_name":"Finder","summary":"empty finder window"}}"#,
            &state,
        )
        .await,
    )
    .unwrap();
    assert_eq!(noise["result"]["recorded"], json!(false));

    // Repeated workflow observed 3x -> recorded -> consolidation creates proactive suggestion!
    for i in 1..=3 {
        let obs_req = json!({
            "jsonrpc": "2.0",
            "id": i + 1,
            "method": "record_observation",
            "params": {
                "source": "terminal",
                "app_name": "Terminal",
                "summary": "Export weekly Stripe CSV and generate summary report",
                "repeat_count": i
            }
        })
        .to_string();
        let res: Value = serde_json::from_str(&dispatch_rpc_json(&obs_req, &state).await).unwrap();
        assert_eq!(res["result"]["recorded"], json!(true));
    }

    let cons: Value = serde_json::from_str(
        &dispatch_rpc_json(r#"{"jsonrpc":"2.0","id":10,"method":"run_consolidation"}"#, &state)
            .await,
    )
    .unwrap();
    assert_eq!(cons["result"]["consolidated_events"], json!(3));
    assert_eq!(cons["result"]["suggestions_created"], json!(1));

    let suggestions = state.store.list_suggestions().unwrap();
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].delivery_mode, "queue_for_idle");
}

#[tokio::test]
async fn e2e_mandatory_nexusmind_gate_and_remote_audit_sync() {
    let dir = TempDir::new().unwrap();
    let heartbeats = Arc::new(AtomicUsize::new(0));
    let events_seen = Arc::new(AtomicUsize::new(0));
    let convs_seen = Arc::new(AtomicUsize::new(0));

    let hb_c = Arc::clone(&heartbeats);
    let ev_c = Arc::clone(&events_seen);
    let cv_c = Arc::clone(&convs_seen);

    let nm_base = spawn_http_mock(move |body| {
        if body.get("pet_name").is_some() {
            hb_c.fetch_add(1, Ordering::SeqCst);
            json!({ "connected": true, "org_id": "org-1", "user_id": "user-1" })
        } else if body.get("event_kind").is_some() {
            ev_c.fetch_add(1, Ordering::SeqCst);
            json!({ "ok": true })
        } else if body.get("transcript_json").is_some() {
            cv_c.fetch_add(1, Ordering::SeqCst);
            json!({ "ok": true })
        } else {
            json!({ "ok": true, "results": [] })
        }
    })
    .await;

    // 1. Default config has require_connection = true and NO NexusMind API key -> MUST block usage
    let mut cfg = MascotConfig {
        db_path: Some(dir.path().join("mascot.db")),
        ..Default::default()
    };
    cfg.decision_engine.mode = DecisionMode::DeterministicOnly;
    cfg.nexusmind.base_url = nm_base.clone();
    let state = DaemonState::new(cfg).unwrap();

    let blocked: Value = serde_json::from_str(
        &dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":1,"method":"submit_prompt","params":{"prompt":"Hola"}}"#,
            &state,
        )
        .await,
    )
    .unwrap();
    assert!(
        blocked.get("error").is_some(),
        "submit_prompt must fail when NexusMind is not connected"
    );

    // 2. Connect NexusMind via save_settings -> heartbeat succeeds and unlocks usage!
    let save_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "save_settings",
        "params": {
            "nexusmind_base_url": nm_base,
            "nexusmind_api_key": "nm_live_valid_key_12345"
        }
    })
    .to_string();
    let saved: Value = serde_json::from_str(&dispatch_rpc_json(&save_req, &state).await).unwrap();
    assert_eq!(saved["result"]["nexusmind_connected"], json!(true));
    assert!(heartbeats.load(Ordering::SeqCst) >= 1);

    // 3. Now submit_prompt succeeds AND records local + remote audit logs & conversation
    let ok_resp: Value = serde_json::from_str(
        &dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":3,"method":"submit_prompt","params":{"prompt":"Hola Nexi","active_app":"Terminal"}}"#,
            &state,
        )
        .await,
    )
    .unwrap();
    assert!(ok_resp.get("error").is_none());
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(events_seen.load(Ordering::SeqCst) >= 1);
    assert!(convs_seen.load(Ordering::SeqCst) >= 1);

    // 4. Local audit endpoints list_conversations and list_activity_logs return records
    let logs: Value = serde_json::from_str(
        &dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":4,"method":"list_activity_logs","params":{"limit":20}}"#,
            &state,
        )
        .await,
    )
    .unwrap();
    assert!(!logs["result"]["logs"].as_array().unwrap().is_empty());

    // 5. Seed legacy offline conversation + action_audit + skill and verify idempotent sync_history migration
    let now = chrono::Utc::now();
    state
        .store
        .save_agent_run(&nexus_mascot_daemon::storage::AgentRunRecord {
            id: "legacy-run-before-nm".into(),
            created_at: now,
            updated_at: now,
            objective: "Legacy offline chat with Nexi".into(),
            status: "completed".into(),
            route: Some("general_llm".into()),
            steps_executed: 1,
            tool_errors: 0,
            state_json: r#"{"messages":[{"role":"user","text":"Legacy offline chat"}]}"#.into(),
            final_reply: Some("Done offline!".into()),
        })
        .unwrap();
    state
        .store
        .record_outcome("Legacy desktop action before NexusMind", "shell", "jev_cloud", 0.2, "success")
        .unwrap();
    state
        .store
        .upsert_skill("legacy-skill", "Legacy Skill", "manual", "steps: []", "global_user")
        .unwrap();

    let sync1: Value = serde_json::from_str(
        &dispatch_rpc_json(r#"{"jsonrpc":"2.0","id":5,"method":"sync_history"}"#, &state).await,
    )
    .unwrap();
    assert!(sync1["result"]["conversations_migrated"].as_u64().unwrap() >= 1);
    assert!(sync1["result"]["events_migrated"].as_u64().unwrap() >= 1);
    assert_eq!(sync1["result"]["skills_migrated"], json!(1));

    // Re-running sync_history must be idempotent (0 duplicate uploads)
    let sync2: Value = serde_json::from_str(
        &dispatch_rpc_json(r#"{"jsonrpc":"2.0","id":6,"method":"sync_history"}"#, &state).await,
    )
    .unwrap();
    assert_eq!(sync2["result"]["conversations_migrated"], json!(0));
    assert_eq!(sync2["result"]["events_migrated"], json!(0));
    assert_eq!(sync2["result"]["skills_migrated"], json!(0));
}

#[tokio::test]
async fn e2e_multi_turn_conversation_preserves_context_and_auto_stores_nexusmind_memory() {
    let dir = TempDir::new().unwrap();
    let memories_stored = Arc::new(AtomicUsize::new(0));
    let mem_c = Arc::clone(&memories_stored);

    // Mock NexusMind backend handling heartbeat, org-wide memory search, store_memory, events, conversations
    let nm_base = spawn_http_mock(move |body| {
        if body.get("pet_name").is_some() {
            json!({ "connected": true, "org_id": "org-1", "user_id": "u-1" })
        } else if body.get("query").is_some() {
            // Verify search is org-wide (no forced `project` filter!)
            assert!(
                body.get("project").is_none(),
                "search_memory must search org-wide, not restricted to personal-mascot"
            );
            json!({
                "memories": [{
                    "project": "clients",
                    "type": "project",
                    "title": "Perfil de Yuliana",
                    "content": "Yuliana es la directora comercial de PadelClub."
                }]
            })
        } else if body.get("topic_key").is_some() {
            mem_c.fetch_add(1, Ordering::SeqCst);
            json!({ "id": "mem-1" })
        } else {
            json!({ "ok": true })
        }
    })
    .await;

    // Mock Claude server that inspects received messages to verify Turn 2 carries Turn 1 context + NexusMind memory!
    let turn_idx = Arc::new(AtomicUsize::new(0));
    let ti = Arc::clone(&turn_idx);
    let claude_base = spawn_http_mock(move |req| {
        let idx = ti.fetch_add(1, Ordering::SeqCst);
        let msgs = req["messages"].as_array().unwrap();
        let system = req["system"].as_str().unwrap_or("");
        if idx == 0 {
            assert!(
                system.contains("Perfil de Yuliana"),
                "System prompt must include org-wide NexusMind memory"
            );
            assert_eq!(msgs.len(), 1);
            json!({ "content": [{ "type": "text", "text": "Perfecto, creé la presentación de PadelClub para Yuliana." }] })
        } else {
            // Turn 2 MUST receive Turn 1 user + Turn 1 assistant + Turn 2 user (>= 3 messages in the SAME conversation!)
            assert!(
                msgs.len() >= 3,
                "Follow-up turn lost conversation history! Only got {} messages",
                msgs.len()
            );
            json!({ "content": [{ "type": "text", "text": "Sí, ya subí la presentación de PadelClub a Drive." }] })
        }
    })
    .await;

    let mut cfg = MascotConfig {
        db_path: Some(dir.path().join("mascot.db")),
        ..Default::default()
    };
    cfg.decision_engine.mode = DecisionMode::DeterministicOnly;
    cfg.nexusmind.base_url = nm_base.clone();
    cfg.llm.provider = LlmProviderKind::Claude;
    cfg.llm.base_url = Some(format!("{claude_base}/v1/messages"));
    cfg.llm.api_key_override = Some("test-claude-key".into());
    let state = DaemonState::new(cfg).unwrap();

    // Connect NexusMind
    let save_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "save_settings",
        "params": { "nexusmind_base_url": nm_base, "nexusmind_api_key": "nm_live_key_123" }
    })
    .to_string();
    let _ = dispatch_rpc_json(&save_req, &state).await;

    // Turn 1
    let r1: Value = serde_json::from_str(
        &dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":2,"method":"submit_prompt","params":{"prompt":"Prepara las propuestas para Yuliana de PadelClub"}}"#,
            &state,
        )
        .await,
    )
    .unwrap();
    let run_id_1 = r1["result"]["run"]["run_id"].as_str().unwrap().to_string();

    // Turn 2 (follow-up without repeating context!) -> MUST reuse the SAME run_id and pass prior history!
    let r2: Value = serde_json::from_str(
        &dispatch_rpc_json(
            r#"{"jsonrpc":"2.0","id":3,"method":"submit_prompt","params":{"prompt":"lo pudiste subir?"}}"#,
            &state,
        )
        .await,
    )
    .unwrap();
    let run_id_2 = r2["result"]["run"]["run_id"].as_str().unwrap().to_string();
    assert_eq!(run_id_1, run_id_2, "Follow-up prompt must stay in the same conversation thread");
    assert!(
        memories_stored.load(Ordering::SeqCst) >= 2,
        "Each completed turn must automatically persist memory to NexusMind /v1/memory/store"
    );

    // Explicit `new_conversation` -> next turn starts a brand new conversation id!
    let _ = dispatch_rpc_json(r#"{"jsonrpc":"2.0","id":4,"method":"new_conversation"}"#, &state).await;
    assert!(state.get_active_conversation_id().is_none());
}
