//! Command set for the phone core.
//!
//! Registered against the real `RuntimeHostState` built by
//! `vrcx_0_composition`, which is the same state the desktop and headless hosts
//! use. That is deliberate: commands ported from the cloud server only need their
//! state type swapped, never their logic.
//!
//! Each command is written once as a plain async fn over `(state, Input)` and
//! registered through `register_typed`, so no hand-written JSON adapter exists.

use std::sync::Arc;

use vrcx_0_command_server::{CommandBus, CommandError, CommandRegistry, EventBus};
use vrcx_0_composition::RuntimeHostState;
use vrcx_0_persistence::feed;

/// Runs a synchronous persistence call off the async workers.
async fn blocking<T, F>(label: &str, operation: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, vrcx_0_persistence::Error> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| CommandError::failed(format!("{label} task failed: {error}")))?
        .map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__feed_rows_query(
    state: Arc<RuntimeHostState>,
    input: feed::FeedRowsQueryInput,
) -> Result<Vec<feed::FeedRowOutput>, CommandError> {
    let db = Arc::clone(state.database());
    blocking("feed rows", move || feed::feed_rows_query(&db, input)).await
}

/// The command surface exposed to the phone UI.
pub fn build_registry() -> CommandRegistry<RuntimeHostState> {
    let mut registry = CommandRegistry::new();
    registry.register_typed::<feed::FeedRowsQueryInput, Vec<feed::FeedRowOutput>, _, _>(
        "app__feed_rows_query",
        app__feed_rows_query,
    );
    registry
}

/// Router state for `serve_loopback`.
pub fn build_bus(state: RuntimeHostState) -> Arc<CommandBus<RuntimeHostState>> {
    Arc::new(CommandBus {
        registry: Arc::new(build_registry()),
        state: Arc::new(state),
        events: EventBus::new(256),
    })
}
