use crate::{
    decision, execute_turn, jev, new_session, open_shell, openshell, redacted, run_acceptance,
    run_verification, save_session, DecisionEngine, NextAction, RuntimeEvent, RuntimeRegistry,
    Session,
};
use anyhow::{bail, Result};
use chrono::Utc;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Terminal,
};
use std::{
    collections::VecDeque,
    io::{self, IsTerminal},
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

enum Job {
    Turn(String, bool),
    Verify,
    Command(String),
}
type JobResult = (Option<Session>, Result<()>);

struct App {
    root: PathBuf,
    runtime: String,
    engine: DecisionEngine,
    session: Option<Session>,
    conversation: VecDeque<String>,
    activity: VecDeque<String>,
    input: String,
    scroll: u16,
    worker: Option<JoinHandle<JobResult>>,
    receiver: Option<Receiver<RuntimeEvent>>,
}

impl App {
    fn add_conversation(&mut self, text: impl Into<String>) {
        push_lines(&mut self.conversation, text.into());
    }
    fn add_activity(&mut self, text: impl Into<String>) {
        push_lines(&mut self.activity, text.into());
        self.scroll = 0;
    }
    fn start(&mut self, job: Job) {
        let (sender, receiver) = mpsc::channel();
        let root = self.root.clone();
        let runtime = self.runtime.clone();
        let engine = self.engine;
        let session = self.session.take();
        self.receiver = Some(receiver);
        self.worker = Some(thread::spawn(move || {
            run_job(root, runtime, engine, session, job, sender)
        }));
    }
    fn tick(&mut self) {
        if let Some(receiver) = &self.receiver {
            let events: Vec<_> = receiver.try_iter().collect();
            for event in events {
                match event {
                    RuntimeEvent::Activity(text) => self.add_activity(text),
                    RuntimeEvent::Assistant(text) => {
                        self.add_conversation(format!("Agente: {text}"))
                    }
                }
            }
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            let outcome = self.worker.take().unwrap().join();
            self.receiver = None;
            match outcome {
                Ok((session, result)) => {
                    self.session = session;
                    match result {
                        Ok(()) => {
                            if let Some(item) = &self.session {
                                self.add_activity(format!(
                                    "Turno terminado · estado: {} · sesión: {}",
                                    item.status, item.id
                                ));
                            } else {
                                self.add_activity("Comando terminado.");
                            }
                        }
                        Err(error) => {
                            self.add_activity(format!("Error: {:#}", redacted(&error.to_string())))
                        }
                    }
                }
                Err(_) => self.add_activity("Error: el proceso del turno terminó inesperadamente."),
            }
        }
    }
    fn submit(&mut self, line: String) -> Result<bool> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(false);
        }
        if self.worker.is_some() {
            self.add_activity("Espera a que termine el turno actual.");
            return Ok(false);
        }
        if matches!(line, "/exit" | "/quit" | "exit" | "quit") {
            return Ok(true);
        }
        if line == "/help" {
            self.add_activity("Mensaje = turno. /plan <tarea>, /login, /verify, /status, /logs, /runtime <id>, /finish, /shell, !<comando>, /exit. ↑↓ desplaza actividad.");
            return Ok(false);
        }
        if line == "/status" {
            let status = self
                .session
                .as_ref()
                .map(|s| {
                    format!(
                        "{} · {} · {} turnos · {}",
                        s.id, s.status, s.state.attempts, s.runtime
                    )
                })
                .unwrap_or_else(|| "Sin sesión activa".into());
            self.add_activity(status);
            return Ok(false);
        }
        if line == "/logs" {
            if let Some(session) = &self.session {
                let entries = session.transcript.clone();
                for entry in entries {
                    self.add_conversation(entry);
                }
            } else {
                self.add_activity("Sin sesión activa.");
            }
            return Ok(false);
        }
        if let Some(selected) = line.strip_prefix("/runtime ") {
            let selected = selected.trim();
            RuntimeRegistry::with_defaults().resolve(selected)?;
            self.runtime = selected.into();
            if let Some(session) = &mut self.session {
                session.runtime = selected.into();
                session.provider_session_id = None;
                session.updated_at = Utc::now();
                save_session(&self.root, session)?;
            }
            self.add_activity(format!("Runtime: {selected}"));
            return Ok(false);
        }
        if line == "/finish" {
            if let Some(session) = &mut self.session {
                if session.state.verification.tests_failed > 0 {
                    self.add_activity(
                        "La verificación falla; revisa la actividad antes de aceptar.",
                    );
                } else {
                    let mut record =
                        decision::completion_judge(&session.state, Some(session.runtime.clone()));
                    record.decision_type = "human_accept".into();
                    record.selected = NextAction::Finish;
                    record.confidence = 1.0;
                    record.reason = "User accepted the result in Nexus TUI".into();
                    session.decisions.push(record);
                    session.status = "finish".into();
                    session.updated_at = Utc::now();
                    save_session(&self.root, session)?;
                    self.add_activity("Resultado aceptado.");
                }
            } else {
                self.add_activity("Sin sesión activa.");
            }
            return Ok(false);
        }
        if line == "/verify" {
            self.start(Job::Verify);
            return Ok(false);
        }
        if let Some(command) = line.strip_prefix('!') {
            if !command.trim().is_empty() {
                self.add_activity(format!("$ {}", redacted(command)));
                self.start(Job::Command(command.to_string()));
            }
            return Ok(false);
        }
        if line == "/shell" || line == "/openshell" || line == "/login" {
            return Ok(false); // The outer event loop suspends the TUI for a real PTY.
        }
        if let Some(task) = line.strip_prefix("/plan ") {
            self.add_conversation(format!("Tú (plan): {}", redacted(task)));
            self.start(Job::Turn(task.into(), true));
            return Ok(false);
        }
        if line.starts_with('/') {
            self.add_activity("Comando desconocido. Usa /help.");
            return Ok(false);
        }
        self.add_conversation(format!("Tú: {}", redacted(line)));
        self.start(Job::Turn(line.into(), false));
        Ok(false)
    }
}

