//! Collector election: which device records live feed observations.
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
//! Ordering comes from `remoteSync.collectorPriority` (integer, default 0):
//!
//! - every fresh participant is ranked by **priority, higher wins**, ties broken
//!   by the **smallest device id** — deterministic, so all nodes agree without
//!   coordination;
//! - a device records feed iff nothing outranks it; `remoteSync.collector = off`
//!   keeps a device out of the registry entirely, so it can neither collect nor
//!   block the real collector;
//! - a legacy `collector = forced` row with no explicit priority is treated as
//!   priority 1000, so configs written before the priority column keep working —
//!   but it no longer means "collect regardless": two legacy forced devices now
//!   resolve to one winner instead of both writing.
//!
//! A typical three-node setup: main desktop 20, second desktop 10, 24/7 server 0.
//! The server therefore only collects while every desktop is off, while an active
//! desktop records its own rows — zero feed latency on the machine you are
//! actually looking at.
//!
//! Failure direction is always fail-open (collect): a wrong election costs a
//! duplicate row, while a silent stop loses data permanently. VRCX-K learned that
//! the hard way (`VRCX_FeedCollector`), and the same rule applies here.
//!
//! Known limit: this is a *node*-level gate, while a sync database can hold
//! several VRChat accounts. If devices ever log in with different account sets,
//! the gate has to be computed per account prefix instead.

use std::sync::Arc;
use std::time::Duration;

use vrcx_0_contracts::{set_live_feed_collector, SyncDeviceRecord, DEVICE_ROLE_FORCED};
use vrcx_0_persistence::config::get_string;

use crate::state::remote_sync::RemoteSyncHost;

/// Config key holding this device's collector role (`auto` | `off`).
pub const CONFIG_COLLECTOR_ROLE: &str = "remoteSync.collector";
/// Config key holding this device's collector priority (higher wins).
pub const CONFIG_COLLECTOR_PRIORITY: &str = "remoteSync.collectorPriority";
/// Config key holding the sync interval, used to size the activity window.
pub const CONFIG_INTERVAL_SEC: &str = "remoteSync.intervalSec";

/// Priority implied by a legacy `collector = forced` row with no explicit number.
const LEGACY_FORCED_PRIORITY: i32 = 1000;

/// Poll cadence for the election; also the worst-case handover delay.
pub const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Minimum activity window: a peer that has not synced for this long is gone.
pub const ACTIVITY_WINDOW: Duration = Duration::from_secs(45);

/// Upper bound, so a pathological interval cannot freeze the election.
const MAX_ACTIVITY_WINDOW: Duration = Duration::from_secs(360);

/// Effective election priority for a device record.
///
/// Legacy `forced` rows (written before the priority column existed) map to a
/// high value so they keep winning.
pub fn effective_priority(role: &str, priority: i32) -> i32 {
    if role == DEVICE_ROLE_FORCED && priority == 0 {
        LEGACY_FORCED_PRIORITY
    } else {
        priority
    }
}

/// Role reported for this device; anything other than `off` participates.
pub fn normalise_role(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "false" | "0" | "no" | "disabled" => "off",
        _ => vrcx_0_contracts::DEVICE_ROLE_AUTO,
    }
}

/// Read this device's role from the local config store.
pub fn read_role(host: &RemoteSyncHost) -> String {
    let raw = get_string(host.db(), CONFIG_COLLECTOR_ROLE, "").unwrap_or_default();
    normalise_role(&raw).to_string()
}

/// Read this device's priority from the local config store.
pub fn read_priority(host: &RemoteSyncHost) -> i32 {
    let raw = get_string(host.db(), CONFIG_COLLECTOR_PRIORITY, "").unwrap_or_default();
    raw.trim().parse::<i32>().unwrap_or(0).clamp(-1000, 1000)
}

/// Activity window: at least [`ACTIVITY_WINDOW`], but never shorter than two
/// sync cycles, so a node that merely skipped one heartbeat is not treated as
/// gone (that would hand collection back and forth on every hiccup).
pub fn activity_window(host: &RemoteSyncHost) -> Duration {
    let interval = get_string(host.db(), CONFIG_INTERVAL_SEC, "")
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(60)
        .clamp(5, 3600);
    Duration::from_secs(interval.saturating_mul(2))
        .max(ACTIVITY_WINDOW)
        .min(MAX_ACTIVITY_WINDOW)
}

/// A peer counts as a candidate while the server-side heartbeat is fresh.
fn peer_is_fresh(device: &SyncDeviceRecord, window: Duration) -> bool {
    if device.online {
        return true;
    }
    // `seen_seconds_ago` is computed by PostgreSQL; negative values mean the
    // peer's heartbeat is ahead of the server clock, which still counts.
    device.seen_seconds_ago.abs() <= window.as_secs() as i64
}

