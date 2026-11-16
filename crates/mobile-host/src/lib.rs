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
    AutoLoginStartInput, LoginSessionCancelInput, LoginSessionEnd, LoginSessionRespondInput,
    LoginSessionStartInput,
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

async fn app__vrchat_auth_saved_snapshot_get(
    state: Arc<RuntimeHostState>,
    _input: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let snapshot = state
        .saved_auth_snapshot()
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(snapshot).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__backend_runtime_combined_snapshot_get(
    state: Arc<RuntimeHostState>,
    _input: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    serde_json::to_value(state.backend_runtime_combined_snapshot())
        .map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_auth_auto_login_start(
    state: Arc<RuntimeHostState>,
    input: AutoLoginStartInput,
) -> Result<serde_json::Value, CommandError> {
    let outcome = state
        .auto_login_start(input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(outcome).map_err(|error| CommandError::failed(error.to_string()))
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
    registry.register_typed::<serde_json::Value, serde_json::Value, _, _>(
        "app__vrchat_auth_saved_snapshot_get",
        app__vrchat_auth_saved_snapshot_get,
    );
    registry.register_typed::<serde_json::Value, serde_json::Value, _, _>(
        "app__backend_runtime_combined_snapshot_get",
        app__backend_runtime_combined_snapshot_get,
    );
    registry.register_typed::<AutoLoginStartInput, serde_json::Value, _, _>(
        "app__vrchat_auth_auto_login_start",
        app__vrchat_auth_auto_login_start,
    );
    registry.register_typed::<DebugEmitInput, serde_json::Value, _, _>(
        "app__debug_emit_event",
        app__debug_emit_event,
    );
    registry.register_typed::<WebExecuteInput, serde_json::Value, _, _>(
        "app__web_execute",
        app__web_execute,
    );
    registry
}

/// Router state for `serve_loopback`.
pub fn build_bus(state: Arc<RuntimeHostState>) -> Arc<CommandBus<RuntimeHostState>> {
    let events = EventBus::new(256);
    let _ = DEBUG_EVENTS.set(events.clone());
    // Replaces any existing sink. A host that already forwards events elsewhere
    // (the headless console sink) must tee instead - see BridgeSink.
    state.event_bus().set_sink(BridgeSink::new(events.clone()));
    Arc::new(CommandBus {
        registry: Arc::new(build_registry()),
        state,
        events,
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
            "app__vrchat_auth_saved_snapshot_get",
            "app__backend_runtime_combined_snapshot_get",
            "app__vrchat_auth_auto_login_start",
            "app__web_execute",
        ] {
            assert!(registry.contains(name), "missing command: {name}");
        }
        assert_eq!(registry.len(), 10);
    }
}






/// Generic VRChat HTTP proxy.
///
/// The desktop exposes ~41 per-endpoint proxy commands for this; on the phone a
/// single command covering any VRChat URL is both smaller and more future-proof -
/// VRCM already knows the paths it needs (94 endpoints), and none of them require
/// Rust-side knowledge of the response shape.
///
/// Returns the desktop's `HttpApiExecuteResponse` shape: `{ status, data }`, with
/// `data` being the raw response body string.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebExecuteInput {
    url: String,
    method: String,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body: Option<String>,
}

async fn app__web_execute(
    state: Arc<RuntimeHostState>,
    input: WebExecuteInput,
) -> Result<serde_json::Value, CommandError> {
    let mut request = vrcx_0_contracts::WebExecuteRequest::new(input.url, input.method);
    request.headers = input.headers;
    request.body = input.body;
    let (status, data) = state
        .web_client()
        .execute(request)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    Ok(serde_json::json!({ "status": status, "data": data }))
}




/// Bridges the runtime's typed event bus into the transport's broadcast channel,
/// so `GET /api/events` delivers the same events the desktop receives over Tauri's
/// `listen`. Without this the phone UI has no presence/notification updates - the
/// friend board would stay empty no matter how many commands it issues.
pub struct BridgeSink(EventBus);

impl BridgeSink {
    pub fn new(events: EventBus) -> Self {
        Self(events)
    }
}

impl vrcx_0_application_core::RuntimeEventSink for BridgeSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.0.publish(event.to_string(), payload);
    }
}


/// Temporary diagnostic: publish an arbitrary event into the transport broadcast.
///
/// Exists so the push path can be exercised without an authenticated session - the
/// real events (presence, notifications) only appear after login, which makes the WS
/// impossible to verify end to end otherwise. Remove once real events flow.
async fn app__debug_emit_event(
    _state: Arc<RuntimeHostState>,
    input: DebugEmitInput,
) -> Result<serde_json::Value, CommandError> {
    let bus = DEBUG_EVENTS
        .get()
        .ok_or_else(|| CommandError::failed("event bus not initialised"))?;
    bus.publish(input.name.clone(), input.payload.clone());
    Ok(serde_json::json!({ "published": input.name }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DebugEmitInput {
    name: String,
    #[serde(default)]
    payload: serde_json::Value,
}

static DEBUG_EVENTS: std::sync::OnceLock<EventBus> = std::sync::OnceLock::new();


