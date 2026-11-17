//! Reads of VRCX's own game log, for hosts that need them without the desktop store.
//!
//! The desktop host already implements `GameLogSessionQueries` over the same four persistence
//! calls (`runtime-host-desktop/src/game_state_store.rs`). That adapter is `pub(crate)` to the
//! desktop host, and the phone builds `RuntimeHostState` instead, so the delegations are repeated
//! here. They are one-liners over `vrcx_0_persistence::game_log`, so the duplication is in the
//! wiring, not in logic - if the trait grows, both adapters must grow with it.

use std::sync::Arc;

use vrcx_0_application_game::GameLogSessionQueries;
use vrcx_0_contracts::game_log::{
    SessionEventRow, SessionLocationSegmentRow, SessionPlayerDurationRow,
};
use vrcx_0_core::OwnerId;
use vrcx_0_persistence::{game_log, DatabaseService};

/// `GameLogSessionQueries` over the phone's database.
pub(crate) struct PersistenceSessionStore {
    db: Arc<DatabaseService>,
}

impl PersistenceSessionStore {
    pub(crate) fn new(db: Arc<DatabaseService>) -> Self {
        Self { db }
    }
}

impl GameLogSessionQueries for PersistenceSessionStore {
    fn session_location_segments(
        &self,
        owner: &OwnerId,
        before_id: Option<i64>,
        limit: i64,
    ) -> vrcx_0_application_core::Result<Vec<SessionLocationSegmentRow>> {
        Ok(game_log::get_session_location_segments(
            self.db.as_ref(),
            owner,
            before_id,
            limit,
        )?)
    }

    fn session_location_segments_by_date_range(
        &self,
        owner: &OwnerId,
        after_date: &str,
        before_date: &str,
        limit: i64,
    ) -> vrcx_0_application_core::Result<Vec<SessionLocationSegmentRow>> {
        Ok(game_log::get_session_location_segments_by_date_range(
            self.db.as_ref(),
            owner,
            after_date,
            before_date,
            limit,
        )?)
    }

    fn session_events_for_range(
        &self,
        owner: &OwnerId,
        after_date: &str,
        before_date: &str,
    ) -> vrcx_0_application_core::Result<Vec<SessionEventRow>> {
        Ok(game_log::get_session_events_for_range(
            self.db.as_ref(),
            owner,
            after_date,
            before_date,
        )?)
    }

    fn session_player_duration_rows(
        &self,
        owner: &OwnerId,
        locations: &[String],
    ) -> vrcx_0_application_core::Result<Vec<SessionPlayerDurationRow>> {
        Ok(game_log::get_session_player_duration_rows(
            self.db.as_ref(),
            owner,
            locations,
        )?)
    }
}


