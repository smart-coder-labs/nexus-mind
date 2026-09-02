//! `nexusmind-migrate` — the interactive front-end for knowledge migrations.
//!
//! The migrations already work from the command line. What they lacked was a
//! way to see what a source contains *before* committing to it, to watch a long
//! run instead of guessing, and to review the queue without switching to a
//! browser. This is that.
//!
//! It drives `migrate-knowledge` rather than reimplementing it, so there is
//! exactly one place where a connector's rules live, and the equivalent command
//! is on screen at all times — nothing here is a capability the CLI lacks.

mod api;
mod app;
mod config;
mod mascot;
mod monorepo;
mod protocol;
mod runner;
mod ui;

use app::{App, Screen};
use api::Verdict;
use config::Source;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

/// How long a frame waits for input before redrawing anyway.
///
/// A run in flight has progress to show even when nobody touches the keyboard,
/// so the loop cannot block on input. 80ms is under the threshold where a
/// progress bar reads as stuttering, and costs nothing when idle.
const TICK: Duration = Duration::from_millis(80);

fn main() -> anyhow::Result<()> {
    // The detection child. Runs the terminal query and exits; see
    // `mascot::Graphics::detect` for why it cannot happen in this process.
    if std::env::args().any(|a| a == "--detect-graphics") {
        mascot::Graphics::run_detection_child();
    }

    if std::env::args().any(|a| a == "--mascot-doctor") {
        mascot::explain_and_exit();
    }

    // Asked before the alternate screen goes up. The query writes an escape
    // sequence and reads the terminal's reply; doing it afterwards would leave
    // the reply printed across the UI. A terminal with no answer is the
    // ordinary case and costs nothing — the quadrant renderer takes over.
    let graphics = mascot::Graphics::detect();
    // The child drove the terminal's termios directly. Put it back to a known
    // cooked state so `ratatui::init` starts where it would have anyway.
    let _ = crossterm::terminal::disable_raw_mode();

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, graphics);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    graphics: Option<mascot::Graphics>,
) -> anyhow::Result<()> {
    let mut app = App::new();
    app.graphics = graphics;
    if !app.binary().exists() {
        app.status = format!(
            "runner not found at {} — build it, or set NEXUSMIND_MIGRATE_BIN",
            app.binary().display()
        );
    }

    while !app.should_quit {
        app.pump();
        app.frame = app.frame.wrapping_add(1);
        terminal.draw(|f| ui::draw(f, &app))?;

        if !event::poll(TICK)? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            handle_key(&mut app, key.code, key.modifiers);
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // Text entry swallows almost everything: a path containing `r` must not
    // start a run. Only Esc and Enter get out.
    if app.editing {
        match code {
            KeyCode::Esc | KeyCode::Enter => {
                app.editing = false;
                app.edit_pristine = false;
            }
            // Clear the field outright, for when replace-on-first-keystroke is
            // not what you want and backspacing a long path is not either.
            KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                if let Some(f) = app.current_field() {
                    f.clear(&mut app.config);
                }
                app.edit_pristine = false;
            }
            KeyCode::Backspace => {
                // Editing an existing value on purpose: stop treating it as a
                // placeholder to be replaced.
                app.edit_pristine = false;
                if let Some(f) = app.current_field() {
                    f.pop_char(&mut app.config);
                }
            }
            KeyCode::Char(c) => {
                if let Some(f) = app.current_field() {
                    if app.edit_pristine {
                        f.clear(&mut app.config);
                        app.edit_pristine = false;
                    }
                    f.push_char(&mut app.config, c);
                }
            }
            _ => {}
        }
        return;
    }

    if app.show_help {
        app.show_help = false;
        return;
    }

    // While picking an existing project, Esc backs out of the picker rather than
    // quitting the whole TUI — the same key, read in context.
    if app.screen == Screen::Projects && app.selecting_for.is_some() {
        match code {
            KeyCode::Esc => {
                app.cancel_select();
                return;
            }
            KeyCode::Enter => {
                app.confirm_select();
                return;
            }
            _ => {}
        }
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => app.should_quit = true,
        KeyCode::Char('?') => app.show_help = true,

        KeyCode::Tab => next_screen(app, 1),
        KeyCode::BackTab => next_screen(app, -1),

        KeyCode::Up => vertical(app, -1),
        KeyCode::Down => vertical(app, 1),

        KeyCode::Enter => match app.screen {
            Screen::Source => app.goto(Screen::Options),
            Screen::Review if app.picking_run() => app.pick_run(),
            Screen::Review => app.toggle_selected(),
            Screen::Projects => app.cycle_action(),
            _ => {
                if let Some(f) = app.current_field() {
                    if f.kind() == app::FieldKind::Toggle {
                        f.toggle(&mut app.config);
                    } else {
                        app.editing = true;
                        app.edit_pristine = true;
                    }
                }
            }
        },
        KeyCode::Char(' ') => match app.screen {
            Screen::Review => app.toggle_selected(),
            Screen::Projects => app.cycle_action(),
            Screen::Source => {}
            _ => {
                if let Some(f) = app.current_field() {
                    f.toggle(&mut app.config);
                }
            }
        },

        // Cycles both → agents → logs. "Expand" and "switch" are the same
        // gesture here: an expanded panel takes the other's room.
        KeyCode::Char('e') => {
            app.activity = app.activity.next();
            app.follow_latest_agent();
        }
        KeyCode::Char('f') if app.screen == Screen::Running => app.follow_latest_agent(),
        KeyCode::Char('m') => app.toggle_mascot(),
        // On the plan screen, `d`/`r` confirm the plan — create the projects,
        // write the config, then launch the routed (dry or real) run — instead
        // of starting a single-project run that would ignore the plan.
        KeyCode::Char('d') if app.screen == Screen::Projects => app.execute_plan(true),
        KeyCode::Char('r') if app.screen == Screen::Projects => app.execute_plan(false),
        KeyCode::Char('s') if app.screen == Screen::Projects => app.begin_select(),
        KeyCode::Char('d') => app.start(true),
        KeyCode::Char('r') => app.start(false),
        KeyCode::Char('x') => app.stop(),
        KeyCode::Char('t') => app.probe(),
        KeyCode::Char('R') => {
            app.goto(Screen::Review);
            if app.picking_run() {
                // Explicit request for the full backend history, even when this
                // session's per-project runs are already on offer.
                app.load_runs();
            } else {
                app.load_candidates();
            }
        }
        KeyCode::Char('p') if app.screen == Screen::Review => app.unpick_run(),
        KeyCode::Char('X') if app.screen == Screen::Review && app.picking_run() => {
            app.cancel_run()
        }
        KeyCode::Char('a') if app.screen == Screen::Review => {
            app.decide(Verdict::Approved, false)
        }
        KeyCode::Char('j') if app.screen == Screen::Review => {
            app.decide(Verdict::Rejected, false)
        }
        KeyCode::Char('s') if app.screen == Screen::Review => {
            app.decide(Verdict::Restaged, false)
        }
        KeyCode::Char('A') if app.screen == Screen::Review => app.decide(Verdict::Approved, true),
        KeyCode::Char('C') if app.screen == Screen::Review => app.commit(),
        _ => {}
    }
}

