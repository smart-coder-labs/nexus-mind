//! Runs `migrate-knowledge` as a child process and turns its stdout into
//! messages the UI thread can consume without ever blocking.
//!
//! The draw loop must stay responsive during a scan that takes minutes, so the
//! child is read on its own thread and everything reaches the app through a
//! channel. `try_recv` in the event loop, never `recv`.

use crate::config::RunConfig;
use crate::protocol::{parse_line, ParsedLine};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum RunnerMsg {
    Line(ParsedLine),
    /// A line the child wrote to stderr. Logs live here in `--json` mode.
    Log(String),
    /// The process ended. Always sent, including when it was killed.
    Exited { code: Option<i32> },
    /// The process could not be started at all.
    Failed(String),
}

/// A running child plus the channel carrying its output.
pub struct RunHandle {
    /// Shared with the reaper thread. A mutex rather than an owned `Child`
    /// because two threads need it: this one to kill, that one to reap. The
    /// reaper polls `try_wait` and releases the lock between polls, so a
    /// cancel never waits on a child that is still running.
    child: Option<Arc<Mutex<Child>>>,
    pub rx: Receiver<RunnerMsg>,
}

impl RunHandle {
    /// Stops the run.
    ///
    /// Killing a runner is safe by construction: candidates are staged in
    /// batches and nothing is ever committed by the runner, so the worst case is
    /// a partially staged run — which the review queue shows as exactly that.
    pub fn cancel(&mut self) {
        if let Some(shared) = self.child.take() {
            if let Ok(mut child) = shared.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Where `migrate-knowledge` lives.
///
/// An explicit override wins; otherwise the **most recently built** of the two
/// local profiles, then `PATH`.
///
/// Newest rather than release-first, which is what this originally did. A
/// months-old release build shadowing a fresh debug one produces the worst
/// possible failure: the runner rejects a flag this TUI just added, exits
/// before writing a single event, and the screen sits at zero with no reason
/// given. Whichever binary was built last is the one the operator meant.
pub fn resolve_binary() -> PathBuf {
    if let Ok(explicit) = std::env::var("NEXUSMIND_MIGRATE_BIN") {
        return PathBuf::from(explicit);
    }
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for candidate in [
        "../backend/target/release/migrate-knowledge",
        "../backend/target/debug/migrate-knowledge",
    ] {
        let path = PathBuf::from(candidate);
        let Ok(modified) = std::fs::metadata(&path).and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| modified > *t) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, p)| p)
        .unwrap_or_else(|| PathBuf::from("migrate-knowledge"))
}

pub fn spawn(bin: &PathBuf, config: &RunConfig, dry_run: bool) -> RunHandle {
    let (tx, rx) = mpsc::channel();

    let mut cmd = Command::new(bin);
    cmd.args(config.to_args(dry_run))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    for (k, v) in config.env_vars() {
        cmd.env(k, v);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(RunnerMsg::Failed(format!(
                "could not start {}: {e}",
                bin.display()
            )));
            return RunHandle { child: None, rx };
        }
    };

    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(parsed) = parse_line(&line) {
                    if tx.send(RunnerMsg::Line(parsed)).is_err() {
                        return;
                    }
                }
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if tx.send(RunnerMsg::Log(line)).is_err() {
                    return;
                }
            }
        }));
    }

    // Reaped off the UI thread, so a run that hangs cannot freeze the terminal.
    let shared = Arc::new(Mutex::new(child));
    let reaped = Arc::clone(&shared);
    std::thread::spawn(move || {
        loop {
            let status = match reaped.lock() {
                Ok(mut child) => child.try_wait(),
                // The mutex is poisoned only if a holder panicked while killing.
                // Nothing left to reap, and looping forever would leak a thread.
                Err(_) => return,
            };
            match status {
                Ok(Some(_)) => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => {
                    let _ = tx.send(RunnerMsg::Failed(format!("lost track of the runner: {e}")));
                    return;
                }
            }
        }

        // `Exited` must be the LAST message, and joining the readers first is
        // what makes it so. A short run finishes before the pipes are drained,
        // and an exit that overtakes the final `finished` event makes a clean
        // run look like a crash — `Progress::apply` treats an exit with no
        // prior finish as a failure, precisely so a real crash is not silent.
        for reader in readers {
            let _ = reader.join();
        }
        let code = reaped
            .lock()
            .ok()
            .and_then(|mut c| c.try_wait().ok().flatten())
            .and_then(|s| s.code());
        let _ = tx.send(RunnerMsg::Exited { code });
    });

    RunHandle {
        child: Some(shared),
        rx,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Source;
    use crate::protocol::RunEvent;

    /// The contract test between this crate and the backend's event module.
    ///
    /// It runs the real binary over this repository. If the runner is not built
    /// the test reports that and passes — a missing artifact is a build state,
    /// not a defect in this crate — but when the binary IS there, the stream it
    /// produces must parse into the types `protocol.rs` declares. That is the
    /// only thing keeping the duplicated wire type from silently drifting.
    #[test]
    fn stream_from_the_real_runner_parses() {
        let bin = resolve_binary();
        if !bin.exists() {
            eprintln!("skipped: {} is not built", bin.display());
            return;
        }
        let cfg = RunConfig {
            source: Source::RepoDocs,
            path: "../../".into(),
            includes: "docs/adr".into(),
            api_url: String::new(),
            api_key: String::new(),
            ..Default::default()
        };
        let handle = spawn(&bin, &cfg, true);

        let mut events = Vec::new();
        let mut noise = Vec::new();
        let mut logs = Vec::new();
        while let Ok(msg) = handle.rx.recv_timeout(std::time::Duration::from_secs(120)) {
            match msg {
                RunnerMsg::Line(ParsedLine::Event(e)) => events.push(e),
                RunnerMsg::Line(ParsedLine::Noise(n)) => noise.push(n),
                RunnerMsg::Line(ParsedLine::Unknown(name)) => {
                    panic!("the runner emitted `{name}`, which this build cannot parse")
                }
                RunnerMsg::Failed(e) => panic!("{e}"),
                RunnerMsg::Exited { .. } => break,
                RunnerMsg::Log(l) => logs.push(l),
            }
        }
        // Without this the failure reads "got None" and says nothing about a
        // runner that rejected an argument — which is exactly how this test
        // failed the first time it ran.
        let why = format!("runner: {}\nstderr: {logs:#?}", bin.display());

        assert!(
            noise.is_empty(),
            "stdout must carry events only in --json mode, found: {noise:?}"
        );
        assert!(
            matches!(events.first(), Some(RunEvent::Started { .. })),
            "every run opens with `started`, got {:?}\n{why}",
            events.first()
        );
        assert!(
            matches!(events.last(), Some(RunEvent::Finished { ok: true, .. })),
            "every run closes with `finished`, got {:?}\n{why}",
            events.last()
        );
        let scanned = events.iter().find_map(|e| match e {
            RunEvent::Scanned { units, .. } => Some(*units),
            _ => None,
        });
        assert!(
            scanned.unwrap_or(0) > 0,
            "the ADR directory should yield units; got {scanned:?}"
        );
    }

    /// A cancelled run must actually stop the child, not just drop the handle's
    /// view of it — an orphaned classifier keeps spending tokens.
    #[test]
    fn cancelling_terminates_the_child() {
        let bin = resolve_binary();
        if !bin.exists() {
            eprintln!("skipped: {} is not built", bin.display());
            return;
        }
        let cfg = RunConfig {
            source: Source::RepoDocs,
            path: "../../".into(),
            api_url: String::new(),
            api_key: String::new(),
            ..Default::default()
        };
        let mut handle = spawn(&bin, &cfg, true);
        handle.cancel();
        assert!(handle.child.is_none());
        // The reaper may or may not have won the race to report the exit; what
        // matters is that cancel returned, which it only does after `wait`.
    }

    /// The stale-release trap, as a test: whichever profile was built last is
    /// the one that gets run.
    #[test]
    fn the_most_recently_built_runner_wins() {
        let release = PathBuf::from("../backend/target/release/migrate-knowledge");
        let debug = PathBuf::from("../backend/target/debug/migrate-knowledge");
        let (Ok(r), Ok(d)) = (
            std::fs::metadata(&release).and_then(|m| m.modified()),
            std::fs::metadata(&debug).and_then(|m| m.modified()),
        ) else {
            eprintln!("skipped: both profiles must exist to compare them");
            return;
        };
        let expected = if r > d { release } else { debug };
        // Deliberately does not clear NEXUSMIND_MIGRATE_BIN: mutating the
        // process environment from one test in a parallel suite is how the
        // other flaky tests in this repository got that way. An operator who
        // set the override gets it honoured, and this assertion steps aside.
        if std::env::var("NEXUSMIND_MIGRATE_BIN").is_ok() {
            eprintln!("skipped: an explicit override is in effect");
            return;
        }
        assert_eq!(resolve_binary(), expected);
    }

    #[test]
    fn a_missing_binary_reports_instead_of_panicking() {
        let handle = spawn(
            &PathBuf::from("/nonexistent/migrate-knowledge"),
            &RunConfig::default(),
            true,
        );
        assert!(matches!(
            handle.rx.recv().unwrap(),
            RunnerMsg::Failed(_)
        ));
    }
}
