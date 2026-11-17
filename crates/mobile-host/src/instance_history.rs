//! Room history, the same read the desktop's 房间历史 page performs.
//!
//! `app__instance_history_query` is the upstream command behind that page, and the query itself lives
//! in `application-game::instance_history_query` - portable, over an `InstanceHistoryQueries` store.
//! The desktop implements that store in `runtime-host-desktop::game_state_store`; the phone
//! implements it here over the same four columns, so the phone and the desktop show one history read
//! from the same rows instead of two different readings.
//!
//! This replaces the phone's earlier `app__game_log_sessions_query` usage: sessions answered "which
//! stretch of time did I spend somewhere", while the desktop's page answers "which rooms did I visit",
//! and only the second carries the access type and the per-day grouping the page shows.

use std::sync::Arc;

use vrcx_0_application_game::{
    instance_history_query, InstanceHistoryEntryOutput, InstanceHistoryQueries,
    InstanceHistoryQueryInput,
};
use vrcx_0_composition::RuntimeHostState;
use vrcx_0_contracts::game_log::PreviousInstanceEventRow;
use vrcx_0_core::OwnerId;
use vrcx_0_persistence::{game_log, DatabaseService};

struct PersistenceInstanceHistoryStore {
    db: Arc<DatabaseService>,
}

impl InstanceHistoryQueries for PersistenceInstanceHistoryStore {
    fn previous_instance_events(
        &self,
        owner: &OwnerId,
        user_id: &str,
        date_from: &str,
        date_to: &str,
        limit: usize,
    ) -> vrcx_0_application_core::Result<Vec<PreviousInstanceEventRow>> {
        Ok(game_log::previous_instance_event_rows_query(
            self.db.as_ref(),
            owner,
            user_id,
            date_from,
            date_to,
            limit,
        )?)
    }
}

/// `app__instance_history_query`: the rooms this account visited, newest first.
pub async fn instance_history_query_command(
    state: &RuntimeHostState,
    input: InstanceHistoryQueryInput,
) -> vrcx_0_application_core::Result<Vec<InstanceHistoryEntryOutput>> {
    let store = PersistenceInstanceHistoryStore {
        db: Arc::clone(state.database()),
    };
    let owner = OwnerId::new(
        state
            .desktop_assembly()
            .auth_scope()
            .snapshot()
            .current_user_id,
    );
    tokio::task::spawn_blocking(move || instance_history_query(&store, &owner, input))
        .await
        .map_err(|error| {
            vrcx_0_application_core::Error::Custom(format!("instance history task failed: {error}"))
        })?
}
