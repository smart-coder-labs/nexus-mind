mod api_runtime;
mod codex_runtime;
mod decision;
mod jev;
mod model;
mod nexusmind;
mod openshell;
mod runtime;
mod storage;
mod tui;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use model::{NextAction, Session, TaskExecutionState, Verification};
use runtime::{RuntimeEvent, RuntimeRegistry, RuntimeRequest};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc::Sender,
};
use storage::{list_sessions, load_session, save_session};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum DecisionEngine {
    Auto,
    Jev,
    Local,
}

#[derive(Parser)]
#[command(
    name = "nexus",
    version,
    about = "Interactive, evidence-driven coding harness"
)]
struct Cli {
    #[arg(long, global = true)]
    repository: Option<PathBuf>,
    #[arg(long, global = true, default_value = "claude-code-headless")]
    runtime: String,
    #[arg(long, global = true, value_enum, default_value = "auto")]
    decision_engine: DecisionEngine,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Prepare local session storage; optionally select a NexusMind project.
    Init {
        #[arg(long)]
        project: Option<String>,
        /// NexusMind project ID for .nexusmind.yaml (defaults to --project).
        #[arg(long)]
        project_id: Option<String>,
    },
    /// Execute one coding turn and verify it.
    Run { objective: Vec<String> },
    /// Plan in the runtime's read-only permission mode.
    Plan { objective: Vec<String> },
    /// Start the interactive Nexus shell.
    Chat {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        plain: bool,
    },
    /// Start a Claude Code session in the Nexus TUI.
    Claude { objective: Vec<String> },
    /// Start a Codex session in the Nexus TUI.
    Codex { objective: Vec<String> },
    /// Open a real terminal shell in the mandatory OpenShell sandbox.
    Shell,
    /// Machine-facing Claude turn for the NexusMind autonomous worker.
    #[command(hide = true)]
    WorkerExec {
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=400))]
        max_turns: u32,
        #[arg(long)]
        permission_mode: String,
        #[arg(long)]
        allowed_tools: String,
        #[arg(long)]
        mcp_config: Option<PathBuf>,
        #[arg(long)]
        context_dir: Option<PathBuf>,
    },
    /// Check Claude or Codex account login inside OpenShell without running a task.
    Auth { runtime: String },
    /// Inspect runtime, decision engine, context connection, and sessions.
    Status,
    /// Resume a saved conversation interactively.
    Resume {
        session: String,
        #[arg(long)]
        plain: bool,
    },
    /// Run verification; update a session when its ID is supplied.
    Verify { session: Option<String> },
    /// Show the saved final responses and decision history.
    Logs { session: String },
}

