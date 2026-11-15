//! Collector election: which device may hold the VRChat realtime session.
//!
//! Several devices sharing one sync database all hold the same VRChat account,
//! so they all observe the same friend events. `feed_*` rows are natural-keyed
//! on a client-stamped `created_at`, so duplicated observations cannot be merged
//! by any key — the only reliable fix is a single collector.
//!
//! Losing the election does **not** drop the realtime connection: the websocket
//! stays up so the UI stays live, and only live feed *writes* are skipped
//! (`vrcx_0_contracts::collector_gate`). The rows arrive through sync instead,
//! so the data is identical on every device without duplicating observations.
//!
//! The rules are the ones that shipped in VRCX-K (`VRCX_FeedCollector`:
//! `feedCollector.js` + `nodeRegistry.js`), re-based onto the sync device
//! registry, with the two weaknesses of that implementation fixed by the move:
//!
//! | VRCX-K | here |
//! | --- | --- |
//! | heartbeat stamped with each node's **local** clock (skew > TTL breaks the election) | `_sync_devices.last_seen_at` is `now()` **on the PostgreSQL side** |
//! | `node_id` persisted by hand in VRCXStorage | `sync.deviceId` lives in `_sync_meta` per database |
//!
//! Roles (`remoteSync.collector`):
//!
//! - `auto` (default) — participates: yields to any fresh `forced` peer, and to
//!   any fresh `auto` peer with a **smaller device id** (deterministic, so all
//!   nodes agree without coordination);
//! - `forced` — never yields. A 24/7 server sets this so desktops step aside;
//!   two `forced` devices both collect (manual double-write, as documented in
//!   VRCX-K);
//! - `off` — never collects and does not register itself, so it cannot block
//!   the real collector either.
//!
//! Failure direction is always fail-open (collect): a wrong election costs a
//! duplicate row, while a silent stop loses data permanently — VRCX-K learned
//! this the hard way, and the same rule applies here.

use std::sync::Arc;
use std::time::Duration;

use vrcx_0_contracts::{set_live_feed_collector, SyncDeviceRecord, DEVICE_ROLE_FORCED};
use vrcx_0_persistence::config::get_string;

use super::RemoteSyncHost;

/// Config key holding this device's collector role.
pub const CONFIG_COLLECTOR_ROLE: &str = "remoteSync.collector";

/// Poll cadence for the election, matching the sync interval's order of
/// magnitude while staying under the activity window.
pub const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// How recently a peer must have synced to count as an election candidate.
/// Generous versus the default 60 s interval so one missed cycle does not hand
/// collection back and forth.
pub const ACTIVITY_WINDOW: Duration = Duration::from_secs(90);

/// Role reported for this device; anything unrecognized falls back to `auto`.
pub fn normalise_role(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "forced" | "true" | "1" | "yes" => vrcx_0_contracts::DEVICE_ROLE_FORCED,
        "off" | "false" | "0" | "no" => "off",
        _ => vrcx_0_contracts::DEVICE_ROLE_AUTO,
    }
}

/// Read this device's role from the local config store.
pub fn read_role(host: &RemoteSyncHost) -> String {
    let raw = get_string(host.db(), CONFIG_COLLECTOR_ROLE, "").unwrap_or_default();
    normalise_role(&raw).to_string()
}

/// A peer counts as a candidate while the server-side heartbeat is fresh.
fn peer_is_fresh(device: &SyncDeviceRecord) -> bool {
    if device.online {
        return true;
    }
    // `seen_seconds_ago` is computed by PostgreSQL; negative values mean the
    // peer's heartbeat is ahead of the server clock, which still counts.
    device.seen_seconds_ago.abs() <= ACTIVITY_WINDOW.as_secs() as i64
}

/// Devices that currently own collection, i.e. peers this device must yield to.
///
/// `own_role` decides the rule set; `PEER` order is device-id order so every
/// node computes the same winner.
pub fn blockers(own_device_id: &str, own_role: &str, devices: &[SyncDeviceRecord]) -> Vec<String> {
    if own_role == "off" || own_role == DEVICE_ROLE_FORCED {
        // `off` collects nothing; `forced` yields to nobody.
        return Vec::new();
    }

    let fresh: Vec<&SyncDeviceRecord> = devices
        .iter()
        .filter(|device| device.device_id != own_device_id)
        .filter(|device| peer_is_fresh(device))
        .collect();

    let mut blockers: Vec<String> = fresh
        .iter()
        .filter(|device| device.role == DEVICE_ROLE_FORCED)
        .map(|device| device.device_id.clone())
        .collect();

    if blockers.is_empty() {
        blockers.extend(
            fresh
                .iter()
                .filter(|device| device.role != DEVICE_ROLE_FORCED)
                .filter(|device| device.device_id.as_str() < own_device_id)
                .map(|device| device.device_id.clone()),
        );
    }

    blockers.sort();
    blockers
}

/// Whether this device may keep its realtime session open.
pub fn may_collect(own_device_id: &str, own_role: &str, devices: &[SyncDeviceRecord]) -> bool {
    own_role != "off" && blockers(own_device_id, own_role, devices).is_empty()
}

