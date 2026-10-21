//! Process wide logging and error reporting setup.

use sentry::ClientInitGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Starts Sentry and installs the tracing subscriber used by deployed backend
/// processes. Sentry reads `SENTRY_DSN`, `SENTRY_ENVIRONMENT`, and
/// `SENTRY_RELEASE` from the process environment. With no DSN, error reporting
/// is disabled and the regular console logger still works.
pub fn init(log_level: &str) -> ClientInitGuard {
    let guard = sentry::init(sentry::ClientOptions::default());

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(log_level))
        .with(tracing_subscriber::fmt::layer())
        .with(sentry::integrations::tracing::layer())
        .init();

    if guard.is_enabled() {
        tracing::info!("Sentry error reporting enabled");
    }

    guard
}