fn repository(path: Option<PathBuf>) -> Result<PathBuf> {
    let path = path.unwrap_or(env::current_dir()?);
    let path = path
        .canonicalize()
        .context("repository path does not exist")?;
    if !path.is_dir() {
        bail!("repository must be a directory");
    }
    Ok(path)
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .context("could not run git")?;
    if !output.status.success() {
        bail!("directory is not a Git repository or git failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string())
}

fn changed_files(root: &Path) -> Result<Vec<String>> {
    let mut changes = Vec::new();
    for (name, repository) in openshell::git_repositories(root)? {
        for line in git_output(&repository, &["status", "--porcelain"])?.lines() {
            if let Some(path) = line.get(3..).map(str::trim).filter(|path| !path.is_empty()) {
                changes.push(if name.is_empty() {
                    path.to_string()
                } else {
                    format!("{name}/{path}")
                });
            }
        }
    }
    Ok(changes)
}

fn change_summary(root: &Path) -> Result<String> {
    let mut summary = String::new();
    for (name, repository) in openshell::git_repositories(root)? {
        let stat = git_output(&repository, &["diff", "--stat"])?;
        if !stat.is_empty() {
            if !name.is_empty() {
                summary.push_str(&format!("{name}/\n"));
            }
            summary.push_str(&stat);
            summary.push('\n');
        }
    }
    Ok(summary)
}

fn redacted(value: &str) -> String {
    let mut text = value.to_string();
    for name in [
        "TYPESAFE_API_KEY",
        "JEV_API_KEY",
        "NEXUSMIND_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
    ] {
        if let Ok(secret) = env::var(name) {
            if secret.len() >= 4 {
                text = text.replace(&secret, "[REDACTED]");
            }
        }
    }
    text
}

fn new_session(root: &Path, runtime: String, objective: String) -> Session {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let state = TaskExecutionState {
        task_id: id.clone(),
        objective,
        repository: root.display().to_string(),
        requirements_total: 1,
        requirements_covered: 0,
        unresolved_requirements: vec!["no task-specific acceptance evidence".into()],
        changed_files: vec![],
        changed_symbols: vec![],
        impact_radius: 0,
        affected_processes: vec![],
        affected_contracts: vec![],
        verification: Verification::default(),
        unresolved_evidence: vec!["verification not run".into()],
        attempts: 0,
        tool_errors: 0,
        tokens_used: 0,
        estimated_cost_usd: 0.0,
        agent_summary: String::new(),
        change_summary: String::new(),
        verification_output: String::new(),
        context_sources: vec![],
    };
    Session {
        id,
        created_at: now,
        updated_at: now,
        runtime,
        state,
        decisions: vec![],
        transcript: vec![],
        status: "ready".into(),
        provider_session_id: None,
    }
}

fn verification_command(root: &Path) -> Option<String> {
    if let Ok(command) = env::var("NEXUS_VERIFY_COMMAND") {
        if !command.trim().is_empty() {
            return Some(command);
        }
    }
    if root.join("Cargo.toml").exists() {
        Some("cargo test".into())
    } else if root.join("apps/backend/Cargo.toml").exists() {
        // The sandbox deliberately has a fresh Git baseline, not the host's
        // history. One backend test asserts properties of that real history.
        Some("cargo test --manifest-path apps/backend/Cargo.toml -j 1 -- --skip migration::git_history::tests::scanning_this_repository_filters_most_of_its_history".into())
    } else if root.join("package.json").exists() {
        Some("npm test".into())
    } else {
        None
    }
}

fn run_verification(
    root: &Path,
    state: &mut TaskExecutionState,
    sandbox: &openshell::OpenShell,
) -> Result<()> {
    let command =
        verification_command(root).context("no verification command; set NEXUS_VERIFY_COMMAND")?;
    // Completion decisions use the latest verification result, not a lifetime
    // count that would keep a fixed task permanently in the failed state.
    state.verification.tests_passed = 0;
    state.verification.tests_failed = 0;
    state.verification.commands.clear();
    state.unresolved_evidence.retain(|item| {
        item != "verification failed"
            && item != "verification not run"
            && item != "no verification command configured"
    });
    let output = sandbox
        .command_output(&["sh", "-lc", &command], None)
        .context("could not start verification in OpenShell")?;
    state.verification.commands.push(command);
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let tail = combined
        .chars()
        .rev()
        .take(8_000)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    state.verification_output = redacted(&tail);
    if output.status.success() {
        state.verification.tests_passed += 1;
    } else {
        state.verification.tests_failed += 1;
        state.unresolved_evidence = vec!["verification failed".into()];
    }
    Ok(())
}

fn run_acceptance(state: &mut TaskExecutionState, sandbox: &openshell::OpenShell) -> Result<()> {
    let Ok(command) = env::var("NEXUS_ACCEPTANCE_COMMAND") else {
        return Ok(());
    };
    if command.trim().is_empty() {
        return Ok(());
    }
    let output = sandbox
        .command_output(&["sh", "-lc", &command], None)
        .context("could not start task-specific acceptance check in OpenShell")?;
    state.verification.commands.push(command);
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    state
        .verification_output
        .push_str(&redacted(&combined.chars().take(4_000).collect::<String>()));
    if output.status.success() {
        state.verification.tests_passed += 1;
        state.requirements_covered = state.requirements_total;
        state.unresolved_requirements.clear();
        state.unresolved_evidence.retain(|item| {
            item != "verification not run" && item != "no verification command configured"
        });
    } else {
        state.verification.tests_failed += 1;
        state.unresolved_requirements = vec!["task-specific acceptance check failed".into()];
    }
    Ok(())
}

fn judge(session: &mut Session, engine: DecisionEngine) -> Result<()> {
    let runtime = Some(session.runtime.clone());
    let mut decision = match engine {
        DecisionEngine::Jev => jev::completion_judge(&session.state, runtime)?,
        DecisionEngine::Auto if jev::configured() => {
            jev::completion_judge(&session.state, runtime)?
        }
        DecisionEngine::Auto | DecisionEngine::Local => {
            let mut record = decision::completion_judge(&session.state, runtime);
            record.decision_type = "task_completion_local".into();
            record.reason = format!("Local rules: {}", record.reason);
            record
        }
    };
    // Provider output is an observation, not completion evidence. A failed
    // runtime cannot finish even if a remote judge returns that choice.
    if session.status == "failed" && decision.selected == NextAction::Finish {
        decision.selected = NextAction::Retry;
        decision.reason.push_str("; runtime failed");
    }
    session.status = decision.selected.as_str().to_string();
    session.decisions.push(decision);
    Ok(())
}

fn execute_turn(
    root: &Path,
    session: &mut Session,
    input: &str,
    planning_only: bool,
    engine: DecisionEngine,
    events: Option<Sender<RuntimeEvent>>,
) -> Result<()> {
    if matches!(engine, DecisionEngine::Jev) && !jev::configured() {
        bail!("Jev requires TYPESAFE_API_KEY (or JEV_API_KEY); export it before starting a coding turn");
    }
    let registry = RuntimeRegistry::with_defaults();
    let runtime = registry.resolve(&session.runtime)?;
    match session.runtime.as_str() {
        "claude-api" if env::var_os("ANTHROPIC_API_KEY").is_none() => {
            bail!("claude-api requires ANTHROPIC_API_KEY; use `nexus claude` for Claude Code login")
        }
        "openai-api" if env::var_os("OPENAI_API_KEY").is_none() => {
            bail!("openai-api requires OPENAI_API_KEY; use `nexus codex` for the existing Codex login")
        }
        _ => {}
    }
    runtime::emit(
        &events,
        RuntimeEvent::Activity("Preparando OpenShell…".into()),
    );
    let sandbox = openshell::OpenShell::ensure(root, &session.runtime)?;
    let host_snapshot = openshell::HostSnapshot::capture(root)?;
    runtime::emit(
        &events,
        RuntimeEvent::Activity("Sincronizando repositorio…".into()),
    );
    sandbox.upload(root)?;
    if session.runtime == "codex-headless" {
        runtime::emit(
            &events,
            RuntimeEvent::Activity("Preparando sesión Codex de ChatGPT en OpenShell…".into()),
        );
        sandbox.prepare_codex_auth()?;
    }
    let before = changed_files(root)?;
    session.status = "running".into();
    session.updated_at = Utc::now();
    session.state.attempts += 1;
    save_session(root, session)?;

    let mut prompt = String::new();
    if session.provider_session_id.is_none() {
        match nexusmind::retrieve(root, input) {
            Ok(Some(context)) => {
                session.state.context_sources.extend(context.sources);
                prompt.push_str(
                    "NexusMind context (verify against the current files before relying on it):\n",
                );
                prompt.push_str(&context.text);
                prompt.push('\n');
            }
            Ok(None) => {}
            Err(error) => {
                if env::var("NEXUSMIND_REQUIRED").as_deref() == Ok("1") {
                    return Err(error);
                }
                runtime::emit(
                    &events,
                    RuntimeEvent::Activity(format!("NexusMind context unavailable: {error:#}")),
                );
            }
        }
    }
    if planning_only {
        prompt.push_str("Plan the request. Do not edit files. Include expected changes, risks, and verification.\n");
    } else {
        prompt.push_str("Work on the request in this repository. Explain the result and the evidence from verification.\n");
    }
    if openshell::is_multi_repository_workspace(root)? {
        prompt.push_str("This workspace contains multiple child Git repositories. Inspect and edit files only inside those child repository directories. Loose files at the parent root are not synchronized to or from OpenShell.\n");
    }
    if session.state.attempts > 1 && session.provider_session_id.is_none() {
        if let Some(last) = session.transcript.last() {
            prompt.push_str("Previous turn summary:\n");
            prompt.push_str(&last.chars().take(2_000).collect::<String>());
            prompt.push('\n');
        }
    }
    prompt.push_str("User request: ");
    prompt.push_str(input);
    let request = RuntimeRequest {
        prompt,
        repository: root.display().to_string(),
        read_only: planning_only,
        provider_session_id: session.provider_session_id.clone(),
        openshell: Some(sandbox.clone()),
        events: events.clone(),
    };
    runtime::emit(
        &events,
        RuntimeEvent::Activity(format!("Iniciando {}…", session.runtime)),
    );
    let response = match runtime.run(&request) {
        Ok(response) => response,
        Err(error) => {
            if session.runtime == "codex-headless" {
                sandbox.clear_codex_auth();
            }
            session.state.tool_errors += 1;
            session.status = "failed".into();
            session.updated_at = Utc::now();
            if !planning_only {
                if let Err(sync_error) = sandbox.download(root, &host_snapshot) {
                    runtime::emit(
                        &events,
                        RuntimeEvent::Activity(format!(
                            "OpenShell recovery sync needs review: {sync_error:#}"
                        )),
                    );
                    session.status = "human_review".into();
                }
            }
            save_session(root, session)?;
            return Err(error);
        }
    };
    if session.runtime == "codex-headless" {
        sandbox.clear_codex_auth();
    }
    if session.runtime.ends_with("-api") && !response.output.trim().is_empty() {
        runtime::emit(&events, RuntimeEvent::Assistant(redacted(&response.output)));
    }
    if !planning_only {
        runtime::emit(
            &events,
            RuntimeEvent::Activity("Recuperando cambios de OpenShell…".into()),
        );
        if let Err(error) = sandbox.download(root, &host_snapshot) {
            session.status = "human_review".into();
            session.updated_at = Utc::now();
            save_session(root, session)?;
            return Err(error.context("OpenShell changes were not safely synchronized"));
        }
    }
    session.provider_session_id = response.provider_session_id;
    session.state.tokens_used +=
        response.usage.input_tokens.unwrap_or(0) + response.usage.output_tokens.unwrap_or(0);
    session.state.estimated_cost_usd += response.usage.cost_usd.unwrap_or(0.0);
    session.state.agent_summary =
        redacted(&response.output.chars().take(8_000).collect::<String>());
    session.transcript.push(redacted(&format!(
        "user: {input}\nassistant: {}",
        session.state.agent_summary
    )));
    session.state.changed_files = changed_files(root)?;
    session.state.change_summary = change_summary(root).unwrap_or_default();
    if !before.is_empty() && before == session.state.changed_files {
        session
            .state
            .unresolved_evidence
            .push("repository already had these changes before the turn".into());
    }
    if !response.success {
        session.state.tool_errors += 1;
        session.status = "failed".into();
        session.updated_at = Utc::now();
        save_session(root, session)?;
        bail!(
            "runtime reported an unsuccessful turn; inspect `nexus logs {}`",
            session.id
        );
    }
    if planning_only {
        session.status = "planned".into();
        session.updated_at = Utc::now();
        save_session(root, session)?;
        return Ok(());
    }
    if verification_command(root).is_some() {
        if let Some(command) = verification_command(root) {
            runtime::emit(&events, RuntimeEvent::Activity(format!("$ {command}")));
        }
        run_verification(root, &mut session.state, &sandbox)?;
        runtime::emit(
            &events,
            RuntimeEvent::Activity(format!(
                "verification: {}\n{}",
                if session.state.verification.tests_failed == 0 {
                    "passed"
                } else {
                    "failed"
                },
                session.state.verification_output
            )),
        );
    } else {
        session.state.unresolved_evidence = vec!["no verification command configured".into()];
    }
    // A generic suite passing does not prove the user's objective. Only a
    // task-specific acceptance command can mark the requirement covered.
    if session.state.verification.tests_failed == 0 {
        run_acceptance(&mut session.state, &sandbox)?;
    }
    session.updated_at = Utc::now();
    save_session(root, session)?;
    runtime::emit(&events, RuntimeEvent::Activity("Consultando JEV…".into()));
    if let Err(error) = judge(session, engine) {
        session.status = "human_review".into();
        save_session(root, session)?;
        return Err(error.context("completion judge unavailable; session saved for review"));
    }
    save_session(root, session)
}

fn print_session(session: &Session) {
    println!("session: {}", session.id);
    println!(
        "status: {} | runtime: {} | turns: {}",
        session.status, session.runtime, session.state.attempts
    );
    println!(
        "changed files: {} | verification passed: {} failed: {}",
        session.state.changed_files.len(),
        session.state.verification.tests_passed,
        session.state.verification.tests_failed
    );
    if let Some(decision) = session.decisions.last() {
        println!(
            "decision: {:?} ({:.0}%) via {} — {}",
            decision.selected,
            decision.confidence * 100.0,
            decision.decision_type,
            decision.reason
        );
    }
}

fn open_shell(root: &Path) -> Result<ExitStatus> {
    let runner = openshell::OpenShell::ensure(root, "shell")?;
    let host_snapshot = openshell::HostSnapshot::capture(root)?;
    runner.upload(root)?;
    println!(
        "OpenShell sandbox `{}` · {}",
        runner.sandbox(),
        runner.workspace()
    );
    let status = runner.interactive_shell()?;
    runner.download(root, &host_snapshot)?;
    Ok(status)
}

fn worker_exec(
    root: &Path,
    max_turns: u32,
    permission_mode: &str,
    allowed_tools: &str,
    mcp_config: Option<&Path>,
    context_dir: Option<&Path>,
) -> Result<()> {
    if env::var("OPENSHELL_GATEWAY").ok().filter(|value| !value.trim().is_empty()).is_none() {
        bail!("OpenShell gateway is not configured: set OPENSHELL_GATEWAY to a registered gateway name");
    }
    if env::var("NEXUS_OPENSHELL_IMAGE").ok().filter(|value| !value.trim().is_empty()).is_none() {
        bail!("OpenShell worker image is not configured: set NEXUS_OPENSHELL_IMAGE");
    }
    if !matches!(permission_mode, "plan" | "default" | "acceptEdits") {
        bail!("unsupported worker permission mode");
    }
    if allowed_tools.trim().is_empty() || allowed_tools.len() > 4096 {
        bail!("worker tool allowlist is empty or too long");
    }
    let mut prompt = String::new();
    io::stdin().take(512 * 1024 + 1).read_to_string(&mut prompt)?;
    if prompt.is_empty() || prompt.len() > 512 * 1024 {
        bail!("worker prompt must contain 1–524288 bytes");
    }
    let sandbox = openshell::OpenShell::ensure(root, "claude-code-headless")?;
    sandbox.auth_status("claude-code-headless")?;
    let snapshot = openshell::HostSnapshot::capture(root)?;
    sandbox.upload(root)?;
    let guest_config = mcp_config
        .map(|path| sandbox.upload_worker_mcp_config(path))
        .transpose()?;
    let guest_context = context_dir
        .map(|path| sandbox.upload_worker_context(path))
        .transpose()?;
    let max_turns = max_turns.to_string();
    let mut argv = vec![
        "claude", "-p", "--output-format", "stream-json", "--verbose",
        "--max-turns", &max_turns, "--permission-mode", permission_mode,
        "--allowedTools", allowed_tools,
    ];
    if let Some(path) = guest_config.as_deref() {
        argv.extend(["--mcp-config", path]);
    }
    if let Some(path) = guest_context.as_deref() {
        argv.extend(["--add-dir", path]);
    }
    let mut command = sandbox.exec_command(&argv, false, None)?;
    command
        .current_dir(root)
        .env("NEXUSMIND_MCP_TOOL_PROFILE", "essential")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("could not start Claude in OpenShell")?;
    let mut stdin = child.stdin.take().context("worker stdin was not piped")?;
    let stdin_writer = std::thread::spawn(move || stdin.write_all(prompt.as_bytes()));
    let stderr = child.stderr.take().context("worker stderr was not piped")?;
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stderr = stderr;
        let mut chunk = [0_u8; 8192];
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    let remaining = 64 * 1024_usize - buffer.len();
                    buffer.extend_from_slice(&chunk[..size.min(remaining)]);
                }
            }
        }
        buffer
    });
    let mut stdout = child.stdout.take().context("worker stdout was not piped")?;
    io::copy(&mut stdout, &mut io::stdout())?;
    let status = child.wait()?;
    let _ = stdin_writer.join();
    let stderr = stderr_reader.join().unwrap_or_default();
    if !stderr.is_empty() {
        io::stderr().write_all(&stderr)?;
    }
    sandbox
        .download(root, &snapshot)
        .context("worker changes were not safely synchronized from OpenShell")?;
    if !status.success() {
        bail!("Claude exited with {status} inside OpenShell");
    }
    Ok(())
}