/// Whether `peer` outranks this device (priority first, then device id).
fn peer_outranks(own_device_id: &str, own_priority: i32, peer: &SyncDeviceRecord) -> bool {
    let peer_priority = effective_priority(&peer.role, peer.priority);
    peer_priority > own_priority
        || (peer_priority == own_priority && peer.device_id.as_str() < own_device_id)
}

/// Fresh peers that outrank this device, i.e. peers it must yield to.
pub fn blockers(
    own_device_id: &str,
    own_role: &str,
    own_priority: i32,
    devices: &[SyncDeviceRecord],
    window: Duration,
) -> Vec<String> {
    if own_role == "off" {
        // `off` records nothing and blocks nobody.
        return Vec::new();
    }

    // Compare like with like: a legacy `forced` device is ranked at the same
    // effective priority its peers advertise, otherwise it would see itself as
    // outranked by every other legacy forced row.
    let own_priority = effective_priority(own_role, own_priority);

    let mut blockers: Vec<String> = devices
        .iter()
        .filter(|device| device.device_id != own_device_id)
        .filter(|device| peer_is_fresh(device, window))
        .filter(|device| peer_outranks(own_device_id, own_priority, device))
        .map(|device| device.device_id.clone())
        .collect();

    blockers.sort();
    blockers
}

/// Whether this device may record live feed right now.
pub fn may_collect(
    own_device_id: &str,
    own_role: &str,
    own_priority: i32,
    devices: &[SyncDeviceRecord],
    window: Duration,
) -> bool {
    own_role != "off" && blockers(own_device_id, own_role, own_priority, devices, window).is_empty()
}

/// Evaluate the election once and apply it to the write gate.
///
/// Returns `(may_collect, blockers)` so callers can log transitions.
pub async fn evaluate(host: &Arc<RemoteSyncHost>) -> (bool, Vec<String>) {
    let role = read_role(host);
    let priority = read_priority(host);
    let window = activity_window(host);
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

    let blocking = blockers(&device_id, &role, priority, &status.remote_devices, window);
    let may = may_collect(&device_id, &role, priority, &status.remote_devices, window);

    // Record the decision in `_sync_meta` (per-device, never synced) so it can
    // be inspected without a UI:
    //   select key, value from _sync_meta where key like 'sync.collector%';
    let _ = vrcx_0_persistence::sync::sync_meta_set(host.db(), "sync.collectorRole", &role);
    let _ = vrcx_0_persistence::sync::sync_meta_set(
        host.db(),
        "sync.collectorPriority",
        &priority.to_string(),
    );
    let _ = vrcx_0_persistence::sync::sync_meta_set(
        host.db(),
        "sync.collectorMayCollect",
        if may { "1" } else { "0" },
    );
    let _ = vrcx_0_persistence::sync::sync_meta_set(
        host.db(),
        "sync.collectorBlockers",
        &blocking.join(","),
    );

    // Flip the write gate only; the websocket stays up so the UI remains live
    // and the rows arrive through sync instead.
    if set_live_feed_collector(may) {
        if may {
            tracing::info!(
                priority,
                "collector election: this device now records live feed"
            );
        } else {
            tracing::info!(
                priority,
                blockers = ?blocking,
                "collector election: letting another device record live feed (UI stays live)"
            );
        }
    }
    (may, blocking)
}

