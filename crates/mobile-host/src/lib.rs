//! Host state and command set for the in-process mobile core.
//!
//! The phone runs the VRCX-0 core as a library inside its own process, so the
//! host state here is deliberately minimal: a local SQLite database plus the
//! event bus. Domain runtimes (avatars, groups, sync, …) get added as the
//! screens that need them are ported — each command is written once as a plain
//! async fn and registered through `register_typed`, so no second adapter copy
//! of the command surface appears.
//!
//! SQLite work runs on a blocking thread because `persistence` is synchronous;
//! this mirrors what the desktop shell does with `spawn_blocking`.

use std::path::Path;
use std::sync::Arc;

use vrcx_0_command_server::{CommandBus, CommandError, CommandRegistry, EventBus};
use vrcx_0_persistence::feed;
use vrcx_0_persistence::DatabaseService;

pub struct MobileRuntimeHostState {
    db: Arc<DatabaseService>,
    events: EventBus,
}

impl MobileRuntimeHostState {
    /// Opens the local database at `app_data_dir` and builds the event bus.
    ///
    /// `app_data_dir` is the directory the platform layer resolved (on Android
    /// `/data/user/0/<package>/files/VRCX-0`), so this mirrors exactly what the
    /// desktop does at startup.
    pub fn open(app_data_dir: &Path) -> Result<Self, CommandError> {
        let db_path = app_data_dir.join("VRCX-0.sqlite3");
        let db = DatabaseService::new(&db_path)
            .map_err(|error| CommandError::failed(format!("open database: {error}")))?;
        Ok(Self {
            db: Arc::new(db),
            events: EventBus::new(256),
        })
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn database(&self) -> &Arc<DatabaseService> {
        &self.db
    }
}

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
    host: Arc<MobileRuntimeHostState>,
    input: feed::FeedRowsQueryInput,
) -> Result<Vec<feed::FeedRowOutput>, CommandError> {
    let db = Arc::clone(host.database());
    blocking("feed rows", move || feed::feed_rows_query(&db, input)).await
}

/// The command surface exposed to the phone UI.
pub fn build_registry() -> CommandRegistry<MobileRuntimeHostState> {
    let mut registry = CommandRegistry::new();
    registry.register_typed::<feed::FeedRowsQueryInput, Vec<feed::FeedRowOutput>, _, _>(
        "app__feed_rows_query",
        app__feed_rows_query,
    );
    registry
}

/// Builds the router state for `serve_loopback`.
pub fn build_bus(host: MobileRuntimeHostState) -> Arc<CommandBus<MobileRuntimeHostState>> {
    let registry = build_registry();
    let events = host.events().clone();
    Arc::new(CommandBus {
        registry: Arc::new(registry),
        state: Arc::new(host),
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scratch directory inside the workspace.
    ///
    /// `std::env::temp_dir()` is unusable on this build machine: the workspace
    /// carries a Low integrity label while %TEMP% does not, so creating files
    /// there fails with ERROR_ACCESS_DENIED.
    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn opens_a_local_database_and_registers_feed_query() {
        let dir = temp_dir("open");
        let host = MobileRuntimeHostState::open(&dir).expect("open host");
        assert!(dir.join("VRCX-0.sqlite3").exists(), "sqlite file created");

        let registry = build_registry();
        assert!(registry.contains("app__feed_rows_query"));
        assert_eq!(registry.len(), 1);

        drop(host);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn event_bus_is_reachable_from_the_state() {
        let dir = temp_dir("events");
        let host = MobileRuntimeHostState::open(&dir).expect("open host");
        let mut receiver = host.events().subscribe();
        host.events()
            .publish("authenticatedSessionProjection", serde_json::json!({ "phase": "ready" }));

        let (event, _) = receiver.try_recv().expect("event");
        assert_eq!(event, "authenticatedSessionProjection");

        drop(host);
        let _ = std::fs::remove_dir_all(dir);
    }
}
