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
fn group_api_deps(state: &RuntimeHostState) -> vrcx_0_application::social::GroupApiDeps {
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