fn interactive(
    root: &Path,
    runtime: String,
    engine: DecisionEngine,
    mut session: Option<Session>,
) -> Result<()> {
    let mut runtime = runtime;
    println!("Nexus shell · {} · /help for commands", root.display());
    loop {
        let name = session
            .as_ref()
            .map(|item| item.runtime.as_str())
            .unwrap_or(&runtime);
        print!("nexus[{name}]> ");
        io::stdout().flush()?;
        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                println!();
                continue;
            }
            Err(error) => return Err(error.into()),
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if matches!(line, "/exit" | "/quit" | "exit" | "quit") {
            break;
        }
        if line == "/help" {
            println!("Message = agent turn. /plan <task>, /login, /verify, /finish, /status, /runtime <id>, /shell, /logs, /exit; !<command> runs in OpenShell.");
            continue;
        }
        if line == "/status" {
            if let Some(item) = &session {
                print_session(item);
            } else {
                println!("No active session yet.");
            }
            continue;
        }
        if line == "/logs" {
            if let Some(item) = &session {
                for entry in &item.transcript {
                    println!("{entry}\n");
                }
            }
            continue;
        }
        if line == "/login" {
            let selected = session
                .as_ref()
                .map(|item| item.runtime.as_str())
                .unwrap_or(&runtime);
            match tui::login(root, selected) {
                Ok(status) => println!("login finished: {status}"),
                Err(error) => eprintln!("login: {error:#}"),
            }
            continue;
        }
        if line == "/shell" || line == "/openshell" {
            if let Err(error) = open_shell(root) {
                eprintln!("{error:#}");
            }
            continue;
        }
        if let Some(command) = line.strip_prefix('!') {
            if !command.trim().is_empty() {
                match openshell::OpenShell::ensure(root, "shell").and_then(|sandbox| {
                    let host_snapshot = openshell::HostSnapshot::capture(root)?;
                    sandbox.upload(root)?;
                    let output = sandbox.command_output(&["sh", "-lc", command], Some(120))?;
                    sandbox.download(root, &host_snapshot)?;
                    Ok(output)
                }) {
                    Ok(output) => {
                        print!("{}", String::from_utf8_lossy(&output.stdout));
                        eprint!("{}", String::from_utf8_lossy(&output.stderr));
                        println!("OpenShell exit: {}", output.status);
                    }
                    Err(error) => eprintln!("OpenShell: {error:#}"),
                }
            }
            continue;
        }
        if line == "/verify" {
            if let Some(item) = &mut session {
                match openshell::OpenShell::ensure(root, &item.runtime)
                    .and_then(|sandbox| {
                        sandbox.upload(root)?;
                        run_verification(root, &mut item.state, &sandbox)?;
                        run_acceptance(&mut item.state, &sandbox)
                    })
                    .and_then(|_| judge(item, engine))
                {
                    Ok(_) => {
                        item.updated_at = Utc::now();
                        save_session(root, item)?;
                        print_session(item);
                    }
                    Err(error) => eprintln!("verification: {error:#}"),
                }
            } else {
                eprintln!("No active session.");
            }
            continue;
        }
        if line == "/finish" {
            if let Some(item) = &mut session {
                if item.state.verification.tests_failed > 0 {
                    eprintln!("Verification is failing; fix it or inspect /logs before accepting.");
                } else {
                    let mut decision =
                        decision::completion_judge(&item.state, Some(item.runtime.clone()));
                    decision.decision_type = "human_accept".into();
                    decision.selected = NextAction::Finish;
                    decision.confidence = 1.0;
                    decision.reason =
                        "User accepted the observed result in the interactive shell".into();
                    item.decisions.push(decision);
                    item.status = "finish".into();
                    item.updated_at = Utc::now();
                    save_session(root, item)?;
                    print_session(item);
                }
            } else {
                eprintln!("No active session.");
            }
            continue;
        }
        if let Some(selected) = line.strip_prefix("/runtime ") {
            let selected = selected.trim();
            if RuntimeRegistry::with_defaults().resolve(selected).is_err() {
                eprintln!("Unknown runtime: {selected}");
                continue;
            }
            runtime = selected.into();
            if let Some(item) = &mut session {
                item.runtime = selected.into();
                item.provider_session_id = None;
                item.updated_at = Utc::now();
                save_session(root, item)?;
            } else {
                println!("runtime selected: {selected}");
            }
            continue;
        }
        if let Some(task) = line.strip_prefix("/plan ") {
            let plan_runtime = session
                .as_ref()
                .map(|item| item.runtime.clone())
                .unwrap_or_else(|| runtime.clone());
            let mut plan = new_session(root, plan_runtime, task.into());
            match execute_turn(root, &mut plan, task, true, engine, None) {
                Ok(_) => print_session(&plan),
                Err(error) => eprintln!("plan: {error:#}"),
            }
            continue;
        }
        if line.starts_with('/') {
            eprintln!("Unknown command. Use /help.");
            continue;
        }
        let item = session.get_or_insert_with(|| new_session(root, runtime.clone(), line.into()));
        match execute_turn(root, item, line, false, engine, None) {
            Ok(_) => print_session(item),
            Err(error) => eprintln!("turn: {error:#}"),
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = repository(cli.repository)?;
    if !matches!(
        &cli.command,
        Commands::Status | Commands::Logs { .. } | Commands::Auth { .. }
    ) {
        openshell::check_repository(&root)?;
    }
    match cli.command {
        Commands::Init {
            project,
            project_id,
        } => {
            let binary = openshell::OpenShell::ensure_installed()?;
            let dir = storage::prepare(&root)?;
            let selected = nexusmind::initialize_repository_config(
                &root,
                project.as_deref(),
                project_id.as_deref(),
            )?;
            if let Some(project) = selected {
                let config = serde_json::json!({"project": project});
                fs::write(dir.join("config.json"), serde_json::to_vec_pretty(&config)?)?;
            }
            println!("Nexus initialized in {}", dir.display());
            println!(
                "Repository configuration: {}",
                root.join(".nexusmind.yaml").display()
            );
            println!("OpenShell installed: {}", binary.display());
            println!("Use `nexus claude` or `nexus codex` with your CLI login. Jev and NexusMind are optional integrations.");
        }
        Commands::Run { objective } => {
            if objective.is_empty() {
                bail!("provide an objective");
            }
            let task = objective.join(" ");
            let mut session = new_session(&root, cli.runtime, task.clone());
            execute_turn(&root, &mut session, &task, false, cli.decision_engine, None)?;
            print_session(&session);
        }
        Commands::Plan { objective } => {
            if objective.is_empty() {
                bail!("provide an objective");
            }
            let task = objective.join(" ");
            let mut session = new_session(&root, cli.runtime, task.clone());
            execute_turn(&root, &mut session, &task, true, cli.decision_engine, None)?;
            print_session(&session);
        }
        Commands::Chat { session, plain } => {
            openshell::OpenShell::ensure_installed()?;
            let loaded = session.map(|id| load_session(&root, &id)).transpose()?;
            if plain {
                interactive(&root, cli.runtime, cli.decision_engine, loaded)?;
            } else {
                tui::run(root.clone(), cli.runtime, cli.decision_engine, loaded, None)?;
            }
        }
        Commands::Claude { objective } => {
            openshell::OpenShell::ensure(&root, "claude-code-headless")?;
            tui::run(
                root.clone(),
                "claude-code-headless".into(),
                cli.decision_engine,
                None,
                (!objective.is_empty()).then(|| objective.join(" ")),
            )?;
        }
        Commands::Codex { objective } => {
            openshell::OpenShell::ensure(&root, "codex-headless")?;
            tui::run(
                root.clone(),
                "codex-headless".into(),
                cli.decision_engine,
                None,
                (!objective.is_empty()).then(|| objective.join(" ")),
            )?;
        }
        Commands::Resume { session, plain } => {
            openshell::OpenShell::ensure_installed()?;
            let loaded = load_session(&root, &session)?;
            if plain {
                interactive(&root, cli.runtime, cli.decision_engine, Some(loaded))?;
            } else {
                tui::run(
                    root.clone(),
                    cli.runtime,
                    cli.decision_engine,
                    Some(loaded),
                    None,
                )?;
            }
        }
        Commands::Shell => {
            let status = open_shell(&root)?;
            if !status.success() {
                bail!("shell exited with {status}");
            }
        }
        Commands::WorkerExec {
            max_turns,
            permission_mode,
            allowed_tools,
            mcp_config,
            context_dir,
        } => worker_exec(
            &root,
            max_turns,
            &permission_mode,
            &allowed_tools,
            mcp_config.as_deref(),
            context_dir.as_deref(),
        )?,
        Commands::Auth { runtime } => {
            let selected = match runtime.as_str() {
                "claude" => "claude-code-headless",
                "codex" => "codex-headless",
                _ => bail!("use `nexus auth claude` or `nexus auth codex`"),
            };
            let sandbox = openshell::OpenShell::ensure(&root, selected)?;
            println!("{}", sandbox.auth_status(selected)?);
        }
        Commands::Verify { session } => {
            if let Some(id) = session {
                let mut item = load_session(&root, &id)?;
                let sandbox = openshell::OpenShell::ensure(&root, &item.runtime)?;
                sandbox.upload(&root)?;
                run_verification(&root, &mut item.state, &sandbox)?;
                run_acceptance(&mut item.state, &sandbox)?;
                judge(&mut item, cli.decision_engine)?;
                item.updated_at = Utc::now();
                save_session(&root, &item)?;
                print_session(&item);
            } else {
                let mut state =
                    new_session(&root, cli.runtime.clone(), "manual verification".into()).state;
                let sandbox = openshell::OpenShell::ensure(&root, &cli.runtime)?;
                sandbox.upload(&root)?;
                run_verification(&root, &mut state, &sandbox)?;
                println!(
                    "verification: {}",
                    if state.verification.tests_failed == 0 {
                        "passed"
                    } else {
                        "failed"
                    }
                );
                if state.verification.tests_failed > 0 {
                    bail!("verification failed:\n{}", state.verification_output.trim());
                }
            }
        }
        Commands::Status => {
            println!("repository: {}", root.display());
            println!("workspace: {}", openshell::workspace_description(&root)?);
            println!(
                "Jev: {} | NexusMind context: {}",
                if jev::configured() {
                    "configured"
                } else {
                    "optional; using local decisions"
                },
                if nexusmind::configured() {
                    "configured"
                } else {
                    "offline"
                }
            );
            println!(
                "OpenShell: {}",
                if openshell::OpenShell::installed() {
                    "installed (gateway and sandbox health not checked)"
                } else {
                    "will install on init/first use"
                }
            );
            for item in RuntimeRegistry::with_defaults().descriptors() {
                println!(
                    "runtime: {} ({}/{})",
                    item.id, item.provider, item.execution_mode
                );
            }
            for item in list_sessions(&root)? {
                println!("{}  {:<14}  {}", item.id, item.status, item.state.objective);
            }
        }
        Commands::Logs { session } => {
            let item = load_session(&root, &session)?;
            print_session(&item);
            for decision in item.decisions {
                println!(
                    "{}: {:?} — {}",
                    decision.decision_type, decision.selected, decision.reason
                );
            }
            for entry in item.transcript {
                println!("\n{entry}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod workspace_tests {
    use super::*;

    #[test]
    fn changed_files_in_multi_repo_parent_are_scoped_to_children() {
        let root = env::temp_dir().join(format!("nexus-workspace-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("loose.txt"), "outside").unwrap();
        for name in ["alpha", "beta"] {
            let child = root.join(name);
            fs::create_dir(&child).unwrap();
            assert!(Command::new("git")
                .args(["init", "-q"])
                .current_dir(&child)
                .status()
                .unwrap()
                .success());
            fs::write(child.join("change.txt"), name).unwrap();
        }
        assert_eq!(
            changed_files(&root).unwrap(),
            vec!["alpha/change.txt", "beta/change.txt"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_files_preserves_a_tracked_modification_path() {
        let root = env::temp_dir().join(format!("nexus-workspace-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        fs::write(root.join("tracked.txt"), "before").unwrap();
        assert!(Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        fs::write(root.join("tracked.txt"), "after").unwrap();
        assert_eq!(changed_files(&root).unwrap(), vec!["tracked.txt"]);
        fs::remove_dir_all(root).unwrap();
    }
}
