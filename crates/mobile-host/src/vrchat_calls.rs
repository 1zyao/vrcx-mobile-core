//! The VRChat reads that `VrchatRemoteFacade` did not have upstream.
//!
//! These nine calls used to sit inside `application/src/remote/vrchat_facade.rs`, which is a crate that
//! also exists upstream - 165 lines of additions that every merge had to carry. They are all the same
//! shape: build a request from `vrcx_0_vrchat_protocol`, then hand it to the facade's executor. The
//! facade keeps one public entrance ([`VrchatRemoteFacade::execute`]) and these become the phone's
//! business, so the shared crate gains a single `pub` keyword instead of nine methods.

use vrcx_0_composition::RuntimeHostState;
use vrcx_0_contracts::vrchat_api::{VrchatResponse, VrchatScope};
use vrcx_0_core::vrchat_endpoints::VRCHAT_API_DEFAULT_ENDPOINT;

type Result<T> = vrcx_0_application_core::Result<T>;

async fn run(
    state: &RuntimeHostState,
    command: &str,
    detail: String,
    request: vrcx_0_contracts::vrchat_api::VrchatRequest,
) -> Result<VrchatResponse> {
    state
        .vrchat_remote()
        .execute(command, detail, request, VrchatScope::Vrchat)
        .await
}

/// `GET users/{id}` (`app__vrchat_user_get`).
pub async fn user_get(state: &RuntimeHostState, user_id: String) -> Result<VrchatResponse> {
    let (user_id, request) = vrcx_0_vrchat_protocol::users::user_get_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        user_id,
    )?;
    run(state, "app__vrchat_user_get", format!("Getting user {user_id}."), request).await
}

/// `GET groups/{id}` (`app__vrchat_group_get`).
pub async fn group_get(
    state: &RuntimeHostState,
    group_id: String,
    include_roles: bool,
) -> Result<VrchatResponse> {
    let (group_id, request) = vrcx_0_vrchat_protocol::groups::profile_get_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        group_id,
        include_roles,
    )?;
    run(state, "app__vrchat_group_get", format!("Getting group {group_id}."), request).await
}

/// `GET groups/{id}/members` (`app__vrchat_group_members_get`).
pub async fn group_members(
    state: &RuntimeHostState,
    group_id: String,
    n: i32,
    offset: i32,
    sort: vrcx_0_application::social::GroupMemberSort,
    role_id: String,
) -> Result<VrchatResponse> {
    let (group_id, request) =
        vrcx_0_vrchat_protocol::groups::members_get_input(group_id, n, offset, sort, role_id)?;
    run(
        state,
        "app__vrchat_group_members_get",
        format!("Getting members of group {group_id}."),
        request,
    )
    .await
}

/// `POST groups/{id}/join` (`app__vrchat_group_join`).
pub async fn group_join(state: &RuntimeHostState, group_id: String) -> Result<VrchatResponse> {
    let (group_id, request) = vrcx_0_vrchat_protocol::groups::join_input(group_id)?;
    run(state, "app__vrchat_group_join", format!("Joining group {group_id}."), request).await
}

/// `POST groups/{id}/leave` (`app__vrchat_group_leave`).
pub async fn group_leave(state: &RuntimeHostState, group_id: String) -> Result<VrchatResponse> {
    let (group_id, request) = vrcx_0_vrchat_protocol::groups::leave_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        group_id,
    )?;
    run(state, "app__vrchat_group_leave", format!("Leaving group {group_id}."), request).await
}

/// `GET users/{id}/groups` (`app__vrchat_group_user_groups_get`).
pub async fn user_groups(state: &RuntimeHostState, user_id: String) -> Result<VrchatResponse> {
    let (user_id, request) = vrcx_0_vrchat_protocol::groups::user_groups_get_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        user_id,
    )?;
    run(
        state,
        "app__vrchat_group_user_groups_get",
        format!("Getting groups for user {user_id}."),
        request,
    )
    .await
}

/// `PUT users/{id}/avatar` (`app__vrchat_avatar_select`).
pub async fn avatar_select(state: &RuntimeHostState, avatar_id: String) -> Result<VrchatResponse> {
    let (avatar_id, request) = vrcx_0_vrchat_protocol::avatars::avatar_select_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        avatar_id,
    )?;
    run(state, "app__vrchat_avatar_select", format!("Selecting avatar {avatar_id}."), request).await
}

