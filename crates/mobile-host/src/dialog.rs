//! User-dialog reads: the tab badges and the groups overview.
//!
//! Both commands exist upstream for the desktop's user dialog, and both are thin orchestrations over
//! pieces that are constructible from the public host surface - no `local_data()`, and no new accessor
//! on `composition`:
//!
//! * tab counts = `LocalUserDialogTabCountsSource::new(db, web)` + `UserDialogTabCountsRuntime::new()`
//! * groups overview = `UserGroupsOverviewDeps { groups, auth_scope }`, the same deps the phone's other
//!   group commands build (`VrchatRequestAdapter` + diagnostics + sync gate + auth scope + mutation gate)

use std::sync::Arc;

use vrcx_0_composition::RuntimeHostState;

/// The same `GroupApiDeps` the phone's group commands build inside `composition`.
pub(crate) fn group_api_deps(state: &RuntimeHostState) -> vrcx_0_application::social::GroupApiDeps {
    let assembly = state.desktop_assembly();
    vrcx_0_application::social::GroupApiDeps::new(
        Arc::new(vrcx_0_outbound_adapters::VrchatRequestAdapter::new(
            Arc::clone(state.web_client()),
        )),
        assembly.diagnostics().clone(),
        assembly.sync().clone(),
        assembly.auth_scope().clone(),
        arc_remote_mutations(state),
    )
}

fn arc_remote_mutations(
    state: &RuntimeHostState,
) -> Arc<vrcx_0_application_core::RemoteMutationGate> {
    Arc::clone(state.desktop_assembly().remote_mutations())
}

/// `app__user_dialog_tab_counts_get`: the 群组 / 创建的世界 / 原创模型 badges on the user dialog.
pub async fn user_dialog_tab_counts_query(
    state: &RuntimeHostState,
    input: vrcx_0_application::social::UserDialogTabCountsInput,
) -> vrcx_0_application_core::Result<vrcx_0_application::social::UserDialogTabCountsOutput> {
    // One runtime for the process: it holds the TTL cache the desktop also keeps as a field.
    static RUNTIME: std::sync::OnceLock<
        Arc<vrcx_0_application::social::UserDialogTabCountsRuntime>,
    > = std::sync::OnceLock::new();
    let runtime = RUNTIME
        .get_or_init(|| Arc::new(vrcx_0_application::social::UserDialogTabCountsRuntime::new()))
        .clone();
    let source = Arc::new(vrcx_0_outbound_adapters::LocalUserDialogTabCountsSource::new(
        Arc::clone(state.database()),
        Arc::clone(state.web_client()),
    ));
    vrcx_0_application::social::get_user_dialog_tab_counts(
        runtime.as_ref(),
        vrcx_0_application::social::UserDialogTabCountsDeps::new(
            source,
            state.desktop_assembly().auth_scope().clone(),
        ),
        input,
    )
    .await
}

/// `app__user_groups_overview_get`: the 群组 tab of the user dialog.
pub async fn user_groups_overview_query(
    state: &RuntimeHostState,
    input: vrcx_0_application::social::UserGroupsOverviewInput,
) -> vrcx_0_application_core::Result<vrcx_0_application::social::UserGroupsOverviewOutput> {
    vrcx_0_application::social::get_user_groups_overview(
        vrcx_0_application::social::UserGroupsOverviewDeps {
            groups: group_api_deps(state),
            auth_scope: state.desktop_assembly().auth_scope().clone(),
        },
        input,
    )
    .await
}

/// `app__avatar_history_list`: recently worn/seen avatars for one account.
pub fn avatar_history_list(
    state: &RuntimeHostState,
    user_id: String,
    limit: i64,
) -> vrcx_0_persistence::Result<Vec<vrcx_0_contracts::AvatarCacheOutput>> {
    vrcx_0_persistence::avatars::avatar_history_list(state.database().as_ref(), user_id, limit)
        .map_err(Into::into)
}

/// `app__avatar_tags_distinct`: every tag string in use, for the tag filter.
pub fn avatar_tags_distinct(
    state: &RuntimeHostState,
) -> vrcx_0_persistence::Result<Vec<String>> {
    vrcx_0_persistence::avatars::avatar_tags_distinct(state.database().as_ref()).map_err(Into::into)
}

/// `app__my_avatars_get`: the signed-in account's own avatars.
///
/// Desktop builds this from `local_data()`; the pieces it needs are public: a persistence-backed store
/// (`LocalAvatarApplicationAdapter`), a remote (`VrchatAvatarRemote` over web + diagnostics + sync) and
/// the auth scope, plus the expected scope so a session switch mid-request is refused.
pub async fn my_avatars(
    state: &RuntimeHostState,
    input: vrcx_0_application::avatars::MyAvatarsInput,
) -> vrcx_0_application_core::Result<Vec<serde_json::Value>> {
    let expected_scope = state.desktop_assembly().auth_scope().snapshot();
    if !expected_scope.active || expected_scope.current_user_id.trim().is_empty() {
        return Err(vrcx_0_application_core::Error::Custom(
            "My avatars query requires an authenticated session.".into(),
        ));
    }
    let assembly = state.desktop_assembly();
    let store = vrcx_0_outbound_adapters::LocalAvatarApplicationAdapter::new(Arc::clone(
        state.database(),
    ));
    let remote = vrcx_0_outbound_adapters::VrchatAvatarRemote::new(
        Arc::clone(state.web_client()),
        assembly.diagnostics().clone(),
        assembly.sync().clone(),
    );
    vrcx_0_application::avatars::get_my_avatars(
        &vrcx_0_application::avatars::MyAvatarsDeps::new(
            &store,
            &remote,
            assembly.auth_scope(),
            expected_scope,
        ),
        input,
    )
    .await
}

/// `app__my_avatar_by_id_get`: one of the signed-in account's avatars, by id.
pub async fn my_avatar_by_id(
    state: &RuntimeHostState,
    input: vrcx_0_application::avatars::MyAvatarByIdInput,
) -> vrcx_0_application_core::Result<Option<serde_json::Value>> {
    let expected_scope = state.desktop_assembly().auth_scope().snapshot();
    if !expected_scope.active || expected_scope.current_user_id.trim().is_empty() {
        return Err(vrcx_0_application_core::Error::Custom(
            "My avatars query requires an authenticated session.".into(),
        ));
    }
    let assembly = state.desktop_assembly();
    let store = vrcx_0_outbound_adapters::LocalAvatarApplicationAdapter::new(Arc::clone(
        state.database(),
    ));
    let remote = vrcx_0_outbound_adapters::VrchatAvatarRemote::new(
        Arc::clone(state.web_client()),
        assembly.diagnostics().clone(),
        assembly.sync().clone(),
    );
    vrcx_0_application::avatars::get_my_avatar_by_id(
        &vrcx_0_application::avatars::MyAvatarsDeps::new(
            &store,
            &remote,
            assembly.auth_scope(),
            expected_scope,
        ),
        input,
    )
    .await
}
