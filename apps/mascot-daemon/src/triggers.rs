//! Autonomous background scheduler for:
//! 1. Periodic idle consolidation ("Sleep Cycle" -> NexusMind sync & pattern detection)
//! 2. User-configured recurring routines (`mascot.yaml -> routines`)

use crate::agent::start_agent_run;
use crate::config::AutonomyLevel;
use crate::ipc::{ensure_nexusmind_ready, verify_and_heartbeat, DaemonState};
use crate::learning::run_idle_consolidation;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

pub fn spawn_background_scheduler(state: Arc<DaemonState>) {
    tokio::spawn(async move {
        let mut last_consolidation = Instant::now();
        let mut last_heartbeat = Instant::now() - Duration::from_secs(60);
        let mut last_routine_run: HashMap<String, Instant> = HashMap::new();
        let mut tick = tokio::time::interval(Duration::from_secs(30));

        loop {
            tick.tick().await;

            // 0. Periodic live heartbeat to NexusMind (every 60s)
            if last_heartbeat.elapsed() >= Duration::from_secs(60) {
                last_heartbeat = Instant::now();
                let _ = verify_and_heartbeat(&state).await;
            }

            if state.kill_switch.load(Ordering::SeqCst) {
                continue;
            }

            // 1. Periodic consolidation every 15 minutes
            if last_consolidation.elapsed() >= Duration::from_secs(15 * 60) {
                last_consolidation = Instant::now();
                match run_idle_consolidation(&state).await {
                    Ok(report) if report.consolidated_events > 0 => {
                        info!(
                            consolidated = report.consolidated_events,
                            synced = report.synced_to_nexusmind,
                            suggestions = report.suggestions_created,
                            "idle consolidation completed"
                        );
                    }
                    Err(err) => warn!(error = %err, "idle consolidation error"),
                    _ => {}
                }
            }

            // 2. Scheduled autonomous routines (requires mandatory NexusMind connection)
            if ensure_nexusmind_ready(&state).await.is_err() {
                continue;
            }
            let (autonomy_level, routines) = {
                let cfg = state.config.read().unwrap();
                (cfg.autonomy.level, cfg.routines.clone())
            };
            if !matches!(
                autonomy_level,
                AutonomyLevel::SupervisedAutonomous | AutonomyLevel::FullAutonomous
            ) {
                continue;
            }

            for routine in routines.into_iter().filter(|r| r.enabled && r.every_minutes > 0) {
                let due = last_routine_run
                    .get(&routine.id)
                    .map(|t| t.elapsed() >= Duration::from_secs(routine.every_minutes * 60))
                    .unwrap_or(true);
                if due {
                    last_routine_run.insert(routine.id.clone(), Instant::now());
                    info!(routine = %routine.id, title = %routine.title, "running scheduled routine");
                    if let Err(err) =
                        start_agent_run(&state, &routine.prompt, Some("scheduler"), None, true)
                            .await
                    {
                        warn!(routine = %routine.id, error = %err, "scheduled routine failed");
                    }
                }
            }
        }
    });
}
