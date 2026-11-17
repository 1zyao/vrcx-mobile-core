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
use vrcx_0_contracts::notifications::{NotificationListItemOutput, NotificationListQueryInput};
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
#[derive(serde::Deserialize, specta::Type)]
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

async fn app__vrchat_group_gallery_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::social::VrchatGroupGalleryInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .group_gallery(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_group_posts_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::social::VrchatGroupPagedInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .group_posts(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_group_user_instances_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::social::VrchatGroupUserGroupsInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .group_user_instances(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_group_members_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::social::VrchatGroupMembersInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .group_members(input.group_id, input.n, input.offset, input.sort, input.role_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_group_join(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::social::VrchatGroupIdInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .group_join(args.input.group_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_group_leave(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::social::VrchatGroupIdInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .group_leave(args.input.group_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

/// Mirrors cloud's `VrchatFavoriteAddInput`; `favoriteId` is the ENTITY id
/// (world/avatar/user), which is how VRCX addresses favourites.
#[derive(serde::Deserialize, specta::Type)]
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
#[derive(serde::Deserialize, specta::Type)]
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
#[derive(serde::Deserialize, specta::Type)]
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
#[derive(serde::Deserialize, specta::Type)]
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

/// Cloud keeps this runtime in a `OnceLock`; it carries the per-card cache, so it must outlive a
/// single command invocation.
static FAVORITE_DETAILS: std::sync::OnceLock<
    vrcx_0_application::favorites::FavoriteDetailsRuntime,
> = std::sync::OnceLock::new();

async fn app__favorite_details_hydrate(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::favorites::FavoriteDetailsHydrateInput>,
) -> Result<vrcx_0_application::favorites::FavoriteDetailsHydrateOutput, CommandError> {
    let expected_scope = state
        .require_active_scope("Batch action")
        .map_err(|error| CommandError::failed(error.to_string()))?;
    let runtime = FAVORITE_DETAILS.get_or_init(|| state.favorite_details_runtime());
    runtime
        .hydrate(args.input, expected_scope)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))
}
async fn app__notification_list_query(
    state: Arc<RuntimeHostState>,
    args: QueryArgs<NotificationListQueryInput>,
) -> Result<Vec<NotificationListItemOutput>, CommandError> {
    // The same persistence call cloud's local_data service makes, so the phone reads notifications
    // from its own (synced) database instead of asking VRChat.
    let db = Arc::clone(state.database());
    blocking("notification list", move || {
        vrcx_0_persistence::notifications::notification_list_query(db.as_ref(), args.query)
    })
    .await
}

/// `app__world_get` takes its input under `input`, not `query`.
///
/// Cloud registers it as `arg(&args, "input")?`
/// (cloud/crates/server/src/commands/local_data_queries.rs:800), unlike the
/// `query`-enveloped list commands above, so the wire shape is
/// `{"input": {"worldId": …}}` and stays that way here.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct WorldGetArgs {
    input: WorldGetInput,
}

/// World-get input, declared here rather than in `contracts`.
///
/// Upstream has no such type - it hands the three fields to the world cache directly - so keeping it
/// in the shared `contracts` crate meant carrying a fork-only file inside a crate that also exists
/// upstream, and that file was the one thing blocking the merge. Same reason the phone's other
/// command inputs live in this crate.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct WorldGetInput {
    world_id: String,
    #[serde(default)]
    force: bool,
    #[serde(default = "default_true")]
    full: bool,
}

fn default_true() -> bool {
    true
}

/// World details, cache-first, from the same read model the desktop/cloud command
/// uses (`RuntimeHostState::world_get`); this replaces the phone's
/// `proxy GET https://api.vrchat.cloud/api/1/worlds/{id}` round trip.
async fn app__world_get(
    state: Arc<RuntimeHostState>,
    args: WorldGetArgs,
) -> Result<vrcx_0_contracts::VrchatResponse, CommandError> {
    state
        .world_get(&args.input.world_id, args.input.force, args.input.full)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))
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

/// Session restore, using the state-level method that applies login transitions.
///
/// `RuntimeHostContext::auto_login_start` takes a no-op transition sink, so it never calls
/// `start_authenticated_runtime_session` and leaves `RuntimeAuthScope` inactive. Anything that
/// checks the scope (the social baselines, for instance) then reports `stale`. The state-level
/// `start_auto_login` does apply transitions, and cloud wires this command to it as well.
async fn app__vrchat_auth_auto_login_start(
    state: Arc<RuntimeHostState>,
    args: InputArgs<AutoLoginStartInput>,
) -> Result<serde_json::Value, CommandError> {
    let outcome = state
        .start_auto_login(args.input)
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
async fn app__config_list_values(
    state: Arc<RuntimeHostState>,
    _args: serde_json::Value,
) -> Result<Vec<vrcx_0_contracts::ConfigReadEntry>, CommandError> {
    state
        .config_list_values()
        .await
        .map_err(|error| CommandError::failed(error.to_string()))
}

/// `app__config_set_values` takes a flat `entries` array, exactly as the desktop reads it
/// (`arg(&args, "entries")`). Wrapping it in `input` - which the first version did - makes every
/// save fail deserialisation, and a caller that swallows the error sees nothing happen.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct ConfigRemoveArgs {
    #[serde(default)]
    key: String,
}

async fn app__config_remove_value(
    state: Arc<RuntimeHostState>,
    args: ConfigRemoveArgs,
) -> Result<i64, CommandError> {
    state
        .config_remove_value(args.key)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__config_set_values(
    state: Arc<RuntimeHostState>,
    args: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let entries: Vec<vrcx_0_contracts::ConfigWriteEntry> = serde_json::from_value(
        args.get("entries").cloned().unwrap_or(serde_json::Value::Null),
    )
    .map_err(|error| CommandError::bad_request(format!("invalid `entries`: {error}")))?;
    state
        .config_set_values(entries)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    Ok(serde_json::Value::Null)
}

/// Cloud's `sync__configure`: writes `remoteSync.*` config entries, then restarts the engine.
///
/// The wire shape is cloud's own - flat `enabled` / `intervalSec` / `connection` on the args, not
/// wrapped in `input` - so a client written against cloud works here unchanged.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SyncConfigureArgs {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    interval_sec: Option<i64>,
    #[serde(default)]
    connection: Option<vrcx_0_contracts::sync::SyncConnectionInput>,
    /// Cloud reads this from the args but never writes it. Kept here because VRCX's own settings
    /// page sends it, and silently dropping it would make that field a lie.
    #[serde(default)]
    collector_priority: Option<i64>,
}

async fn sync__configure(
    state: Arc<RuntimeHostState>,
    args: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let input: SyncConfigureArgs = serde_json::from_value(args)
        .map_err(|error| CommandError::bad_request(format!("invalid sync config: {error}")))?;

    // Values are written raw, the way the desktop does it (`config_entry(key, value: impl
    // Display)`): a JSON string would be stored with its quotes, and `"host"` is not a host. I had
    // this wrong first and it wrote `"100.70.120.64"` into the config, which broke the next cycle
    // with "PostgreSQL connect failed".
    let mut entries: Vec<vrcx_0_contracts::ConfigWriteEntry> = Vec::new();
    let mut push = |key: &str, value: String| {
        entries.push(vrcx_0_contracts::ConfigWriteEntry {
            key: key.to_string(),
            value,
        });
    };
    if let Some(enabled) = input.enabled {
        push("remoteSync.enabled", enabled.to_string());
    }
    if let Some(fields) = input.connection {
        push("remoteSync.host", fields.host.trim().to_string());
        if (1..65536).contains(&fields.port) {
            push("remoteSync.port", fields.port.to_string());
        }
        push("remoteSync.user", fields.user.trim().to_string());
        if let Some(password) = fields.password {
            if !password.trim().is_empty() {
                push("remoteSync.password", password.trim().to_string());
            }
        }
        push("remoteSync.database", fields.database.trim().to_string());
        push("remoteSync.tlsVerify", fields.tls_verify.to_string());
        push(
            "remoteSync.allowPlaintext",
            fields.allow_plaintext.to_string(),
        );
    }
    if let Some(interval) = input.interval_sec {
        push("remoteSync.intervalSec", interval.max(5).to_string());
    }
    if let Some(priority) = input.collector_priority {
        push(
            "remoteSync.collectorPriority",
            priority.clamp(0, 100).to_string(),
        );
    }
    if !entries.is_empty() {
        state
            .config_set_values(entries)
            .await
            .map_err(|error| CommandError::failed(error.to_string()))?;
        state
            .remote_sync_restart()
            .await
            .map_err(|error| CommandError::failed(error.to_string()))?;
    }
    let status = state.remote_sync().status().await;
    serde_json::to_value(status).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__game_log_sessions_query(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application_game::GameLogSessionsQueryInput>,
) -> Result<Vec<vrcx_0_application_game::GameLogSessionDto>, CommandError> {
    state
        .game_log_sessions_query(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))
}

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
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct InputArgs<T> {
    input: T,
}

/// Mirrors cloud's `FriendUserInput` (`{"input":{"userId":"…"}}`).
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct FriendUserInput {
    user_id: String,
}

/// Mirrors cloud's `SearchUsersInput`; `params` is the facade's own parameter type, so
/// the JSON is whatever that type already deserialises.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct SearchUsersInput {
    params: vrcx_0_contracts::vrchat_requests::UserSearchParams,
}

/// Mirrors cloud's `SearchGroupsInput`.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct SearchGroupsInput {
    params: vrcx_0_contracts::vrchat_requests::GroupSearchParams,
}

// --- remote sync (VRCX's own shared-database sync engine) ---
//
// The command names are `sync__*`, not `app__*`: cloud registers them this way and the frontend
// calls them by that name, so the phone keeps the same surface.

async fn sync__status(
    state: Arc<RuntimeHostState>,
    _args: serde_json::Value,
) -> Result<vrcx_0_contracts::SyncStatusSnapshot, CommandError> {
    Ok(state.remote_sync().status().await)
}

async fn sync__get_connection(
    state: Arc<RuntimeHostState>,
    _args: serde_json::Value,
) -> Result<vrcx_0_contracts::SyncConnectionFields, CommandError> {
    Ok(state.remote_sync().connection_fields())
}

async fn sync__bootstrap_progress(
    state: Arc<RuntimeHostState>,
    _args: serde_json::Value,
) -> Result<vrcx_0_contracts::SyncBootstrapProgress, CommandError> {
    Ok(state
        .remote_sync()
        .current()
        .map(|engine| engine.bootstrap_progress())
        .unwrap_or_default())
}

async fn sync__trigger_now(
    state: Arc<RuntimeHostState>,
    _args: serde_json::Value,
) -> Result<vrcx_0_contracts::SyncStatusSnapshot, CommandError> {
    state
        .remote_sync()
        .run_cycle_now()
        .await
        .map_err(|error| CommandError::failed(error.to_string()))
}

#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct SyncTestConnectionArgs {
    connection: vrcx_0_contracts::SyncConnectionInput,
}