fn push_lines(target: &mut VecDeque<String>, text: String) {
    for line in text.lines() {
        target.push_back(
            line.chars()
                .filter(|ch| !ch.is_control() || *ch == '\t')
                .take(500)
                .collect(),
        );
    }
    while target.len() > 600 {
        target.pop_front();
    }
}

fn run_job(
    root: PathBuf,
    runtime: String,
    engine: DecisionEngine,
    mut session: Option<Session>,
    job: Job,
    sender: Sender<RuntimeEvent>,
) -> JobResult {
    let result = match job {
        Job::Turn(task, planning) => {
            if planning {
                let mut plan = new_session(&root, runtime, task.clone());
                let result = execute_turn(&root, &mut plan, &task, true, engine, Some(sender));
                let _ = save_session(&root, &plan);
                result
            } else {
                let item = session.get_or_insert_with(|| new_session(&root, runtime, task.clone()));
                execute_turn(&root, item, &task, false, engine, Some(sender))
            }
        }
        Job::Verify => {
            if let Some(item) = &mut session {
                let result = (|| -> Result<()> {
                    let sandbox = openshell::OpenShell::ensure(&root, &item.runtime)?;
                    sandbox.upload(&root)?;
                    let command = crate::verification_command(&root).unwrap_or_default();
                    let _ = sender.send(RuntimeEvent::Activity(format!("$ {command}")));
                    run_verification(&root, &mut item.state, &sandbox)?;
                    let _ = sender.send(RuntimeEvent::Activity(
                        item.state.verification_output.clone(),
                    ));
                    run_acceptance(&mut item.state, &sandbox)?;
                    crate::judge(item, engine)?;
                    item.updated_at = Utc::now();
                    save_session(&root, item)
                })();
                result
            } else {
                Err(anyhow::anyhow!("Sin sesión activa."))
            }
        }
        Job::Command(command) => (|| -> Result<()> {
            let sandbox = openshell::OpenShell::ensure(&root, "shell")?;
            let snapshot = openshell::HostSnapshot::capture(&root)?;
            sandbox.upload(&root)?;
            let output = sandbox.command_output(&["sh", "-lc", &command], Some(120))?;
            sandbox.download(&root, &snapshot)?;
            let text = format!(
                "{}\n{}\nexit: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
                output.status
            );
            let _ = sender.send(RuntimeEvent::Activity(redacted(&text)));
            Ok(())
        })(),
    };
    (session, result)
}

