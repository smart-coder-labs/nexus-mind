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
            // Editing a finding is the one action that leaves the TUI, so it is
            // handled here rather than in `handle_key`: it needs the terminal
            // back, and `handle_key` has no way to give it up.
            if key.code == KeyCode::Char('E')
                && app.screen == Screen::Review
                && app.current_candidate().is_some()
            {
                edit_current_candidate(terminal, &mut app);
                continue;
            }
            handle_key(&mut app, key.code, key.modifiers);
        }
    }
    Ok(())
}

/// Hands the candidate under the cursor to `$EDITOR`, then sends back what came
/// out.
///
/// A finding is a paragraph of prose, and correcting one means moving around
/// inside it. Rather than grow a text editor in here — cursor, wrapping,
/// selection, undo, all of it worse than what the operator already has — the
/// TUI steps aside for the editor they configured, the way `git commit` and
/// `crontab -e` do.
fn edit_current_candidate(terminal: &mut ratatui::DefaultTerminal, app: &mut App) {
    let Some(candidate) = app.current_candidate() else {
        return;
    };
    let document = format!(
        "# Editing a staged migration candidate. Lines starting with '#' are ignored.\n\
         # Save and quit to apply; quit without saving to leave it as it was.\n\
         #\n\
         # Source: {}\n\
         # Kind may be: memory, convention, task, sdd_artifact\n\
         Kind: {}\n\
         Title: {}\n\
         \n\
         {}\n",
        candidate.source_identity,
        candidate.destination_kind,
        candidate.title(),
        candidate.content,
    );

    let path = std::env::temp_dir().join(format!("nexusmind-candidate-{}.md", candidate.id));
    if let Err(e) = std::fs::write(&path, &document) {
        app.status = format!("could not open an editor: {e}");
        return;
    }

    // Down and back up around the child: the editor needs the real terminal,
    // and ratatui's alternate screen and raw mode would otherwise still be on.
    ratatui::restore();
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    // `sh -c` so a configured editor with arguments ("code --wait", "emacs -nw")
    // works, which is common enough that requiring a bare binary would be a bug.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\"", editor = editor))
        .arg("sh")
        .arg(&path)
        .status();
    *terminal = ratatui::init();
    terminal.clear().ok();

    match status {
        Ok(s) if s.success() => {}
        Ok(_) => {
            app.status = format!("{editor} exited with an error — nothing was changed");
            let _ = std::fs::remove_file(&path);
            return;
        }
        Err(e) => {
            app.status = format!("could not run {editor}: {e}");
            let _ = std::fs::remove_file(&path);
            return;
        }
    }

    let edited = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::remove_file(&path);
    let (title, kind, content) = parse_edited_candidate(&edited);
    if content.trim().is_empty() {
        app.status = "the edited candidate was empty — nothing was changed".into();
        return;
    }
    let before = app.current_candidate();
    let unchanged = before.is_some_and(|c| {
        content == c.content && title == c.title() && (kind.is_empty() || kind == c.destination_kind)
    });
    if unchanged {
        app.status = "unchanged".into();
        return;
    }
    app.apply_candidate_edit(title, kind, content);
}

