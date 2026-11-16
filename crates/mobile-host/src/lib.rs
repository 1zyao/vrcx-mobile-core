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
use vrcx_0_application::auth::{
    LoginSessionCancelInput, LoginSessionEnd, LoginSessionRespondInput, LoginSessionStartInput,
};
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

async fn app__vrchat_auth_session_start(
    state: Arc<RuntimeHostState>,
    input: LoginSessionStartInput,
) -> Result<serde_json::Value, CommandError> {
    let outcome = state.start_login_session(input).await;
    serde_json::to_value(outcome).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_auth_session_cancel(
    state: Arc<RuntimeHostState>,
    input: LoginSessionCancelInput,
) -> Result<serde_json::Value, CommandError> {
    let outcome = state.cancel_login_session(input).await;
    serde_json::to_value(outcome).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_auth_session_respond(
    state: Arc<RuntimeHostState>,
    input: LoginSessionRespondInput,
) -> Result<serde_json::Value, CommandError> {
    let outcome = state.respond_login_session(input).await;
    serde_json::to_value(outcome).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_auth_session_end(
    state: Arc<RuntimeHostState>,
    input: LoginSessionEnd,
) -> Result<serde_json::Value, CommandError> {
    let snapshot = state
        .end_login_session(input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(snapshot).map_err(|error| CommandError::failed(error.to_string()))
}

/// The command surface exposed to the phone UI.
pub fn build_registry() -> CommandRegistry<RuntimeHostState> {
    let mut registry = CommandRegistry::new();
    registry.register_typed::<feed::FeedRowsQueryInput, Vec<feed::FeedRowOutput>, _, _>(
        "app__feed_rows_query",
        app__feed_rows_query,
    );
    registry.register_typed::<LoginSessionStartInput, serde_json::Value, _, _>(
        "app__vrchat_auth_session_start",
        app__vrchat_auth_session_start,
    );
    registry.register_typed::<LoginSessionRespondInput, serde_json::Value, _, _>(
        "app__vrchat_auth_session_respond",
        app__vrchat_auth_session_respond,
    );
    registry.register_typed::<LoginSessionCancelInput, serde_json::Value, _, _>(
        "app__vrchat_auth_session_cancel",
        app__vrchat_auth_session_cancel,
    );
    registry.register_typed::<LoginSessionEnd, serde_json::Value, _, _>(
        "app__vrchat_auth_session_end",
        app__vrchat_auth_session_end,
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



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_the_phone_command_surface() {
        let registry = build_registry();
        for name in [
            "app__feed_rows_query",
            "app__vrchat_auth_session_start",
            "app__vrchat_auth_session_cancel",
            "app__vrchat_auth_session_respond",
            "app__vrchat_auth_session_end",
        ] {
            assert!(registry.contains(name), "missing command: {name}");
        }
        assert_eq!(registry.len(), 5);
    }
}