pub fn run(
    root: PathBuf,
    runtime: String,
    engine: DecisionEngine,
    session: Option<Session>,
    first_task: Option<String>,
) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("La TUI necesita un terminal interactivo; usa `nexus chat --plain` para entrada estándar.");
    }
    let mut app = App {
        root,
        runtime,
        engine,
        session,
        conversation: VecDeque::new(),
        activity: VecDeque::new(),
        input: String::new(),
        scroll: 0,
        worker: None,
        receiver: None,
    };
    app.add_activity("Nexus TUI · OpenShell obligatorio · /help para comandos");
    app.add_activity(format!(
        "Decisión: {}",
        if jev::configured() {
            "JEV"
        } else {
            "local (JEV opcional)"
        }
    ));
    if app.runtime == "claude-code-headless" {
        app.add_activity("Claude: si aún no has iniciado sesión en OpenShell, escribe /login.");
    }
    if let Some(task) = first_task {
        app.submit(task)?;
    }
    let mut screen = Screen::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    loop {
        app.tick();
        terminal.draw(|frame| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(6),
                    Constraint::Length(3),
                ])
                .split(frame.area());
            let session = app
                .session
                .as_ref()
                .map(|s| s.id.chars().take(8).collect::<String>())
                .unwrap_or_else(|| "nueva".into());
            let state = if app.worker.is_some() {
                "trabajando"
            } else {
                "lista"
            };
            let header = format!(
                " Nexus │ {} │ {} │ {} │ OpenShell │ JEV {} ",
                app.session
                    .as_ref()
                    .map(|s| s.runtime.as_str())
                    .unwrap_or(&app.runtime),
                session,
                state,
                if jev::configured() { "✓" } else { "opcional" }
            );
            frame.render_widget(
                Paragraph::new(header)
                    .style(Style::default().fg(Color::Cyan))
                    .block(Block::default().borders(Borders::ALL)),
                areas[0],
            );
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(areas[1]);
            let chat = app
                .conversation
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let activity = app.activity.iter().cloned().collect::<Vec<_>>().join("\n");
            let chat_height = columns[0].height.saturating_sub(2);
            let activity_height = columns[1].height.saturating_sub(2);
            let chat_scroll = chat.lines().count().saturating_sub(chat_height as usize) as u16;
            let activity_scroll = activity
                .lines()
                .count()
                .saturating_sub(activity_height as usize) as u16;
            frame.render_widget(
                Paragraph::new(chat)
                    .wrap(Wrap { trim: false })
                    .scroll((chat_scroll, 0))
                    .block(
                        Block::default()
                            .title(" Conversación ")
                            .borders(Borders::ALL),
                    ),
                columns[0],
            );
            frame.render_widget(
                Paragraph::new(activity)
                    .wrap(Wrap { trim: false })
                    .scroll((activity_scroll.saturating_sub(app.scroll), 0))
                    .block(
                        Block::default()
                            .title(" Actividad · comandos y salidas ")
                            .borders(Borders::ALL),
                    ),
                columns[1],
            );
            let prompt = format!("> {}", app.input);
            frame.render_widget(
                Paragraph::new(prompt)
                    .style(Style::default().add_modifier(Modifier::BOLD))
                    .block(
                        Block::default()
                            .title(" Escribe y pulsa Enter · /help ")
                            .borders(Borders::ALL),
                    ),
                areas[2],
            );
            frame.set_cursor_position((
                areas[2].x + 3 + app.input.chars().count() as u16,
                areas[2].y + 1,
            ));
        })?;
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) if app.worker.is_none() => break,
                    (KeyCode::Enter | KeyCode::Char('\n') | KeyCode::Char('\r'), _) => {
                        let line = std::mem::take(&mut app.input);
                        if matches!(line.trim(), "/shell" | "/openshell" | "/login")
                            && app.worker.is_none()
                        {
                            drop(terminal);
                            drop(screen);
                            let result = if line.trim() == "/login" {
                                login(
                                    &app.root,
                                    app.session
                                        .as_ref()
                                        .map(|s| s.runtime.as_str())
                                        .unwrap_or(&app.runtime),
                                )
                            } else {
                                open_shell(&app.root)
                            };
                            screen = Screen::enter()?;
                            terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                            match result {
                                Ok(status) => {
                                    app.add_activity(format!("Terminal terminó: {status}"))
                                }
                                Err(error) => app.add_activity(format!("Terminal: {error:#}")),
                            }
                        } else {
                            match app.submit(line) {
                                Ok(true) => break,
                                Ok(false) => {}
                                Err(error) => app.add_activity(format!("Error: {error:#}")),
                            }
                        }
                    }
                    (KeyCode::Backspace, _) => {
                        app.input.pop();
                    }
                    (KeyCode::Char(ch), _) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.input.push(ch)
                    }
                    (KeyCode::Up, _) => app.scroll = app.scroll.saturating_add(1),
                    (KeyCode::Down, _) => app.scroll = app.scroll.saturating_sub(1),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn login(root: &std::path::Path, runtime: &str) -> Result<std::process::ExitStatus> {
    if runtime == "codex-headless" {
        return std::process::Command::new("codex")
            .arg("login")
            .status()
            .map_err(Into::into);
    }
    if runtime != "claude-code-headless" {
        bail!("/login solo corresponde a los ejecutables Claude Code y Codex");
    }
    let sandbox = openshell::OpenShell::ensure(root, runtime)?;
    sandbox.upload(root)?;
    sandbox
        .exec_command(&["claude", "auth", "login", "--claudeai"], true, None)?
        .status()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_control_characters_from_agent_activity() {
        let mut lines = VecDeque::new();
        push_lines(&mut lines, "ok\u{1b}[31m\u{7} done".into());
        assert_eq!(lines.front().map(String::as_str), Some("ok[31m done"));
    }
}
