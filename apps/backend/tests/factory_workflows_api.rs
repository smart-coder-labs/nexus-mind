use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexusmind::{
    api::router,
    auth::password::hash_password,
    config::Config,
    db::{connection, migrations, queries},
};
use serde_json::{json, Value};
use tower::util::ServiceExt;
#[tokio::test]
async fn authenticated_api_has_no_plan_request_quota_but_still_requires_auth() {
    let app = app("super_user");
    let cookie = login(&app).await;
    for _ in 0..120 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/autonomous-agents/settings")
                    .header("Cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key("x-ratelimit-limit"));
        assert!(!response.headers().contains_key("x-ratelimit-remaining"));
    }
    assert_eq!(
        request(&app, "GET", "/v1/autonomous-agents/settings", None, None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
}
#[tokio::test]
async fn shared_pause_requires_authority_over_both_execution_engines() {
    let app = app("fleet_only");
    let cookie = login(&app).await;
    let (status, _) = request(
        &app,
        "PATCH",
        "/v1/autonomous-agents/settings",
        Some(&cookie),
        None,
        Some(json!({"enabled":false})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, current) = request(
        &app,
        "GET",
        "/v1/autonomous-agents/settings",
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["enabled"], true);
    let (status, _) = request(
        &app,
        "PATCH",
        "/v1/autonomous-agents/settings",
        Some(&cookie),
        None,
        Some(json!({"retention_days":30})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
#[tokio::test]
async fn runtime_settings_require_owner_permission_and_runner_scoped_reports() {
    let app = app("super_user");
    let cookie = login(&app).await;
    let (_, r) = request(
        &app,
        "POST",
        "/v1/factory/runners",
        Some(&cookie),
        None,
        Some(json!({"name":"Runtime"})),
    )
    .await;
    let (_, w) = request(
        &app,
        "POST",
        "/v1/factory/workspaces",
        Some(&cookie),
        None,
        Some(json!({"runner_id":r["id"],"name":"Runtime","root_path":"/work/runtime"})),
    )
    .await;
    let id = w["id"].as_str().unwrap();
    let settings = format!("/v1/factory/workspaces/{id}/runtime");
    let reports = format!("/v1/factory/runner/workspaces/{id}/runtime-report");
    let token = r["token"].as_str().unwrap();
    let body = json!({"revision":0,"config":{"agents":[],"checks":[],"services":[]}});
    assert!(request(
        &app,
        "PUT",
        &settings,
        None,
        Some(token),
        Some(body.clone())
    )
    .await
    .0
    .is_client_error());
    assert!(
        request(&app, "POST", &reports, Some(&cookie), None, Some(json!({})))
            .await
            .0
            .is_client_error()
    );
    let (status, saved) = request(
        &app,
        "PUT",
        &settings,
        Some(&cookie),
        None,
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["ready"], false);
    assert_eq!(
        request(&app, "PUT", &settings, Some(&cookie), None, Some(body))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (status, reported) = request(
        &app,
        "POST",
        &reports,
        None,
        Some(token),
        Some(json!({"remote_enabled":false,"applied_revision":1})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reported["ready"], false);
    assert_eq!(
        request(
            &app,
            "POST",
            &reports,
            None,
            Some(token),
            Some(json!({"applied_revision":99}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, other) = request(
        &app,
        "POST",
        "/v1/factory/runners",
        Some(&cookie),
        None,
        Some(json!({"name":"Other"})),
    )
    .await;
    assert_eq!(
        request(
            &app,
            "POST",
            &reports,
            None,
            other["token"].as_str(),
            Some(json!({}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert!(request(
        &app,
        "PUT",
        &settings,
        Some(&cookie),
        None,
        Some(json!({"revision":1,"config":{"api_key":"fixture-not-a-secret"}}))
    )
    .await
    .0
    .is_client_error());
}
#[cfg(unix)]
#[tokio::test]
async fn real_python_runner_discovers_and_prepares_without_touching_personal_changes() {
    use std::os::unix::fs::PermissionsExt;
    let app = app("super_user");
    let cookie = login(&app).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server_app = app.clone();
    let server = tokio::spawn(async move { axum::serve(listener, server_app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("projects");
    let source = root.join("api");
    std::fs::create_dir_all(&source).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&source)
            .output()
            .unwrap();
        assert!(output.status.success(), "git fixture failed");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Test"]);
    std::fs::write(source.join("code.txt"), "committed").unwrap();
    git(&["add", "code.txt"]);
    git(&["commit", "-qm", "initial"]);
    std::fs::write(source.join("code.txt"), "personal changes").unwrap();
    std::fs::write(source.join(".env"), "PASSWORD=private").unwrap();
    let (_, runner) = request(
        &app,
        "POST",
        "/v1/factory/runners",
        Some(&cookie),
        None,
        Some(json!({"name":"integration runner"})),
    )
    .await;
    let (status, workspace) = request(
        &app,
        "POST",
        "/v1/factory/workspaces",
        Some(&cookie),
        None,
        Some(json!({"runner_id":runner["id"],"name":"App","root_path":root})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let config = dir.path().join("runner.json");
    std::fs::write(&config, json!({"api_url":format!("http://{address}"),"runner_token":runner["token"],"allowed_roots":[root],"state_dir":dir.path().join("state")}).to_string()).unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/factory/runner.py");
    let tick = || {
        let mut command = tokio::process::Command::new("python3");
        command
            .arg(&script)
            .arg("--config")
            .arg(&config)
            .arg("--once");
        command
    };
    let output = tick().output().await.unwrap();
    assert!(output.status.success(), "discovery failed");
    let (_, catalog) = request(
        &app,
        "GET",
        "/v1/factory/workflow-catalog",
        Some(&cookie),
        None,
        None,
    )
    .await;
    let inventory = &catalog["workspaces"][0];
    assert_eq!(inventory["discovered"][0]["path"], "api");
    let (status,_)=request(&app,"POST",&format!("/v1/factory/workspaces/{}/confirm",workspace["id"].as_str().unwrap()),Some(&cookie),None,Some(json!({"revision":inventory["inventory_revision"],"repositories":inventory["discovered"]}))).await;
    assert_eq!(status, StatusCode::OK);
    let (status,definition)=request(&app,"POST","/v1/factory/workflows",Some(&cookie),None,Some(json!({"name":"Prepare","workspace_id":workspace["id"],"steps":[{"id":"prepare","name":"Prepare","kind":"prepare","depends_on":[],"adapter":"prepare-workspace","capabilities":["workspace:prepare"],"write_repositories":[],"max_attempts":2,"timeout_seconds":300}]}))).await;
    assert_eq!(status, StatusCode::OK);
    let (status,run)=request(&app,"POST","/v1/factory/workflow-runs",Some(&cookie),None,Some(json!({"definition_id":definition["id"],"source_key":"manual:e2e","pinned_sources":true,"task":{"title":"Prepare committed code"}}))).await;
    assert_eq!(status, StatusCode::OK);
    let output = tick().output().await.unwrap();
    assert!(output.status.success(), "preparation failed");
    let (_, finished) = request(
        &app,
        "GET",
        &format!("/v1/factory/workflow-runs/{}", run["id"].as_str().unwrap()),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(finished["status"], "succeeded");
    let checkout = dir
        .path()
        .join("state")
        .join(run["id"].as_str().unwrap())
        .join("api");
    assert_eq!(
        std::fs::read_to_string(checkout.join("code.txt")).unwrap(),
        "committed"
    );
    assert!(!checkout.join(".env").exists());
    assert_eq!(
        std::fs::read_to_string(source.join("code.txt")).unwrap(),
        "personal changes"
    );
    assert!(finished["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "step.succeeded"));
    if let Ok(image) = std::env::var("FACTORY_TEST_IMAGE") {
        // Real containers, real HTTP, two independent writers and clean review.
        // Agent commands are deterministic fixtures, not a live model provider.
        let mut repos = inventory["discovered"].clone();
        repos[0]["writable"] = json!(true);
        let (status, _) = request(
            &app,
            "POST",
            &format!(
                "/v1/factory/workspaces/{}/confirm",
                workspace["id"].as_str().unwrap()
            ),
            Some(&cookie),
            None,
            Some(json!({"revision":inventory["inventory_revision"],"repositories":repos})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let agent =
            |code: &str| json!({"kind":"agent","image":image,"command":["python3","-c",code]});
        let config_data = json!({"api_url":format!("http://{address}"),"runner_token":runner["token"],"allowed_roots":[root],"state_dir":dir.path().join("state"),
            "adapters":{
                "investigator":agent("import json; print(json.dumps({'success':True,'summary':'Private implementation reasoning'}))"),
                "test-author":agent("from pathlib import Path; import json,subprocess; p=Path('/work/api/regression.py');p.write_text(\"from pathlib import Path; assert Path('/work/api/code.txt').read_text()=='fixed'\"); assert subprocess.run(['python3',str(p)]).returncode!=0; print(json.dumps({'success':True,'summary':'Regression reproduces the failure'}))"),
                "implementer":agent("from pathlib import Path; import json; Path('/work/api/code.txt').write_text('fixed');print(json.dumps({'success':True,'summary':'Fixed across the task scope'}))"),
                "functional":{"kind":"checks","image":image,"network":"services","checks":[{"name":"functional","command":["python3","-c","import urllib.request; assert urllib.request.urlopen('http://backend:8080/code.txt').read()==b'fixed'"]},{"name":"regression","command":["python3","/work/api/regression.py"]}]},
                "security":{"kind":"checks","image":image,"checks":[{"name":"security","command":["python3","-c","from pathlib import Path;assert not Path('/work/api/.env').exists()"]}]},
                "reviewer":agent("import json;from pathlib import Path;c=json.loads(Path('/input/context.json').read_text());assert not any(k in c['dependency_results'] for k in ['investigate','tests','implement']);assert Path('/work/api/code.txt').read_text()=='fixed';print(json.dumps({'success':True,'verdict':'accept','findings':[]}))")
            },
            "workspace_profiles":{workspace["id"].as_str().unwrap():{"services":{"backend":{"image":image,"cwd":"api","command":["python3","-m","http.server","8080"],"healthcheck":["python3","-c","import urllib.request; urllib.request.urlopen('http://localhost:8080/code.txt')"]}}}}
        });
        std::fs::write(&config, config_data.to_string()).unwrap();
        let s = |id: &str, kind: &str, adapter: &str, deps: Vec<&str>, writes: Vec<&str>| json!({"id":id,"name":id,"kind":kind,"adapter":adapter,"depends_on":deps,"write_repositories":writes,"max_attempts":3,"timeout_seconds":120});
        let definition = json!({"name":"Isolated delivery","workspace_id":workspace["id"],"repair_step":"implement","max_repairs":1,"steps":[
            s("prepare","prepare","prepare-workspace",vec![],vec![]),s("investigate","agent","investigator",vec!["prepare"],vec![]),
            s("tests","agent","test-author",vec!["investigate"],vec!["api"]),s("implement","agent","implementer",vec!["tests"],vec!["api"]),
            s("environment","check","integrated-environment",vec!["implement"],vec![]),s("functional","check","functional",vec!["environment"],vec![]),
            s("security","check","security",vec!["environment"],vec![]),s("gate","gate","evidence-gate",vec!["functional","security"],vec![]),
            s("review","review","reviewer",vec!["gate"],vec![])
        ]});
        let (status, d) = request(
            &app,
            "POST",
            "/v1/factory/workflows",
            Some(&cookie),
            None,
            Some(definition),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{d}");
        let(status,r)=request(&app,"POST","/v1/factory/workflow-runs",Some(&cookie),None,Some(json!({"definition_id":d["id"],"source_key":"manual:container-delivery","pinned_sources":true,"task":{"title":"Fix and review"}}))).await;
        assert_eq!(status, StatusCode::OK);
        for _ in 0..12 {
            let output = tick().output().await.unwrap();
            assert!(
                output.status.success(),
                "runner failed: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            let (_, state) = request(
                &app,
                "GET",
                &format!("/v1/factory/workflow-runs/{}", r["id"].as_str().unwrap()),
                Some(&cookie),
                None,
                None,
            )
            .await;
            assert_ne!(state["status"], "blocked", "{state}");
            if state["status"] == "succeeded" {
                assert_eq!(state["execution"]["revision"], 2);
                assert!(!state["artifacts"].as_array().unwrap().is_empty());
                assert!(state["steps"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| s["status"] == "succeeded"));
                break;
            }
        }
        let (_, state) = request(
            &app,
            "GET",
            &format!("/v1/factory/workflow-runs/{}", r["id"].as_str().unwrap()),
            Some(&cookie),
            None,
            None,
        )
        .await;
        assert_eq!(state["status"], "succeeded", "{state}");
        assert_eq!(
            std::fs::read_to_string(source.join("code.txt")).unwrap(),
            "personal changes"
        );
    }
    server.abort();
}

fn app(role: &str) -> axum::Router {
    let c = connection::connect(":memory:").unwrap();
    migrations::run(&c).unwrap();
    let (org, user, _) =
        queries::bootstrap(&c, "Test Org", "test-org", "owner@test.com", "Owner").unwrap();
    if role == "fleet_only" {
        queries::create_role(
            &c,
            &org.id,
            "fleet_only",
            "Fleet only",
            &[
                "autonomous_agent:read".into(),
                "autonomous_agent:enable".into(),
            ],
            None,
        )
        .unwrap();
    }
    queries::set_user_password(&c, &user.id, &hash_password("testpass1").unwrap()).unwrap();
    c.execute("UPDATE users SET role=?1 WHERE id=?2", [role, &user.id])
        .unwrap();
    router::build(
        c,
        Config {
            port: 8080,
            db_path: ":memory:".into(),
            log_level: "error".into(),
            cors_origins: "*".into(),
            superuser_key: None,
            smtp_host: "localhost".into(),
            smtp_port: 587,
            smtp_username: None,
            smtp_password: None,
            smtp_from: None,
            app_base_url: "http://localhost:5173".into(),
            admin_origin: "http://localhost:3000".into(),
            cookie_secure: false,
            github_client_id: None,
            github_client_secret: None,
            github_redirect_uri: None,
            backup_database_url: None,
            backup_interval_hours: 6,
            autonomous_agents_enabled: false,
            claude_code_bin: "/usr/bin/claude".into(),
            nexus_worker_bin: "/app/nexus".into(),
            claude_code_probe_interval_seconds: 300,
            autonomous_agent_poll_seconds: 15,
        },
    )
}
async fn request(
    app: &axum::Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header("Cookie", cookie);
    }
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn login(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"email":"owner@test.com","password":"testpass1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}
#[tokio::test]
async fn human_roles_do_not_bypass_explicit_permissions() {
    let app = app("admin");
    let cookie = login(&app).await;
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/factory/workflow-catalog",
            Some(&cookie),
            None,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/factory/runners",
            Some(&cookie),
            None,
            Some(json!({"name":"local"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}
#[tokio::test]
async fn runner_token_cannot_edit_control_plane_and_can_be_revoked() {
    let app = app("super_user");
    let cookie = login(&app).await;
    let (status, runner) = request(
        &app,
        "POST",
        "/v1/factory/runners",
        Some(&cookie),
        None,
        Some(json!({"name":"local"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = runner["token"].as_str().unwrap();
    let id = runner["id"].as_str().unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/factory/runner/workspaces",
            None,
            Some(token),
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/factory/runner/workspaces",
            Some(&cookie),
            None,
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/factory/runner/workspaces",
            None,
            Some("invalid"),
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/v1/factory/workspaces",
            None,
            Some(token),
            Some(json!({"name":"x","runner_id":id,"root_path":"/work"}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, workspace) = request(
        &app,
        "POST",
        "/v1/factory/workspaces",
        Some(&cookie),
        None,
        Some(json!({"name":"Kasymir","runner_id":id,"root_path":"/work/kasymir"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, inventory) = request(
        &app,
        "GET",
        "/v1/factory/runner/workspaces",
        None,
        Some(token),
        None,
    )
    .await;
    assert_eq!(inventory["workspaces"][0]["id"], workspace["id"]);
    let (_, catalog) = request(
        &app,
        "GET",
        "/v1/factory/workflow-catalog",
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert!(!catalog.to_string().contains(token));
    assert_eq!(
        request(
            &app,
            "DELETE",
            &format!("/v1/factory/runners/{id}"),
            Some(&cookie),
            None,
            None
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/factory/runner/workspaces",
            None,
            Some(token),
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}
