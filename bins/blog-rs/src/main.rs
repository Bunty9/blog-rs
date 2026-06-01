//! blog-rs server entry point.

mod config;
mod embed;
mod error;
mod mailer;
mod middleware;
mod routes;
mod state;
mod templates;
mod tokens;
mod view;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;

use base64::Engine;
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::state::AppState;

#[derive(Parser)]
#[command(name = "blog-rs")]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let cfg = match config::load(cli.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config load failed: {e}");
            return ExitCode::from(2);
        }
    };

    let filter = EnvFilter::try_new(&cfg.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .json()
                .with_current_span(true)
                .with_span_list(false),
        )
        .init();

    tracing::info!(bind = %cfg.bind, db = %cfg.database_url, "blog-rs starting");

    let pool = match db::initialize(&cfg.database_url, cfg.max_db_connections).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "database init failed");
            return ExitCode::from(2);
        }
    };

    let signing_key =
        match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(cfg.signing_key.as_bytes()) {
            Ok(k) if !k.is_empty() => k,
            _ => {
                tracing::error!("signing_key missing or invalid base64; refusing to boot");
                return ExitCode::from(2);
            }
        };

    if let Some(ab) = cfg.admin_bootstrap.clone() {
        match auth::password::hash(&ab.password) {
            Ok(hash) => match db::users::bootstrap_admin(&pool, &ab.email, &hash).await {
                Ok(true) => tracing::info!(email = %ab.email, "admin bootstrapped"),
                Ok(false) => tracing::info!("users table non-empty; admin_bootstrap ignored"),
                Err(e) => {
                    tracing::error!(error = %e, "admin bootstrap failed");
                    return ExitCode::from(2);
                }
            },
            Err(e) => {
                tracing::error!(error = %e, "argon2 hashing failed");
                return ExitCode::from(2);
            }
        }
    }

    let mailer = match mailer::from_env() {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "mailer init failed");
            return ExitCode::from(2);
        }
    };
    let site = state::SiteConfig::from_env();
    let state = AppState::new(pool, cfg.clone(), signing_key)
        .with_mailer(mailer)
        .with_site(site);
    let app = routes::router(state.clone());

    // Spawn the outbox dispatcher. The shutdown token is cancelled when the
    // axum graceful-shutdown future resolves so the worker stops polling
    // instead of being killed mid-tick by the runtime tear-down.
    let shutdown = tokio_util::sync::CancellationToken::new();
    let worker_handles = worker::spawn_all(state.clone(), shutdown.clone());

    let listener = match tokio::net::TcpListener::bind(&cfg.bind).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, "bind failed");
            return ExitCode::from(2);
        }
    };
    tracing::info!(addr = %cfg.bind, "listening");
    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    shutdown.cancel();
    for h in worker_handles {
        let _ = h.await;
    }
    if let Err(e) = serve_result {
        tracing::error!(error = %e, "axum::serve returned");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// Resolve when the process is asked to stop. Listens for Ctrl-C (local dev)
/// and, on Unix, SIGTERM — the signal `docker stop` / Litestream send — so the
/// outbox worker is cancelled between ticks instead of being SIGKILLed after
/// the runtime grace period.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            // If we can't install the handler, never resolve via this arm.
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    tracing::info!("shutdown signal received");
}
