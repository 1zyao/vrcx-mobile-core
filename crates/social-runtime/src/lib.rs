//! Host-neutral social runtime: the two social baseline reads.
//!
//! These are the implementations behind the canonical command names
//! `app__social_favorites_baseline_get` and `app__social_friend_roster_baseline_get`,
//! whose argument envelope is `{"input": {…}}` on every host.
//!
//! Ported from cloud's `ServerSocialRuntime`
//! (`cloud/crates/runtime-host-server/src/social.rs:193` and `:251`), and identical in
//! behaviour to the desktop host's `DesktopSocialRuntime::favorites_baseline` /
//! `friend_roster_baseline` (`crates/runtime-host-desktop/src/social.rs:132` / `:191`).
//! Only the baseline half is moved: cloud's struct carries 14 fields because it also
//! serves moderation sync, friend mutations and notification actions, none of which
//! either baseline read touches.
//!
//! The two reads need exactly three collaborators, all already assembled by the
//! composition root:
//!
//! * [`AuthenticatedRuntimeOrchestrator`] owns the `SocialBaselineDeps` (realtime
//!   stores, config store, `VrchatRealtimeRemoteRequests`, `WebClient`,
//!   `RuntimeAuthScope`), so they are never re-derived here, and it also receives the
//!   refreshed favourites snapshot through `update_favorites_baseline`, which is what
//!   makes `app__social_favorites_baseline_get` feed the authenticated phase instead
//!   of just returning bytes.
//! * [`RealtimeHostRuntime`]: `build_synced_friend_roster_baseline` projects the remote
//!   roster through it (`sync_friend_snapshot_with_watermark`), so a live presence
//!   delta that arrives during the fetch is not overwritten by the baseline.
//! * `RuntimeDiagnostics` / `RuntimeSyncEngine`: the same command and status
//!   bookkeeping cloud records, so these commands show up in diagnostics/sync output
//!   with a real status instead of silently succeeding.

use std::sync::Arc;

use vrcx_0_application::social::AuthenticatedRuntimeOrchestrator;
use vrcx_0_application_core::{
    Result, RuntimeDiagnostics, RuntimeOperationStatus, RuntimeSyncEngine,
};
use vrcx_0_application_realtime::{
    build_favorites_baseline, build_synced_friend_roster_baseline, RealtimeHostRuntime,
    SocialFavoritesBaselineInput, SocialFavoritesBaselineOutput, SocialFriendRosterBaselineInput,
    SocialFriendRosterBaselineOutput,
};

/// Social reads shared by every host (desktop, headless, phone core).
#[derive(Clone)]
pub struct SocialRuntime {
    authenticated: AuthenticatedRuntimeOrchestrator,
    realtime: Arc<RealtimeHostRuntime>,
    diagnostics: RuntimeDiagnostics,
    sync: RuntimeSyncEngine,
}

impl SocialRuntime {
    pub fn new(
        authenticated: AuthenticatedRuntimeOrchestrator,
        realtime: Arc<RealtimeHostRuntime>,
        diagnostics: RuntimeDiagnostics,
        sync: RuntimeSyncEngine,
    ) -> Self {
        Self {
            authenticated,
            realtime,
            diagnostics,
            sync,
        }
    }

    /// `app__social_favorites_baseline_get`.
    ///
    /// Reads `auth/user/favoritelimits`, the paged favourites and the favourite
    /// groups, merges the local favourites/config, and returns
    /// `{ userId, stale, count, snapshot }` where `snapshot.favoriteLimits` is the raw
    /// limits payload and `snapshot.favorite{Friend,World,Avatar}Groups` carry
    /// `capacity`/`count` per group.
    pub async fn favorites_baseline(
        &self,
        input: SocialFavoritesBaselineInput,
    ) -> Result<SocialFavoritesBaselineOutput> {
        let command = "app__social_favorites_baseline_get";
        self.diagnostics.record_command(
            command,
            RuntimeOperationStatus::Running,
            "Favorites baseline started.",
        );
        let result =
            build_favorites_baseline(self.authenticated.social_baseline_deps(), input).await;
        match &result {
            Ok(output) => {
                self.authenticated.update_favorites_baseline(output.clone());
                let status = if output.stale {
                    RuntimeOperationStatus::Stale
                } else {
                    RuntimeOperationStatus::Ok
                };
                let sync_status = if output.stale {
                    RuntimeOperationStatus::Stale
                } else {
                    RuntimeOperationStatus::Ready
                };
                self.diagnostics.record_command(
                    command,
                    status,
                    format!(
                        "user={} stale={} count={}",
                        output.user_id, output.stale, output.count
                    ),
                );
                self.sync.record(
                    "favorites",
                    sync_status,
                    if output.stale {
                        format!(
                            "Favorites baseline skipped stale request for {}.",
                            output.user_id
                        )
                    } else {
                        format!("Favorites baseline loaded for {}.", output.user_id)
                    },
                    0,
                );
            }
            Err(error) => {
                self.diagnostics.record_command(
                    command,
                    RuntimeOperationStatus::Error,
                    error.to_string(),
                );
                self.sync.record_failure("favorites", error.to_string());
            }
        }
        Ok(result?)
    }

    /// `app__social_friend_roster_baseline_get`.
    ///
    /// Reads the paged `auth/user/friends` roster and hands it to the realtime runtime
    /// as a baseline, so the returned snapshot is the projection (remote roster folded
    /// into whatever live deltas already exist), not the raw HTTP body.
    pub async fn friend_roster_baseline(
        &self,
        input: SocialFriendRosterBaselineInput,
    ) -> Result<SocialFriendRosterBaselineOutput> {
        let command = "app__social_friend_roster_baseline_get";
        self.diagnostics.record_command(
            command,
            RuntimeOperationStatus::Running,
            "Friend roster baseline started.",
        );
        let result = build_synced_friend_roster_baseline(
            self.authenticated.social_baseline_deps(),
            &self.realtime,
            input,
        )
        .await
        .map(|baseline| baseline.output);
        match &result {
            Ok(output) => {
                let status = if output.stale {
                    RuntimeOperationStatus::Stale
                } else {
                    RuntimeOperationStatus::Ok
                };
                let sync_status = if output.stale {
                    RuntimeOperationStatus::Stale
                } else {
                    RuntimeOperationStatus::Ready
                };
                self.diagnostics.record_command(
                    command,
                    status,
                    format!(
                        "user={} stale={} count={}",
                        output.user_id, output.stale, output.count
                    ),
                );
                self.sync.record(
                    "friends",
                    sync_status,
                    if output.stale {
                        format!(
                            "Friend roster baseline skipped stale request for {}.",
                            output.user_id
                        )
                    } else {
                        format!("Friend roster baseline loaded for {}.", output.user_id)
                    },
                    0,
                );
            }
            Err(error) => {
                self.diagnostics.record_command(
                    command,
                    RuntimeOperationStatus::Error,
                    error.to_string(),
                );
                self.sync.record_failure("friends", error.to_string());
            }
        }
        Ok(result?)
    }
}