/// `PUT users/{id}/avatar/fallback` (`app__vrchat_avatar_select_fallback`).
pub async fn avatar_select_fallback(
    state: &RuntimeHostState,
    avatar_id: String,
) -> Result<VrchatResponse> {
    let (avatar_id, request) = vrcx_0_vrchat_protocol::avatars::avatar_select_fallback_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        avatar_id,
    )?;
    run(
        state,
        "app__vrchat_avatar_select_fallback",
        format!("Selecting fallback avatar {avatar_id}."),
        request,
    )
    .await
}

/// `GET avatars/{id}` (`app__avatar_get`).
pub async fn avatar_get(state: &RuntimeHostState, avatar_id: String) -> Result<VrchatResponse> {
    let (avatar_id, request) = vrcx_0_vrchat_protocol::avatars::avatar_get_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        avatar_id,
    )?;
    run(state, "app__avatar_get", format!("Getting avatar {avatar_id}."), request).await
}

/// `GET auth/user/avatarmoderations` (`app__vrchat_avatar_moderations_get`).
///
/// Avatar moderation has no `avatars()` accessor on the host, and no facade method, so these three go
/// through the facade's public `execute` the same way the nine reads above do - the protocol builders
/// already exist upstream.
pub async fn avatar_moderations_get(
    state: &RuntimeHostState,
) -> Result<VrchatResponse> {
    let request =
        vrcx_0_vrchat_protocol::avatars::avatar_moderations_get_input(VRCHAT_API_DEFAULT_ENDPOINT.into());
    run(state, "app__vrchat_avatar_moderations_get", "Getting avatar moderations.".to_string(), request).await
}

/// `POST auth/user/avatarmoderations` (`app__vrchat_avatar_moderation_send`).
pub async fn avatar_moderation_send(
    state: &RuntimeHostState,
    avatar_id: String,
) -> Result<VrchatResponse> {
    let (avatar_id, request) = vrcx_0_vrchat_protocol::avatars::avatar_moderation_send_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        avatar_id,
    )?;
    run(
        state,
        "app__vrchat_avatar_moderation_send",
        format!("Blocking avatar {avatar_id}."),
        request,
    )
    .await
}

/// `DELETE auth/user/avatarmoderations` (`app__vrchat_avatar_moderation_delete`).
pub async fn avatar_moderation_delete(
    state: &RuntimeHostState,
    avatar_id: String,
) -> Result<VrchatResponse> {
    let (avatar_id, request) = vrcx_0_vrchat_protocol::avatars::avatar_moderation_delete_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        avatar_id,
    )?;
    run(
        state,
        "app__vrchat_avatar_moderation_delete",
        format!("Unblocking avatar {avatar_id}."),
        request,
    )
    .await
}
/// `DELETE worlds/{id}` (`app__vrchat_world_delete`).
pub async fn world_delete(state: &RuntimeHostState, world_id: String) -> Result<VrchatResponse> {
    let (world_id, request) =
        vrcx_0_vrchat_protocol::worlds::world_delete_input(VRCHAT_API_DEFAULT_ENDPOINT.into(), world_id)?;
    run(state, "app__vrchat_world_delete", format!("Deleting world {world_id}."), request).await
}

/// `PUT worlds/{id}/publish` (`app__vrchat_world_publish`).
pub async fn world_publish(state: &RuntimeHostState, world_id: String) -> Result<VrchatResponse> {
    let (world_id, request) =
        vrcx_0_vrchat_protocol::worlds::world_publish_input(VRCHAT_API_DEFAULT_ENDPOINT.into(), world_id)?;
    run(state, "app__vrchat_world_publish", format!("Publishing world {world_id}."), request).await
}

/// `DELETE worlds/{id}/publish` (`app__vrchat_world_unpublish`).
pub async fn world_unpublish(state: &RuntimeHostState, world_id: String) -> Result<VrchatResponse> {
    let (world_id, request) =
        vrcx_0_vrchat_protocol::worlds::world_unpublish_input(VRCHAT_API_DEFAULT_ENDPOINT.into(), world_id)?;
    run(state, "app__vrchat_world_unpublish", format!("Unpublishing world {world_id}."), request).await
}

