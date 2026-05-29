//! Background workers spawned in `main()`. Today: just the outbox dispatcher.
//!
//! The shutdown token is passed in so the long-running poll loop can return
//! before `tokio::main` joins the runtime. If the binary is killed without
//! cancelling the token (SIGTERM/SIGKILL), the worker dies with the process
//! and any in-flight send is abandoned. That's fine — the outbox row is still
//! marked `sending` and the next tick after restart will time it out (a real
//! deployment would add a "rescue stuck sending rows" sweep on startup).

pub mod outbox;

use crate::state::AppState;
use tokio_util::sync::CancellationToken;

#[allow(dead_code)] // Called from main; tests drive outbox::tick directly.
pub fn spawn_all(state: AppState, shutdown: CancellationToken) -> Vec<tokio::task::JoinHandle<()>> {
    vec![tokio::spawn(outbox::run(state, shutdown))]
}