fn vertical(app: &mut App, delta: isize) {
    match app.screen {
        Screen::Source => {
            let i = Source::ALL
                .iter()
                .position(|s| *s == app.config.source)
                .unwrap_or(0) as isize;
            let n = Source::ALL.len() as isize;
            app.config.source = Source::ALL[(i + delta).rem_euclid(n) as usize];
        }
        Screen::Review if app.picking_run() => {
            let n = app.run_list_len();
            if n > 0 {
                app.run_cursor = (app.run_cursor as isize + delta).rem_euclid(n as isize) as usize;
            }
        }
        Screen::Review => {
            let n = app.candidates.len();
            if n > 0 {
                app.review_cursor =
                    (app.review_cursor as isize + delta).rem_euclid(n as isize) as usize;
            }
        }
        // Walks the plan rows, or the existing-project picker when it is open.
        Screen::Projects => app.plan_move(delta),
        // On the run screen the arrows walk the exchanges — there are no
        // fields there, and inspecting what the model was asked is the reason
        // anyone stops on this screen.
        Screen::Running => app.move_agent_cursor(delta),
        _ => app.move_cursor(delta),
    }
}

fn next_screen(app: &mut App, delta: isize) {
    const ORDER: [Screen; 7] = [
        Screen::Connection,
        Screen::Source,
        Screen::Options,
        Screen::Projects,
        Screen::Running,
        Screen::Review,
        Screen::Summary,
    ];
    let i = ORDER.iter().position(|s| *s == app.screen).unwrap_or(0) as isize;
    let next = ORDER[(i + delta).rem_euclid(ORDER.len() as isize) as usize];
    app.goto(next);
    // Auto-open the queue only when a single run is unambiguously the target.
    // In a monorepo review there are several, and loading here would fetch the
    // backend history over this session's labelled per-project picker.
    if next == Screen::Review && app.candidates.is_empty() && app.active_run().is_some() {
        app.load_candidates();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bug this prevents: typing a path with an `r` in it silently starting
    /// a real migration run.
    #[test]
    fn keys_typed_into_a_field_never_trigger_commands() {
        let mut app = App::new();
        app.goto(Screen::Options);
        app.cursor = 0;
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        for c in "docs/adr".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(app.config.path, "docs/adr");
        assert!(app.handle.is_none(), "no run was started");
        assert!(!app.should_quit, "the `q` in a path must not quit");
    }

    /// The reported failure, as a test.
    ///
    /// `Path` is pre-filled with `.`. Typing an absolute path used to append to
    /// it, producing `./Users/cesar/…`, and the runner then reported that a
    /// perfectly good repository "is not a git repository". The first keystroke
    /// must replace the pre-filled value.
    #[test]
    fn typing_an_absolute_path_replaces_the_placeholder_instead_of_appending() {
        let mut app = App::new();
        app.goto(Screen::Options);
        app.cursor = 0;
        assert_eq!(app.config.path, ".", "the placeholder this test is about");

        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        for c in "/Users/cesar/repo".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(app.config.path, "/Users/cesar/repo");
        assert!(!app.config.path.starts_with("./"), "the exact reported bug");
    }

    /// Replacement must not fight someone deliberately editing a value: one
    /// backspace means "I am changing this", not "start over".
    #[test]
    fn backspacing_first_keeps_the_existing_value_and_appends() {
        let mut app = App::new();
        app.goto(Screen::Connection);
        app.config.api_url = "http://localhost:808".into();
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        for c in "80".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(app.config.api_url, "http://localhost:8080");
    }

    #[test]
    fn ctrl_u_clears_the_field() {
        let mut app = App::new();
        app.goto(Screen::Connection);
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(app.config.api_url, "");
        assert!(app.editing, "clearing does not leave the field");
    }

    #[test]
    fn escape_leaves_a_field_without_discarding_it() {
        let mut app = App::new();
        app.goto(Screen::Connection);
        app.editing = true;
        app.edit_pristine = false;
        handle_key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.editing);
        assert!(app.config.api_url.ends_with('x'));
    }

    #[test]
    fn backspace_edits_only_the_focused_field() {
        let mut app = App::new();
        app.goto(Screen::Connection);
        app.config.api_url = "http://x".into();
        app.editing = true;
        app.edit_pristine = false;
        handle_key(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.config.api_url, "http://");
        assert_eq!(app.config.api_key, RunConfigKey::default().0);
    }

    /// Keeps the assertion above honest without depending on the developer's
    /// environment, where NEXUSMIND_API_KEY may or may not be set.
    struct RunConfigKey(String);
    impl Default for RunConfigKey {
        fn default() -> Self {
            Self(crate::config::RunConfig::default().api_key)
        }
    }

    #[test]
    fn a_run_can_be_picked_from_the_list_and_released_again() {
        let mut app = App::new();
        app.runs = vec![crate::api::Run {
            id: "run-a".into(),
            source_kind: "repo_docs".into(),
            status: "completed".into(),
            source_ref: Some("./repo".into()),
            created_at: "2026-08-15T09:00:00Z".into(),
            client_id: Some("ba3ba5e9-7f81-4f9b-b8d1-46a3d3a6439a".into()),
            project_id: None,
            runner_version: Some("claude 2.1".into()),
            created_by: "cesar@u2s.local".into(),
            updated_at: "2026-08-15T09:04:00Z".into(),
            attestation: serde_json::json!({}),
        }];
        assert!(app.picking_run(), "with no run there is nothing to review");
        app.picked_run = Some("run-a".into());
        assert!(!app.picking_run());
        assert_eq!(app.active_run().as_deref(), Some("run-a"));
    }

    /// The run in flight is the default target, but an explicit pick wins.
    #[test]
    fn an_explicitly_picked_run_overrides_the_session_run() {
        let mut app = App::new();
        app.progress.run_id = Some("session".into());
        assert_eq!(app.active_run().as_deref(), Some("session"));
        app.picked_run = Some("older".into());
        assert_eq!(app.active_run().as_deref(), Some("older"));
    }

    #[test]
    fn tab_cycles_through_every_screen_and_wraps() {
        let mut app = App::new();
        let mut seen = vec![app.screen];
        for _ in 0..6 {
            next_screen(&mut app, 1);
            seen.push(app.screen);
        }
        assert_eq!(seen.len(), 7);
        next_screen(&mut app, 1);
        assert_eq!(app.screen, Screen::Connection, "it wraps");
        next_screen(&mut app, -1);
        assert_eq!(app.screen, Screen::Summary, "and wraps backwards");
    }

    #[test]
    fn the_source_screen_cycles_the_connectors() {
        let mut app = App::new();
        app.goto(Screen::Source);
        assert_eq!(app.config.source, Source::RepoDocs);
        vertical(&mut app, -1);
        assert_eq!(app.config.source, Source::DbSchema, "up from the first wraps");
        vertical(&mut app, 1);
        assert_eq!(app.config.source, Source::RepoDocs);
    }

    /// Approve/reject must not fire from a screen where there is no queue —
    /// `a` on the options screen is a keystroke, not a decision.
    #[test]
    fn review_commands_are_inert_outside_the_review_screen() {
        let mut app = App::new();
        app.goto(Screen::Options);
        for key in ['a', 'j', 'A', 'C'] {
            handle_key(&mut app, KeyCode::Char(key), KeyModifiers::NONE);
        }
        assert_eq!(app.screen, Screen::Options);
        assert!(app.last_command.is_none());
    }

    #[test]
    fn help_closes_on_the_next_keystroke() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
        assert!(app.show_help);
        handle_key(&mut app, KeyCode::Char('r'), KeyModifiers::NONE);
        assert!(!app.show_help);
        assert!(app.handle.is_none(), "the dismissing key does not also act");
    }
}