/// `PUT groups/{id}/representation` (`app__vrchat_group_representation_set`).
pub async fn group_representation_set(
    state: &RuntimeHostState,
    group_id: String,
    is_representing: bool,
) -> Result<VrchatResponse> {
    let (group_id, request) =
        vrcx_0_vrchat_protocol::groups::representation_set_input(group_id, is_representing)?;
    run(
        state,
        "app__vrchat_group_representation_set",
        format!("Setting representation for group {group_id}."),
        request,
    )
    .await
}
/// `PUT worlds/{id}` (`app__vrchat_world_save`).
pub async fn world_save(
    state: &RuntimeHostState,
    world_id: String,
    params: vrcx_0_contracts::vrchat_requests::WorldUpdateRequest,
) -> Result<VrchatResponse> {
    let (world_id, request) =
        vrcx_0_vrchat_protocol::worlds::world_save_input(VRCHAT_API_DEFAULT_ENDPOINT.into(), world_id, params)?;
    run(state, "app__vrchat_world_save", format!("Saving world {world_id}."), request).await
}
/// `POST users/{id}/boop` (`app__vrchat_boop_send`).
pub async fn boop_send(
    state: &RuntimeHostState,
    user_id: String,
    emoji_id: String,
    inventory_item_id: String,
) -> Result<VrchatResponse> {
    let (user_id, request) = vrcx_0_vrchat_protocol::notifications::boop_send_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        user_id,
        emoji_id,
        inventory_item_id,
    )?;
    run(state, "app__vrchat_boop_send", format!("Booping {user_id}."), request).await
}

/// `POST user/{id}/note` (`app__vrchat_tools_user_note_save`).
pub async fn user_note_save(
    state: &RuntimeHostState,
    target_user_id: String,
    note: String,
) -> Result<VrchatResponse> {
    let (target_user_id, request) = vrcx_0_vrchat_protocol::tools::user_note_save_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        target_user_id,
        note,
    )?;
    run(
        state,
        "app__vrchat_tools_user_note_save",
        format!("Saving note for {target_user_id}."),
        request,
    )
    .await
}

/// `GET users/{userId}/{worldId}/persist/exists` (`app__vrchat_world_persistent_data_exists`).
pub async fn world_persistent_data_exists(
    state: &RuntimeHostState,
    user_id: String,
    world_id: String,
) -> Result<VrchatResponse> {
    let (user_id, world_id, request) =
        vrcx_0_vrchat_protocol::worlds::world_persistent_data_exists_input(
            VRCHAT_API_DEFAULT_ENDPOINT.into(),
            user_id,
            world_id,
        )?;
    run(
        state,
        "app__vrchat_world_persistent_data_exists",
        format!("Checking persistence for {user_id} in {world_id}."),
        request,
    )
    .await
}

/// `DELETE users/{userId}/{worldId}/persist` (`app__vrchat_world_persistent_data_delete`).
pub async fn world_persistent_data_delete(
    state: &RuntimeHostState,
    user_id: String,
    world_id: String,
) -> Result<VrchatResponse> {
    let (user_id, world_id, request) =
        vrcx_0_vrchat_protocol::worlds::world_persistent_data_delete_input(
            VRCHAT_API_DEFAULT_ENDPOINT.into(),
            user_id,
            world_id,
        )?;
    run(
        state,
        "app__vrchat_world_persistent_data_delete",
        format!("Deleting persistence for {user_id} in {world_id}."),
        request,
    )
    .await
}
/// `DELETE instances/{location}` (`app__vrchat_instance_close`).
pub async fn instance_close(
    state: &RuntimeHostState,
    location: String,
    hard_close: bool,
) -> Result<VrchatResponse> {
    let (location, request) = vrcx_0_vrchat_protocol::instances::instance_close_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        location,
        hard_close,
    )?;
    run(state, "app__vrchat_instance_close", format!("Closing instance {location}."), request).await
}

/// `POST instances` (`app__vrchat_instance_create`).
pub async fn instance_create(
    state: &RuntimeHostState,
    params: vrcx_0_contracts::vrchat_requests::InstanceCreateRequest,
) -> Result<VrchatResponse> {
    let request = vrcx_0_vrchat_protocol::instances::instance_create_input(
        VRCHAT_API_DEFAULT_ENDPOINT.into(),
        params,
    )?;
    run(state, "app__vrchat_instance_create", "Creating instance.".to_string(), request).await
}