/// Spawn the periodic election loop.
///
/// Runs one evaluation immediately, so a node starting while a higher-priority
/// device collects begins yielding right away instead of after one poll.
pub fn spawn(host: Arc<RemoteSyncHost>, tasks: &vrcx_0_application_core::TaskSupervisor) {
    tasks.spawn(async move {
        loop {
            let _ = evaluate(&host).await;
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vrcx_0_contracts::{SyncDeviceRecord, DEVICE_ROLE_AUTO, DEVICE_ROLE_FORCED};

    use super::{blockers, effective_priority, may_collect, normalise_role, ACTIVITY_WINDOW};

    fn device(id: &str, role: &str, priority: i32, seen: i64) -> SyncDeviceRecord {
        SyncDeviceRecord {
            device_id: id.to_string(),
            role: role.to_string(),
            priority,
            seen_seconds_ago: seen,
            online: seen.abs() <= ACTIVITY_WINDOW.as_secs() as i64,
            ..Default::default()
        }
    }

    fn may(id: &str, role: &str, priority: i32, peers: &[SyncDeviceRecord]) -> bool {
        may_collect(id, role, priority, peers, ACTIVITY_WINDOW)
    }

    #[test]
    fn normalises_role_spellings() {
        assert_eq!(normalise_role("off"), "off");
        assert_eq!(normalise_role(" FALSE "), "off");
        assert_eq!(normalise_role("0"), "off");
        assert_eq!(normalise_role(""), DEVICE_ROLE_AUTO);
        assert_eq!(normalise_role("nonsense"), DEVICE_ROLE_AUTO);
    }

    #[test]
    fn legacy_forced_maps_to_high_priority() {
        assert_eq!(effective_priority(DEVICE_ROLE_FORCED, 0), 1000);
        assert_eq!(effective_priority(DEVICE_ROLE_FORCED, 20), 20);
        assert_eq!(effective_priority(DEVICE_ROLE_AUTO, 0), 0);
    }

    #[test]
    fn highest_priority_collects_when_alone() {
        assert!(may("me", DEVICE_ROLE_AUTO, 0, &[]));
        assert!(may("me", DEVICE_ROLE_AUTO, 20, &[]));
    }

    #[test]
    fn off_never_collects() {
        assert!(!may("me", "off", 100, &[]));
        assert!(blockers("me", "off", 100, &[], ACTIVITY_WINDOW).is_empty());
    }

    #[test]
    fn main_desktop_beats_second_desktop_and_server() {
        let main = device("aaa", DEVICE_ROLE_AUTO, 20, 5);
        let second = device("bbb", DEVICE_ROLE_AUTO, 10, 5);
        let server = device("srv", DEVICE_ROLE_AUTO, 0, 5);

        assert!(may("aaa", DEVICE_ROLE_AUTO, 20, &[main.clone(), second.clone(), server.clone()]));
        assert!(!may("bbb", DEVICE_ROLE_AUTO, 10, &[main.clone(), second.clone(), server.clone()]));
        assert!(!may("srv", DEVICE_ROLE_AUTO, 0, &[main, second, server]));
    }

    #[test]
    fn second_desktop_takes_over_when_the_first_goes_stale() {
        let stale_main = device("aaa", DEVICE_ROLE_AUTO, 20, 600);
        let second = device("bbb", DEVICE_ROLE_AUTO, 10, 5);
        let server = device("srv", DEVICE_ROLE_AUTO, 0, 5);

        assert!(may("bbb", DEVICE_ROLE_AUTO, 10, &[stale_main.clone(), second.clone(), server.clone()]));
        assert!(!may("srv", DEVICE_ROLE_AUTO, 0, &[stale_main, second, server]));
    }

    #[test]
    fn server_collects_only_when_every_desktop_is_gone() {
        let stale_main = device("aaa", DEVICE_ROLE_AUTO, 20, 600);
        let stale_second = device("bbb", DEVICE_ROLE_AUTO, 10, 600);
        let server = device("srv", DEVICE_ROLE_AUTO, 0, 5);

        assert!(may("srv", DEVICE_ROLE_AUTO, 0, &[stale_main, stale_second, server]));
    }

    #[test]
    fn equal_priority_falls_back_to_the_smaller_device_id() {
        assert!(!may("me", DEVICE_ROLE_AUTO, 0, &[device("aaa", DEVICE_ROLE_AUTO, 0, 5)]));
        assert!(may("me", DEVICE_ROLE_AUTO, 0, &[device("zzz", DEVICE_ROLE_AUTO, 0, 5)]));
    }

    #[test]
    fn two_legacy_forced_devices_resolve_to_one_winner() {
        let a = device("aaa", DEVICE_ROLE_FORCED, 0, 5);
        let b = device("bbb", DEVICE_ROLE_FORCED, 0, 5);
        let all = vec![a, b];

        assert!(may("aaa", DEVICE_ROLE_FORCED, 0, &all), "smaller id wins the tie");
        assert!(!may("bbb", DEVICE_ROLE_FORCED, 0, &all));
    }

    #[test]
    fn stale_peers_are_ignored() {
        assert!(may("me", DEVICE_ROLE_AUTO, 0, &[device("aaa", DEVICE_ROLE_AUTO, 99, 600)]));
    }

    #[test]
    fn own_row_is_ignored() {
        assert!(may("me", DEVICE_ROLE_AUTO, 0, &[device("me", DEVICE_ROLE_AUTO, 100, 0)]));
    }

    #[test]
    fn negative_priority_yields_to_default() {
        assert!(!may("me", DEVICE_ROLE_AUTO, -5, &[device("aaa", DEVICE_ROLE_AUTO, 0, 5)]));
    }

    #[test]
    fn base_window_is_at_least_the_documented_minimum() {
        assert!(ACTIVITY_WINDOW >= Duration::from_secs(45));
    }
}
