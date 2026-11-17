//! Host-state helpers the phone's commands need, implemented outside `composition`.
//!
//! These used to live on `RuntimeHostState` itself, inside `composition` - a crate that also exists
//! upstream - so every merge had to decide about them. Nothing here belongs to the shared host: they
//! are the phone's own conveniences, written against the public surface (`database()`,
//! `web_client()`, `desktop_assembly()`), and they keep their original names so the 61 command
//! handlers kept compiling unchanged.
//!
//! The methods still in `composition` are the ones that need the host context itself rather than a
//! public accessor: `social()`, `vrchat_remote()` (the facade is wired from internals) and
//! `remote_sync_restart` (it owns the sync host). Those are the documented residue.

use std::sync::Arc;

use vrcx_0_composition::RuntimeHostState;
use vrcx_0_core::vrchat_endpoints::VRCHAT_API_DEFAULT_ENDPOINT;

pub trait HostStateExt {
    async fn config_list_values(
        &self,
    ) -> vrcx_0_application_core::Result<Vec<vrcx_0_contracts::ConfigReadEntry>>;

    async fn config_set_values(
        &self,
        entries: Vec<vrcx_0_contracts::ConfigWriteEntry>,
    ) -> vrcx_0_application_core::Result<()>;

    async fn config_remove_value(&self, key: String) -> vrcx_0_application_core::Result<i64>;

    fn event_bus(&self) -> &vrcx_0_application_core::RuntimeEventBus;