/// Splits the edited document back into a title, a destination kind and a body.
///
/// Comment lines are dropped, the first `Kind:` and `Title:` lines are the
/// header, and everything after the title is the content. Only the *first* of
/// each is special, so a body that happens to contain one survives intact — a
/// convention about commit messages that itself shows a `Title:` line must not
/// be truncated at it.
///
/// An empty kind means "leave it where it is". A reviewer who deletes the
/// header entirely still gets their prose sent rather than silently dropped.
fn parse_edited_candidate(document: &str) -> (String, String, String) {
    let (mut title, mut kind) = (String::new(), String::new());
    let mut body: Vec<&str> = Vec::new();
    let (mut seen_title, mut seen_kind) = (false, false);
    for line in document.lines() {
        if line.starts_with('#') && !seen_title {
            continue;
        }
        if !seen_kind && !seen_title {
            if let Some(rest) = line.strip_prefix("Kind:") {
                kind = rest.trim().to_string();
                seen_kind = true;
                continue;
            }
        }
        match line.strip_prefix("Title:") {
            Some(rest) if !seen_title => {
                title = rest.trim().to_string();
                seen_title = true;
            }
            _ => body.push(line),
        }
    }
    (title, kind, body.join("\n").trim().to_string())
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // Text entry swallows almost everything: a path containing `r` must not
    // start a run. Only Esc and Enter get out.
    if app.editing {
        match code {
            KeyCode::Esc | KeyCode::Enter => {
                let was = app.current_field();
                app.editing = false;
                app.edit_pristine = false;
                // Finishing the URL or the key is the moment the operator has
                // said what backend they mean. Probing here — rather than
                // waiting for `t` — is what makes the run history appear for
                // someone who came back to resume a migration.
                if matches!(was, Some(app::FieldId::ApiUrl | app::FieldId::ApiKey)) {
                    app.probe_if_connection_changed();
                }
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

    /// The round trip through `$EDITOR`: the header the TUI writes has to come
    /// back off, and the body has to survive verbatim.
    #[test]
    fn the_edited_document_round_trips_title_and_body() {
        let doc = "# Editing a staged migration candidate.\n\
                   # Save and quit to apply.\n\
                   #\n\
                   # Source: src/a.rs\n\
                   Kind: convention\n\
                   Title: Corrected title\n\
                   \n\
                   First paragraph.\n\
                   \n\
                   Second paragraph.\n";
        let (title, kind, content) = parse_edited_candidate(doc);
        assert_eq!(title, "Corrected title");
        assert_eq!(kind, "convention");
        assert_eq!(content, "First paragraph.\n\nSecond paragraph.");
    }

    /// Re-filing is the point: the reviewer changes one word in the header and
    /// the candidate lands somewhere else.
    #[test]
    fn the_kind_line_is_how_a_candidate_is_refiled() {
        let (_, kind, content) =
            parse_edited_candidate("Kind: memory\nTitle: T\n\nthe body\n");
        assert_eq!(kind, "memory");
        assert_eq!(content, "the body");
    }

    /// A body discussing a Kind: line — a convention about this very format —
    /// must not be truncated at it, and must not silently re-file itself.
    #[test]
    fn a_kind_line_inside_the_body_is_left_alone() {
        let (_, kind, content) = parse_edited_candidate(
            "Kind: convention\nTitle: T\n\nWrite the header as:\nKind: memory\n",
        );
        assert_eq!(kind, "convention", "only the first Kind: is the header");
        assert_eq!(content, "Write the header as:\nKind: memory");
    }

    /// Only the first `Title:` is the title. A body that discusses one — a
    /// convention about commit messages, say — must not be truncated at it.
    #[test]
    fn a_title_line_inside_the_body_is_left_alone() {
        let doc = "Title: The real title\n\nUse this shape:\nTitle: <what changed>\n";
        let (title, _, content) = parse_edited_candidate(doc);
        assert_eq!(title, "The real title");
        assert_eq!(content, "Use this shape:\nTitle: <what changed>");
    }

    /// A reviewer who deletes the whole header still gets their prose sent, not
    /// silently dropped as "no title line found".
    #[test]
    fn a_document_with_no_header_is_all_content() {
        let (title, kind, content) = parse_edited_candidate("just the prose, rewritten\n");
        assert_eq!(title, "");
        assert_eq!(kind, "", "no header means the kind is left where it is");
        assert_eq!(content, "just the prose, rewritten");
    }

    /// Connecting has to be something you get by typing the details, not by
    /// knowing a keystroke — that is what puts the run history in front of
    /// someone who came back to resume a migration. But it must not fire on a
    /// half-typed URL, nor again on a field that did not change.
    #[test]
    fn finishing_the_connection_fields_probes_once_and_only_when_complete() {
        let mut app = App::new();
        app.goto(Screen::Connection);
        app.config.api_url = "http://localhost:8080".into();
        app.config.api_key = String::new();

        app.probe_if_connection_changed();
        assert!(
            app.status.is_empty() || !app.status.contains("testing"),
            "a blank key is mid-typing, not a connection attempt: {}",
            app.status
        );

        app.config.api_key = "nm_x".into();
        app.probe_if_connection_changed();
        assert!(
            app.status.contains("testing"),
            "complete details must reach the backend: {}",
            app.status
        );

        app.status = "quiet".into();
        app.probe_if_connection_changed();
        assert_eq!(
            app.status, "quiet",
            "an unchanged pair must not fire a second request"
        );
    }

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