async fn sync__test_connection(
    state: Arc<RuntimeHostState>,
    args: SyncTestConnectionArgs,
) -> Result<vrcx_0_contracts::SyncConnectionTestResult, CommandError> {
    let input = args.connection;
    Ok(state
        .remote_sync()
        .test_connection(
            &input.host,
            input.port,
            &input.user,
            input.password.as_deref(),
            &input.database,
            input.tls_verify,
            input.allow_plaintext,
        )
        .await)
}

async fn app__notification_mark_seen_batch(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application::social::NotificationMarkSeenBatchInput>,
) -> Result<vrcx_0_application::social::NotificationMarkSeenBatchResult, CommandError> {
    state
        .mark_notifications_seen_batch(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))
}
/// Mirrors the facade's file analysis input.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct FileAnalysisArgs {
    #[serde(default)]
    file_id: String,
    #[serde(default)]
    version: i64,
    #[serde(default)]
    variant: String,
}

async fn app__vrchat_auth_file_analysis_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<FileAnalysisArgs>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .file_analysis(input.file_id, input.version, input.variant)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}
/// Cloud reads `fileUrlOrId` as a bare top-level argument.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct FileMetadataGetArgs {
    #[serde(default)]
    file_url_or_id: String,
}

async fn app__file_metadata_get(
    state: Arc<RuntimeHostState>,
    args: FileMetadataGetArgs,
) -> Result<Option<vrcx_0_contracts::FileMetadataOutput>, CommandError> {
    Ok(state.file_metadata_get(args.file_url_or_id).await)
}
/// Mirrors cloud's `VrchatGroupUserGroupsInput`.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct GroupUserGroupsInput {
    #[serde(default)]
    user_id: String,
}

