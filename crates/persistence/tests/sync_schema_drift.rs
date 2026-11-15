//! Guards the remote-sync catalog against upstream schema drift.
//!
//! The sync engine identifies a row by the natural key declared in
//! [`SYNC_TABLE_CATALOG`] and only merges fields that carry explicit field
//! semantics. Those declarations are *text*, not types: if upstream renames a
//! column, adds one we should be syncing, or drops a table outright, nothing
//! fails to compile — the sync simply stops carrying that data, silently.
//!
//! These tests close that hole. Run them after every upstream merge:
//!
//! ```text
//! cargo test -p vrcx-0-persistence --test sync_schema_drift
//! ```
//!
//! A failure here means "the upstream schema moved"; fix it by updating the
//! catalog entry for the table named in the message (add the column to
//! `key_columns` / `field_semantics`, or add the table to the catalog).

use std::collections::BTreeSet;

use serde_json::Value;
use vrcx_0_contracts::SYNC_TABLE_CATALOG;
use vrcx_0_persistence::sync::synced_local_tables;
use vrcx_0_persistence::DatabaseService;

/// Fresh throwaway database carrying the full application schema.
fn temp_db(name: &str) -> DatabaseService {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "vrcx-0-sync-drift-{name}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let db = DatabaseService::new(&dir.join("VRCX-0.sqlite3")).expect("open temp db");
    vrcx_0_persistence::game_log::ensure_game_log_tables(&db).expect("game log tables");
    vrcx_0_persistence::maintenance::ensure_required_database_schema(&db)
        .expect("required schema");
    db
}

/// Column names of a table, via `PRAGMA table_info`.
fn columns(db: &DatabaseService, table: &str) -> BTreeSet<String> {
    let rows = db
        .test_execute(&format!("PRAGMA table_info(\"{table}\")"), &Default::default())
        .unwrap_or_default();
    rows.iter()
        .filter_map(|row: &Value| row.get("name").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
fn every_catalog_key_and_field_column_exists() {
    let db = temp_db("columns");
    let plans = synced_local_tables(&db).expect("enumerate synced tables");
    assert!(
        !plans.is_empty(),
        "no synced tables matched the catalog — the catalog templates or the \
         per-user table naming must have drifted"
    );

    let mut problems: Vec<String> = Vec::new();
    for plan in &plans {
        let present = columns(&db, &plan.table);
        if present.is_empty() {
            problems.push(format!(
                "`{}` (catalog `{}`) has no columns — table missing or renamed",
                plan.table, plan.descriptor.template
            ));
            continue;
        }
        for key in plan.descriptor.key_columns {
            if !present.contains(*key) {
                problems.push(format!(
                    "`{}`: key column `{}` is gone; fix key_columns for catalog \
                     table `{}`",
                    plan.table, key, plan.descriptor.template
                ));
            }
        }
        for (field, _) in plan.descriptor.field_semantics {
            if !present.contains(*field) {
                problems.push(format!(
                    "`{}`: field-semantic column `{}` is gone; fix \
                     field_semantics for catalog table `{}`",
                    plan.table, field, plan.descriptor.template
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "sync catalog drifted from the local schema:\n  {}",
        problems.join("\n  ")
    );
}

/// Every catalog entry must resolve to at least one physical table, otherwise a
/// whole feature silently stops syncing (e.g. upstream renames `feed_avatar`).
#[test]
fn every_catalog_entry_has_a_physical_table() {
    let db = temp_db("tables");
    let plans = synced_local_tables(&db).expect("enumerate synced tables");

    let mut missing: Vec<String> = Vec::new();
    for descriptor in SYNC_TABLE_CATALOG {
        let matched = plans
            .iter()
            .any(|plan| plan.descriptor.template == descriptor.template);
        if !matched {
            missing.push(descriptor.template.to_string());
        }
    }

    assert!(
        missing.is_empty(),
        "catalog tables with no physical counterpart:\n  {}\n\
         Either the table was renamed/removed upstream (update the catalog) or \
         the schema bootstrap no longer creates it.",
        missing.join("\n  ")
    );
}

/// Conversely: physical tables that look like synced feature tables must be
/// declared in the catalog, or upstream can add a table we never replicate.
#[test]
fn feature_tables_are_declared_in_the_catalog() {
    let db = temp_db("undeclared");
    let rows = db
        .test_execute(
            "SELECT name FROM sqlite_schema WHERE type = 'table'",
            &Default::default(),
        )
        .expect("list tables");
    let present: Vec<String> = rows
        .iter()
        .filter_map(|row: &Value| row.get("name").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect();

    // Prefixes whose tables are user-facing facts worth replicating. Caches,
    // credentials and derived read-models are intentionally excluded.
    const WATCHED_PREFIXES: &[&str] = &[
        "gamelog_",
        "feed_",
        "self_profile_log",
        "friend_log_",
        "notifications",
        "avatar_wear_log",
        "moderation",
        "notes",
        "mutual_graph_",
        "profile_bio",
        "memos",
        "world_memos",
        "avatar_memos",
        "avatar_tags",
        "browse_history",
        "favorite_",
        "owners",
    ];

    let plans = synced_local_tables(&db).expect("enumerate synced tables");
    let declared: BTreeSet<&str> = plans.iter().map(|plan| plan.table.as_str()).collect();

    let undeclared: Vec<&String> = present
        .iter()
        .filter(|name| !name.starts_with("_sync_") && !name.starts_with("sqlite_"))
        .filter(|name| WATCHED_PREFIXES.iter().any(|p| name.starts_with(p)))
        .filter(|name| !declared.contains(name.as_str()))
        .collect();

    assert!(
        undeclared.is_empty(),
        "feature tables exist but are not in the sync catalog:\n  {}\n\
         Add a `SyncTableDescriptor` for each (or extend WATCHED_PREFIXES if the \
         table is deliberately local-only).",
        undeclared
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
