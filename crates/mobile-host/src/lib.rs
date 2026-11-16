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

/// Mirrors cloud's `VrchatFavoriteGroupsInput`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteGroupsInput {
    #[serde(default)]
    n: i32,
    #[serde(default)]
    offset: i32,
    #[serde(default)]
    owner_id: String,
}

async fn app__vrchat_favorite_groups_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<FavoriteGroupsInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .favorite_groups(input.n, input.offset, input.owner_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

/// Mirrors cloud's `VrchatFavoriteAddInput`; `favoriteId` is the ENTITY id
/// (world/avatar/user), which is how VRCX addresses favourites.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteAddInput {
    #[serde(rename = "type")]
    type_name: vrcx_0_contracts::VrchatFavoriteType,
    #[serde(default)]
    favorite_id: String,
    #[serde(default)]
    tags: String,
}

/// Mirrors cloud's `VrchatFavoriteDeleteInput` - an entity id, not a record id.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteDeleteInput {
    #[serde(default)]
    object_id: String,
}

async fn app__vrchat_favorite_add(
    state: Arc<RuntimeHostState>,
    args: InputArgs<FavoriteAddInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .favorite_add_remote(vrcx_0_application::favorites::FavoriteRemoteAddInput {
            kind: input.type_name,
            entity_id: input.favorite_id,
            tags: input.tags,
        })
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_favorite_delete(
    state: Arc<RuntimeHostState>,
    args: InputArgs<FavoriteDeleteInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .favorite_delete_remote(vrcx_0_application::favorites::FavoriteRemoteDeleteInput {
            object_id: args.input.object_id,
        })
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

/// Mirrors cloud's `VrchatFavoriteWorldsInput`; the request goes to `worlds/favorites`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteWorldsInput {
    #[serde(default)]
    n: i32,
    #[serde(default)]
    offset: i32,
    #[serde(default)]
    owner_id: String,
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    tag: String,
}

async fn app__vrchat_favorite_worlds_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<FavoriteWorldsInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .favorite_worlds(input.n, input.offset, input.owner_id, input.user_id, input.tag)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

/// Canonical argument envelope.
///
/// The desktop and cloud frontends wrap the input in a key named after the command's
/// parameter (`arg(&args, "query")` in cloud/crates/server/src/commands/local.rs), so
/// the wire shape is `{"query": {...}}`. Keeping the same shape here is what makes
/// VRCM literally the same frontend as the React app.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryArgs<T> {
    query: T,
}

async fn app__feed_rows_query(
    state: Arc<RuntimeHostState>,
    args: QueryArgs<feed::FeedRowsQueryInput>,
) -> Result<Vec<feed::FeedRowOutput>, CommandError> {
    let db = Arc::clone(state.database());
    blocking("feed rows", move || feed::feed_rows_query(&db, args.query)).await
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

/// Avatar styles, same name and argument shape as the cloud/desktop command
/// (`registry.register("app__vrchat_avatar_styles_get", …)` in
/// cloud/crates/server/src/commands/vrchat_proxy.rs): no arguments, returns the raw
/// `VrchatResponse` (`{ status, data }`).
///
/// The request itself is built by `VrchatRemoteFacade::avatar_styles`
/// (crates/application/src/remote/vrchat_facade.rs), so the endpoint path, method and
/// scope are not duplicated here.
async fn app__vrchat_avatar_styles_get(
    state: Arc<RuntimeHostState>,
    _args: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .avatar_styles()
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

/// Canonical argument envelope for commands whose single parameter is named `input`.
///
/// Cloud wraps these as `arg(&args, "input")`, so the wire shape is `{"input": {...}}`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputArgs<T> {
    input: T,
}

/// Mirrors cloud's `FriendUserInput` (`{"input":{"userId":"…"}}`).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FriendUserInput {
    user_id: String,
}

/// Mirrors cloud's `SearchUsersInput`; `params` is the facade's own parameter type, so
/// the JSON is whatever that type already deserialises.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchUsersInput {
    params: vrcx_0_contracts::vrchat_requests::UserSearchParams,
}

/// Mirrors cloud's `SearchGroupsInput`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchGroupsInput {
    params: vrcx_0_contracts::vrchat_requests::GroupSearchParams,
}