async fn app__vrchat_group_user_groups_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<GroupUserGroupsInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .user_groups(args.input.user_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_user_represented_group_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<GroupUserGroupsInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .user_represented_group(args.input.user_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}
/// Cloud reads `ownerUserId` as a bare top-level argument, not an `{input:{…}}` envelope.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct LocalModerationListArgs {
    #[serde(default)]
    owner_user_id: String,
}

async fn app__local_moderation_list(
    state: Arc<RuntimeHostState>,
    args: LocalModerationListArgs,
) -> Result<Vec<vrcx_0_contracts::local_moderation::LocalModerationOutput>, CommandError> {
    // Same persistence read cloud's local_data service makes: moderation state lives in the synced
    // database, so this is a local read rather than a VRChat request.
    let db = Arc::clone(state.database());
    blocking("local moderation list", move || {
        vrcx_0_persistence::local_moderation::local_moderation_list(
            db.as_ref(),
            vrcx_0_core::OwnerId::new(args.owner_user_id),
        )
    })
    .await
}
/// Mirrors cloud's `SearchWorldsInput`.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct SearchWorldsInput {
    #[serde(default)]
    params: vrcx_0_contracts::vrchat_requests::WorldSearchParams,
    #[serde(default)]
    option: Option<String>,
}