/// Evaluate the election once and apply it to the gate.
///
/// Returns `(may_collect, blockers)` so callers can log transitions.
pub async fn evaluate(host: &Arc<RemoteSyncHost>) -> (bool, Vec<String>) {
    let role = read_role(host);
    let status = host.status().await;
    let device_id = if status.device_id.is_empty() {
        host.device_id().unwrap_or_default()
    } else {
        status.device_id.clone()
    };

    // Not configured (or no engine yet): nothing is syncing, so nothing can
    // conflict — collect. Fail-open per the module docs.
    if !status.enabled || device_id.is_empty() {
        set_live_feed_collector(true);
        return (true, Vec::new());
    }

    let blocking = blockers(&device_id, &role, &status.remote_devices);
    let may = may_collect(&device_id, &role, &status.remote_devices);
    // Flip the write gate only; the websocket stays up so the UI remains live
    // and the rows arrive through sync instead.
    if set_live_feed_collector(may) {
        if may {
            tracing::info!(role = %role, "collector election: this device now records live feed");
        } else {
            tracing::info!(
                role = %role,
                blockers = ?blocking,
                "collector election: letting another device record live feed (UI stays live)"
            );
        }
    }
    (may, blocking)
}

/// Spawn the periodic election loop.
///
/// Runs one evaluation immediately so a headless node that starts while a
/// desktop is collecting begins holding right away instead of after one poll.
pub fn spawn(
    host: Arc<RemoteSyncHost>,
    tasks: &vrcx_0_application_core::TaskSupervisor,
) {
    tasks.spawn(async move {
        loop {
            let _ = evaluate(&host).await;
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use vrcx_0_contracts::{DEVICE_ROLE_AUTO, DEVICE_ROLE_FORCED};

    use super::{blockers, may_collect, normalise_role, ACTIVITY_WINDOW};

    fn device(id: &str, role: &str, seen: i64) -> vrcx_0_contracts::SyncDeviceRecord {
        vrcx_0_contracts::SyncDeviceRecord {
            device_id: id.to_string(),
            role: role.to_string(),
            seen_seconds_ago: seen,
            online: seen.abs() <= ACTIVITY_WINDOW.as_secs() as i64,
            ..Default::default()
        }
    }

    #[test]
    fn normalises_role_spellings() {
        assert_eq!(normalise_role("forced"), DEVICE_ROLE_FORCED);
        assert_eq!(normalise_role(" TRUE "), DEVICE_ROLE_FORCED);
        assert_eq!(normalise_role("off"), "off");
        assert_eq!(normalise_role("0"), "off");
        assert_eq!(normalise_role(""), DEVICE_ROLE_AUTO);
        assert_eq!(normalise_role("nonsense"), DEVICE_ROLE_AUTO);
    }

    #[test]
    fn auto_collects_when_nothing_else_is_present() {
        assert!(may_collect("me", DEVICE_ROLE_AUTO, &[]));
    }

    #[test]
    fn auto_yields_to_a_fresh_forced_peer() {
        let peers = vec![device("srv", DEVICE_ROLE_FORCED, 10)];
        assert!(!may_collect("me", DEVICE_ROLE_AUTO, &peers));
    }

    #[test]
    fn forced_never_yields() {
        let peers = vec![device("srv", DEVICE_ROLE_FORCED, 5)];
        assert!(may_collect("me", DEVICE_ROLE_FORCED, &peers));
    }

    #[test]
    fn off_never_collects() {
        assert!(!may_collect("me", "off", &[]));
        assert!(blockers("me", "off", &[]).is_empty());
    }

    #[test]
    fn auto_yields_to_the_smaller_auto_id_only() {
        let peers = vec![device("aaa", DEVICE_ROLE_AUTO, 5)];
        assert!(!may_collect("me", DEVICE_ROLE_AUTO, &peers), "smaller id wins");

        let peers = vec![device("zzz", DEVICE_ROLE_AUTO, 5)];
        assert!(may_collect("me", DEVICE_ROLE_AUTO, &peers), "larger id yields");
    }

    #[test]
    fn stale_peers_are_ignored() {
        let peers = vec![device("srv", DEVICE_ROLE_FORCED, 600)];
        assert!(may_collect("me", DEVICE_ROLE_AUTO, &peers), "600s stale");
    }

    #[test]
    fn own_row_is_ignored() {
        let peers = vec![device("me", DEVICE_ROLE_FORCED, 0)];
        assert!(may_collect("me", DEVICE_ROLE_AUTO, &peers));
    }

    #[test]
    fn two_desktops_converge_on_one_winner() {
        // Both see each other; exactly one must collect, and both must agree.
        let a = device("aaa", DEVICE_ROLE_AUTO, 5);
        let b = device("bbb", DEVICE_ROLE_AUTO, 5);
        let all = vec![a.clone(), b.clone()];

        assert!(may_collect("aaa", DEVICE_ROLE_AUTO, &all));
        assert!(!may_collect("bbb", DEVICE_ROLE_AUTO, &all));
    }
}