async fn app__vrchat_auth_current_user_get(
    state: Arc<RuntimeHostState>,
    _args: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .current_user()
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_friend_status_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<FriendUserInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .friend_status(args.input.user_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_search_users_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<SearchUsersInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .search_users(args.input.params)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_search_groups_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<SearchGroupsInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .search_groups(args.input.params)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

/// The command surface exposed to the phone UI.
pub fn build_registry() -> CommandRegistry<RuntimeHostState> {
    let mut registry = CommandRegistry::new();
    registry.register_typed::<serde_json::Value, serde_json::Value, _, _>(
        "app__vrchat_avatar_styles_get",
        app__vrchat_avatar_styles_get,
    );
    registry.register_typed::<serde_json::Value, serde_json::Value, _, _>(
        "app__vrchat_auth_current_user_get",
        app__vrchat_auth_current_user_get,
    );
    registry.register_typed::<InputArgs<FriendUserInput>, serde_json::Value, _, _>(
        "app__vrchat_friend_status_get",
        app__vrchat_friend_status_get,
    );
    registry.register_typed::<InputArgs<SearchUsersInput>, serde_json::Value, _, _>(
        "app__vrchat_search_users_get",
        app__vrchat_search_users_get,
    );
    registry.register_typed::<InputArgs<SearchGroupsInput>, serde_json::Value, _, _>(
        "app__vrchat_search_groups_get",
        app__vrchat_search_groups_get,
    );
    registry.register_typed::<InputArgs<FavoriteWorldsInput>, serde_json::Value, _, _>(
        "app__vrchat_favorite_worlds_get",
        app__vrchat_favorite_worlds_get,
    );
    registry.register_typed::<InputArgs<FavoriteGroupsInput>, serde_json::Value, _, _>(
        "app__vrchat_favorite_groups_get",
        app__vrchat_favorite_groups_get,
    );
    registry.register_typed::<InputArgs<FavoriteAddInput>, serde_json::Value, _, _>(
        "app__vrchat_favorite_add",
        app__vrchat_favorite_add,
    );
    registry.register_typed::<InputArgs<FavoriteDeleteInput>, serde_json::Value, _, _>(
        "app__vrchat_favorite_delete",
        app__vrchat_favorite_delete,
    );
    registry.register_typed::<FavoriteListArgs, Vec<vrcx_0_contracts::FavoriteRow>, _, _>(
        "app__favorite_list",
        app__favorite_list,
    );
    registry.register_typed::<QueryArgs<feed::FeedRowsQueryInput>, Vec<feed::FeedRowOutput>, _, _>(
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
            "app__favorite_list",
            "app__vrchat_avatar_styles_get",
            "app__vrchat_auth_current_user_get",
            "app__vrchat_friend_status_get",
            "app__vrchat_search_users_get",
            "app__vrchat_search_groups_get",
            "app__vrchat_favorite_worlds_get",
            "app__vrchat_favorite_add",
            "app__vrchat_favorite_delete",
            "app__vrchat_favorite_groups_get",
        ] {
            assert!(registry.contains(name), "missing command: {name}");
        }
        assert_eq!(registry.len(), 20);
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



/// Forwards runtime events to two sinks.
///
/// A host that already logs events (the headless console sink) must not lose them
/// when the command bus is enabled, so both destinations receive every event.
pub struct TeeSink {
    pub first: std::sync::Arc<dyn vrcx_0_application_core::RuntimeEventSink>,
    pub second: std::sync::Arc<dyn vrcx_0_application_core::RuntimeEventSink>,
}

impl vrcx_0_application_core::RuntimeEventSink for TeeSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        self.first.emit(event, payload.clone());
        self.second.emit(event, payload);
    }
}


/// Favorites read model, same name and argument shape as the desktop/cloud command
/// (`arg(&args, "kind")` in cloud/crates/server/src/commands/local_data_queries.rs).
///
/// The favourites live in the local database that a recording node keeps; the phone
/// only reads them. `FavoriteBaselineStore` is implemented by the persistence store,
/// so no new data path is invented here.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteListArgs {
    kind: vrcx_0_core::FavoriteEntityKind,
}

async fn app__favorite_list(
    state: Arc<RuntimeHostState>,
    args: FavoriteListArgs,
) -> Result<Vec<vrcx_0_contracts::FavoriteRow>, CommandError> {
    let db = Arc::clone(state.database());
    // Not the shared `blocking` helper: it insists on application_core::Error, while the
    // store returns persistence::Error. Converting here keeps both error types untouched.
    let rows = tokio::task::spawn_blocking(move || {
        use vrcx_0_application_realtime::FavoriteBaselineStore;
        let store = vrcx_0_outbound_adapters::PersistenceRealtimeStore::new(db);
        store.favorite_list(None, args.kind)
    })
    .await
    .map_err(|error| CommandError::failed(format!("favorite list task failed: {error}")))?
    .map_err(|error| CommandError::failed(error.to_string()))?;
    Ok(rows)
}