async fn app__vrchat_search_worlds_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<SearchWorldsInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .search_worlds(input.params, input.option)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}
/// Mirrors cloud's `AvatarListByUserInput`.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct AvatarListByUserInput {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    user: String,
    #[serde(default)]
    n: i32,
    #[serde(default)]
    offset: i32,
    sort: vrcx_0_contracts::vrchat_requests::AvatarListSort,
    order: vrcx_0_contracts::vrchat_requests::QueryOrder,
    release_status: vrcx_0_contracts::vrchat_requests::ReleaseStatusFilter,
}

#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct AvatarGetArgs {
    #[serde(default)]
    avatar_id: String,
}

#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct EconomyBalanceArgs {
    #[serde(default)]
    user_id: String,
}

async fn app__vrchat_economy_balance_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<EconomyBalanceArgs>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .economy_balance(args.input.user_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_inventory_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_contracts::vrchat_requests::InventoryListParams>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .inventory_items(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct PrintGetArgs {
    #[serde(default)]
    print_id: String,
}

#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct MediaFilesArgs {
    #[serde(default)]
    params: vrcx_0_contracts::vrchat_requests::MediaFileListParams,
}

async fn app__vrchat_media_files_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<MediaFilesArgs>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .media_files(args.input.params)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct PrintsArgs {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    n: i32,
}

async fn app__vrchat_media_prints_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<PrintsArgs>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .prints(args.input.user_id, args.input.n)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_media_print_get_handler(
    state: Arc<RuntimeHostState>,
    args: InputArgs<PrintGetArgs>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .print(args.input.print_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_avatar_select(
    state: Arc<RuntimeHostState>,
    args: InputArgs<AvatarGetArgs>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .avatar_select(args.input.avatar_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_avatar_select_fallback(
    state: Arc<RuntimeHostState>,
    args: InputArgs<AvatarGetArgs>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .avatar_select_fallback(args.input.avatar_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_avatar_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<AvatarGetArgs>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .avatar_get(args.input.avatar_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_avatar_list_by_user_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<AvatarListByUserInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .avatars_by_user(
            input.user_id,
            input.user,
            vrcx_0_application::remote::AvatarListByUserQuery {
                n: input.n,
                offset: input.offset,
                sort: input.sort,
                order: input.order,
                release_status: input.release_status,
            },
        )
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}
/// Mirrors cloud's `UserGetInput` (only the id matters for the raw read).
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct UserGetInput {
    #[serde(default)]
    user_id: String,
}

/// Mirrors cloud's `UserProfileInput`.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct UserProfileInput {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    as_self: bool,
}

/// Mirrors cloud's `VrchatGroupProfileInput`.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct GroupProfileInput {
    #[serde(default)]
    group_id: String,
    #[serde(default)]
    include_roles: bool,
}

/// Mirrors cloud's `VrchatInstanceIdentityInput`.
#[derive(serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
struct InstanceIdentityInput {
    #[serde(default)]
    world_id: String,
    #[serde(default)]
    instance_id: String,
}

async fn app__vrchat_user_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<UserGetInput>,
) -> Result<serde_json::Value, CommandError> {
    let response = state
        .vrchat_remote()
        .user_get(args.input.user_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_user_profile_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<UserProfileInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .user_profile(input.user_id, input.as_self)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_group_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<GroupProfileInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .group_get(input.group_id, input.include_roles)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
}

async fn app__vrchat_instance_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<InstanceIdentityInput>,
) -> Result<serde_json::Value, CommandError> {
    let input = args.input;
    let response = state
        .vrchat_remote()
        .instance_get(input.world_id, input.instance_id)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(response).map_err(|error| CommandError::failed(error.to_string()))
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

/// Social favourites baseline, same name and `{"input": {…}}` envelope as cloud
/// (`arg(&args, "input")` in cloud/crates/server/src/commands/application_social.rs:517).
///
/// The body is `state.social().favorites_baseline(input)`; the orchestration lives in
/// `vrcx-0-social-runtime`, which is the same code path the desktop host uses.
async fn app__social_favorites_baseline_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application_realtime::SocialFavoritesBaselineInput>,
) -> Result<serde_json::Value, CommandError> {
    let output = state
        .social()
        .favorites_baseline(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(output).map_err(|error| CommandError::failed(error.to_string()))
}

/// Social friend roster baseline, same name and envelope as cloud
/// (cloud/crates/server/src/commands/application_social.rs:524).
async fn app__social_friend_roster_baseline_get(
    state: Arc<RuntimeHostState>,
    args: InputArgs<vrcx_0_application_realtime::SocialFriendRosterBaselineInput>,
) -> Result<serde_json::Value, CommandError> {
    let output = state
        .social()
        .friend_roster_baseline(args.input)
        .await
        .map_err(|error| CommandError::failed(error.to_string()))?;
    serde_json::to_value(output).map_err(|error| CommandError::failed(error.to_string()))
}

/// The command surface exposed to the phone UI.
pub fn build_registry() -> CommandRegistry<RuntimeHostState> {
    let mut registry = CommandRegistry::new();
    registry.register_typed::<serde_json::Value, Vec<vrcx_0_contracts::ConfigReadEntry>, _, _>(
        "app__config_list_values",
        app__config_list_values,
    );
    registry.register_typed::<ConfigRemoveArgs, i64, _, _>(
        "app__config_remove_value",
        app__config_remove_value,
    );
    registry.register_typed::<serde_json::Value, serde_json::Value, _, _>(
        "app__config_set_values",
        app__config_set_values,
    );
    registry.register_typed::<serde_json::Value, serde_json::Value, _, _>(
        "sync__configure",
        sync__configure,
    );
    registry.register_typed::<
        InputArgs<vrcx_0_application_game::GameLogSessionsQueryInput>,
        Vec<vrcx_0_application_game::GameLogSessionDto>,
        _,
        _,
    >("app__game_log_sessions_query", app__game_log_sessions_query);
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
    registry.register_typed::<InputArgs<UserGetInput>, serde_json::Value, _, _>(
        "app__vrchat_user_get",
        app__vrchat_user_get,
    );
    registry.register_typed::<InputArgs<UserProfileInput>, serde_json::Value, _, _>(
        "app__vrchat_user_profile_get",
        app__vrchat_user_profile_get,
    );
    registry.register_typed::<InputArgs<GroupProfileInput>, serde_json::Value, _, _>(
        "app__vrchat_group_get",
        app__vrchat_group_get,
    );
    registry.register_typed::<InputArgs<InstanceIdentityInput>, serde_json::Value, _, _>(
        "app__vrchat_instance_get",
        app__vrchat_instance_get,
    );
    registry.register_typed::<QueryArgs<NotificationListQueryInput>, Vec<NotificationListItemOutput>, _, _>(
        "app__notification_list_query",
        app__notification_list_query,
    );

    registry.register_typed::<serde_json::Value, vrcx_0_contracts::SyncStatusSnapshot, _, _>(
        "sync__status",
        sync__status,
    );
    registry.register_typed::<serde_json::Value, vrcx_0_contracts::SyncConnectionFields, _, _>(
        "sync__get_connection",
        sync__get_connection,
    );
    registry.register_typed::<serde_json::Value, vrcx_0_contracts::SyncBootstrapProgress, _, _>(
        "sync__bootstrap_progress",
        sync__bootstrap_progress,
    );
    registry.register_typed::<serde_json::Value, vrcx_0_contracts::SyncStatusSnapshot, _, _>(
        "sync__trigger_now",
        sync__trigger_now,
    );
    registry.register_typed::<SyncTestConnectionArgs, vrcx_0_contracts::SyncConnectionTestResult, _, _>(
        "sync__test_connection",
        sync__test_connection,
    );
    registry.register_typed::<
        InputArgs<vrcx_0_application::social::NotificationMarkSeenBatchInput>,
        vrcx_0_application::social::NotificationMarkSeenBatchResult,
        _,
        _,
    >("app__notification_mark_seen_batch", app__notification_mark_seen_batch);
    registry.register_typed::<InputArgs<FileAnalysisArgs>, serde_json::Value, _, _>(
        "app__vrchat_auth_file_analysis_get",
        app__vrchat_auth_file_analysis_get,
    );
    registry.register_typed::<
        FileMetadataGetArgs,
        Option<vrcx_0_contracts::FileMetadataOutput>,
        _,
        _,
    >("app__file_metadata_get", app__file_metadata_get);
    registry.register_typed::<InputArgs<GroupUserGroupsInput>, serde_json::Value, _, _>(
        "app__vrchat_group_user_groups_get",
        app__vrchat_group_user_groups_get,
    );
    registry.register_typed::<InputArgs<GroupUserGroupsInput>, serde_json::Value, _, _>(
        "app__vrchat_user_represented_group_get",
        app__vrchat_user_represented_group_get,
    );
    registry.register_typed::<
        LocalModerationListArgs,
        Vec<vrcx_0_contracts::local_moderation::LocalModerationOutput>,
        _,
        _,
    >("app__local_moderation_list", app__local_moderation_list);
    registry.register_typed::<InputArgs<SearchWorldsInput>, serde_json::Value, _, _>(
        "app__vrchat_search_worlds_get",
        app__vrchat_search_worlds_get,
    );
    registry.register_typed::<InputArgs<EconomyBalanceArgs>, serde_json::Value, _, _>(
        "app__vrchat_economy_balance_get",
        app__vrchat_economy_balance_get,
    );
    registry.register_typed::<
        InputArgs<vrcx_0_contracts::vrchat_requests::InventoryListParams>,
        serde_json::Value,
        _,
        _,
    >("app__vrchat_media_inventory_items_get", app__vrchat_inventory_get);
    registry.register_typed::<InputArgs<MediaFilesArgs>, serde_json::Value, _, _>(
        "app__vrchat_media_files_get",
        app__vrchat_media_files_get,
    );
    registry.register_typed::<InputArgs<PrintsArgs>, serde_json::Value, _, _>(
        "app__vrchat_media_prints_get",
        app__vrchat_media_prints_get,
    );
    registry.register_typed::<InputArgs<PrintGetArgs>, serde_json::Value, _, _>(
        "app__vrchat_media_print_get",
        app__vrchat_media_print_get_handler,
    );
    registry.register_typed::<InputArgs<AvatarGetArgs>, serde_json::Value, _, _>(
        "app__vrchat_avatar_select",
        app__vrchat_avatar_select,
    );
    registry.register_typed::<InputArgs<AvatarGetArgs>, serde_json::Value, _, _>(
        "app__vrchat_avatar_select_fallback",
        app__vrchat_avatar_select_fallback,
    );
    registry.register_typed::<InputArgs<AvatarGetArgs>, serde_json::Value, _, _>(
        "app__vrchat_avatar_get",
        app__vrchat_avatar_get,
    );
    registry.register_typed::<InputArgs<AvatarListByUserInput>, serde_json::Value, _, _>(
        "app__vrchat_avatar_list_by_user_get",
        app__vrchat_avatar_list_by_user_get,
    );
    registry.register_typed::<
        InputArgs<vrcx_0_application::favorites::FavoriteDetailsHydrateInput>,
        vrcx_0_application::favorites::FavoriteDetailsHydrateOutput,
        _,
        _,
    >("app__favorite_details_hydrate", app__favorite_details_hydrate);
    registry.register_typed::<WorldGetArgs, vrcx_0_contracts::VrchatResponse, _, _>(
        "app__world_get",
        app__world_get,
    );
    registry.register_typed::<
        InputArgs<vrcx_0_application::social::VrchatGroupGalleryInput>,
        serde_json::Value,
        _,
        _,
    >("app__vrchat_group_gallery_get", app__vrchat_group_gallery_get);
    registry.register_typed::<
        InputArgs<vrcx_0_application::social::VrchatGroupPagedInput>,
        serde_json::Value,
        _,
        _,
    >("app__vrchat_group_posts_get", app__vrchat_group_posts_get);
    registry.register_typed::<
        InputArgs<vrcx_0_application::social::VrchatGroupUserGroupsInput>,
        serde_json::Value,
        _,
        _,
    >(
        "app__vrchat_group_user_instances_get",
        app__vrchat_group_user_instances_get,
    );
    registry.register_typed::<
        InputArgs<vrcx_0_application::social::VrchatGroupMembersInput>,
        serde_json::Value,
        _,
        _,
    >("app__vrchat_group_members_get", app__vrchat_group_members_get);
    registry.register_typed::<
        InputArgs<vrcx_0_application::social::VrchatGroupIdInput>,
        serde_json::Value,
        _,
        _,
    >("app__vrchat_group_join", app__vrchat_group_join);
    registry.register_typed::<
        InputArgs<vrcx_0_application::social::VrchatGroupIdInput>,
        serde_json::Value,
        _,
        _,
    >("app__vrchat_group_leave", app__vrchat_group_leave);
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
    registry.register_typed::<InputArgs<AutoLoginStartInput>, serde_json::Value, _, _>(
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
    registry.register_typed::<
        InputArgs<vrcx_0_application_realtime::SocialFavoritesBaselineInput>,
        serde_json::Value,
        _,
        _,
    >(
        "app__social_favorites_baseline_get",
        app__social_favorites_baseline_get,
    );
    registry.register_typed::<
        InputArgs<vrcx_0_application_realtime::SocialFriendRosterBaselineInput>,
        serde_json::Value,
        _,
        _,
    >(
        "app__social_friend_roster_baseline_get",
        app__social_friend_roster_baseline_get,
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
            "app__social_favorites_baseline_get",
            "app__social_friend_roster_baseline_get",
            "app__notification_list_query",
            "app__world_get",
        ] {
            assert!(registry.contains(name), "missing command: {name}");
        }
        assert_eq!(registry.len(), 61);
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
#[derive(serde::Deserialize, specta::Type)]
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

#[derive(serde::Deserialize, specta::Type)]
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
#[derive(serde::Deserialize, specta::Type)]
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

/// The command surface with the types each command takes and returns.
///
/// Read-only: it builds the same registry `build_bus` does, but without a host state, so it can run
/// anywhere. A binding generator consumes this instead of every caller spelling envelope shapes by
/// hand - the `{"input": …}` / `{"query": …}` split is exactly where the hand-written wrappers went
/// wrong (`app__config_set_values` silently failed because it expected no wrapper at all).
pub fn command_shapes() -> serde_json::Value {
    let registry = build_registry();
    let commands: Vec<serde_json::Value> = registry
        .signatures()
        .into_iter()
        .map(|(name, input, output)| {
            let (envelope, input_type) = shape_of(input);
            serde_json::json!({
                "cmd": name,
                // `type_name` renders as `crate::module::Type`; the leaf is what a generator maps.
                "input": input,
                "output": output,
                "envelope": envelope,
                "inputType": input_type,
                "outputType": leaf(output),
            })
        })
        .collect();
    serde_json::json!({
        "total": registry.len(),
        "withSignature": commands.len(),
        "commands": commands,
    })
}

/// The argument envelope a wrapper type implies, plus the type it carries.
///
/// This is the piece the hand-written Kotlin wrappers had to guess, and guessed wrong:
/// `InputArgs<T>` takes `{"input": {…: T}}`, `QueryArgs<T>` takes `{"query": {…: T}}`, and a bare `T`
/// takes its own fields at the top level. Reading it off the type makes it a fact instead of a guess.
fn shape_of(input: &'static str) -> (&'static str, &'static str) {
    let inner = match (input.find('<'), input.rfind('>')) {
        (Some(open), Some(close)) if close > open => &input[open + 1..close],
        _ => input,
    };
    let envelope = if input.starts_with("vrcx_0_mobile_host::InputArgs<") {
        "input"
    } else if input.starts_with("vrcx_0_mobile_host::QueryArgs<") {
        "query"
    } else {
        "flat"
    };
    (envelope, leaf(inner))
}

fn leaf(type_name: &str) -> &str {
    type_name.rsplit("::").next().unwrap_or(type_name)
}

#[cfg(test)]
mod shapes {
    /// Prints the command surface so it can be diffed against the Kotlin wrappers.
    ///
    /// Run with `cargo test -p vrcx-0-mobile-host dump_command_shapes -- --nocapture`.
    #[test]
    fn dump_command_shapes() {
        let shapes = super::command_shapes();
        println!("{}", serde_json::to_string_pretty(&shapes).expect("serialize"));
    }
}













