    async fn favorite_add_remote(
        &self,
        input: vrcx_0_application::favorites::FavoriteRemoteAddInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse>;

    async fn favorite_delete_remote(
        &self,
        input: vrcx_0_application::favorites::FavoriteRemoteDeleteInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse>;

    fn favorite_details_runtime(&self) -> vrcx_0_application::favorites::FavoriteDetailsRuntime;

    async fn file_metadata_get(
        &self,
        file_url_or_id: String,
    ) -> Option<vrcx_0_contracts::FileMetadataOutput>;

    async fn group_posts(
        &self,
        input: vrcx_0_application::social::VrchatGroupPagedInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse>;

    async fn group_gallery(
        &self,
        input: vrcx_0_application::social::VrchatGroupGalleryInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse>;

    async fn group_user_instances(
        &self,
        input: vrcx_0_application::social::VrchatGroupUserGroupsInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse>;

    async fn mark_notifications_seen_batch(
        &self,
        input: vrcx_0_application::social::NotificationMarkSeenBatchInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_application::social::NotificationMarkSeenBatchResult>;

    fn require_active_scope(
        &self,
        label: &str,
    ) -> vrcx_0_application_core::Result<vrcx_0_application_core::RuntimeAuthScopeSnapshot>;

    async fn world_get(
        &self,
        world_id: &str,
        force: bool,
        full: bool,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse>;
}

impl HostStateExt for RuntimeHostState {
    async fn config_list_values(
        &self,
    ) -> vrcx_0_application_core::Result<Vec<vrcx_0_contracts::ConfigReadEntry>> {
        let db = Arc::clone(self.database());
        tokio::task::spawn_blocking(move || vrcx_0_persistence::config::config_list_values(&db))
            .await
            .map_err(|error| {
                vrcx_0_application_core::Error::Custom(format!("config list task failed: {error}"))
            })?
            .map_err(|error| vrcx_0_application_core::Error::Custom(error.to_string()))
    }

    async fn config_set_values(
        &self,
        entries: Vec<vrcx_0_contracts::ConfigWriteEntry>,
    ) -> vrcx_0_application_core::Result<()> {
        let db = Arc::clone(self.database());
        tokio::task::spawn_blocking(move || {
            vrcx_0_persistence::config::config_set_values(&db, entries)
        })
        .await
        .map_err(|error| {
            vrcx_0_application_core::Error::Custom(format!("config set task failed: {error}"))
        })?
        .map_err(|error| vrcx_0_application_core::Error::Custom(error.to_string()))
    }

    async fn config_remove_value(&self, key: String) -> vrcx_0_application_core::Result<i64> {
        let db = Arc::clone(self.database());
        tokio::task::spawn_blocking(move || {
            vrcx_0_persistence::config::config_remove_value(&db, key)
        })
        .await
        .map_err(|error| {
            vrcx_0_application_core::Error::Custom(format!("config remove task failed: {error}"))
        })?
        .map_err(|error| vrcx_0_application_core::Error::Custom(error.to_string()))
    }

    fn event_bus(&self) -> &vrcx_0_application_core::RuntimeEventBus {
        self.desktop_assembly().event_bus()
    }

    async fn favorite_add_remote(
        &self,
        input: vrcx_0_application::favorites::FavoriteRemoteAddInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse> {
        self.desktop_assembly()
            .favorite_mutations()
            .add_remote("Remote favorite mutation", input)
            .await
    }

    async fn favorite_delete_remote(
        &self,
        input: vrcx_0_application::favorites::FavoriteRemoteDeleteInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse> {
        self.desktop_assembly()
            .favorite_mutations()
            .delete_remote("Remote favorite mutation", input)
            .await
    }

    fn favorite_details_runtime(&self) -> vrcx_0_application::favorites::FavoriteDetailsRuntime {
        let assembly = self.desktop_assembly();
        vrcx_0_application::favorites::FavoriteDetailsRuntime::new(
            Arc::new(vrcx_0_outbound_adapters::VrchatFavoriteRemote::new(
                Arc::clone(self.web_client()),
                assembly.diagnostics().clone(),
                assembly.sync().clone(),
                Arc::clone(assembly.world_cache()),
            )),
            assembly.auth_scope().clone(),
            Arc::clone(assembly.world_cache()),
            Arc::clone(assembly.avatar_cache()),
            assembly.tasks().clone(),
        )
    }

    async fn file_metadata_get(
        &self,
        file_url_or_id: String,
    ) -> Option<vrcx_0_contracts::FileMetadataOutput> {
        let file_id = vrcx_0_core::files::extract_file_id(&file_url_or_id)?;
        let auth_scope = self.desktop_assembly().auth_scope().snapshot();
        self.desktop_assembly()
            .file_cache()
            .resolve(&auth_scope.endpoint, &file_id)
            .await
    }

    async fn group_posts(
        &self,
        input: vrcx_0_application::social::VrchatGroupPagedInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse> {
        vrcx_0_application::social::get_posts(crate::dialog::group_api_deps(self), input).await
    }

    async fn group_gallery(
        &self,
        input: vrcx_0_application::social::VrchatGroupGalleryInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse> {
        vrcx_0_application::social::get_gallery(crate::dialog::group_api_deps(self), input).await
    }

    async fn group_user_instances(
        &self,
        input: vrcx_0_application::social::VrchatGroupUserGroupsInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse> {
        vrcx_0_application::social::get_user_instances(crate::dialog::group_api_deps(self), input)
            .await
    }

    async fn mark_notifications_seen_batch(
        &self,
        input: vrcx_0_application::social::NotificationMarkSeenBatchInput,
    ) -> vrcx_0_application_core::Result<vrcx_0_application::social::NotificationMarkSeenBatchResult>
    {
        let assembly = self.desktop_assembly();
        let mutation = vrcx_0_application_core::AuthenticatedMutationContext::capture(
            assembly.auth_scope(),
            assembly.remote_mutations(),
            "Batch action",
        )?;
        let actions = vrcx_0_outbound_adapters::LocalNotificationMarkSeenActions::new(
            self.database().as_ref(),
            self.web_client().as_ref(),
            mutation,
        );
        vrcx_0_application::social::mark_notifications_seen_batch(&actions, input).await
    }

    fn require_active_scope(
        &self,
        label: &str,
    ) -> vrcx_0_application_core::Result<vrcx_0_application_core::RuntimeAuthScopeSnapshot> {
        let scope = self.desktop_assembly().auth_scope().snapshot();
        if scope.active && !scope.current_user_id.trim().is_empty() {
            Ok(scope)
        } else {
            Err(vrcx_0_application_core::Error::Custom(format!(
                "{label} requires an authenticated session."
            )))
        }
    }

    async fn world_get(
        &self,
        world_id: &str,
        force: bool,
        full: bool,
    ) -> vrcx_0_application_core::Result<vrcx_0_contracts::vrchat_api::VrchatResponse> {
        let assembly = self.desktop_assembly();
        let auth_scope = assembly.auth_scope().snapshot();
        let endpoint = if auth_scope.endpoint.is_empty() {
            VRCHAT_API_DEFAULT_ENDPOINT
        } else {
            auth_scope.endpoint.as_str()
        };
        assembly
            .world_cache()
            .get(endpoint, world_id, force, full)
            .await
    }
}
