//! Process-wide health signals that feed the collector election.
//!
//! The election ranks devices by `remoteSync.collectorPriority`, but priority is
//! the user's *intent*: a device that intends to collect may still be unable to
//! (VRChat websocket flapping, expired session, sync database unreachable). This
//! module turns those two live signals into a small score that every device
//! reports in its heartbeat, so peers can tell "this device is not collecting
//! because it is broken" apart from "it is simply lower priority".
//!
//! Inputs, both already produced elsewhere in the process:
//!
//! - realtime websocket status transitions (`RuntimeOutput`: connected /
//!   disconnecting / authFailure / disconnected);
//! - the outcome of each sync cycle (`application-sync` engine).
//!
//! Rules (deliberately blunt — anything cleverer flaps):
//!
//! | signal | healthy | unhealthy |
//! | --- | --- | --- |
//! | auth | session valid | `authFailure` (score drops instantly) |
//! | websocket | up | ≥ 15 min window with < 60 % uptime |
//! | sync cycle | last cycle ok | ≥ 3 consecutive failures |
//!
//! Recovery is asymmetric on purpose: degradation is immediate, recovery needs
//! [`RECOVER_STABLE_SECS`] of unbroken health. Without that hysteresis two nodes
//! would hand collection back and forth every few seconds.
//!
//! Failure direction matches the rest of the election: unknown state counts as
//! healthy (fail-open), because a wrong election costs a duplicate row while a
//! silent stop loses data.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Sliding window used for the websocket uptime ratio.
pub const WINDOW: Duration = Duration::from_secs(15 * 60);
/// Minimum websocket uptime inside [`WINDOW`] to stay healthy.
pub const MIN_UPTIME_PCT: u32 = 60;
/// Consecutive failed sync cycles that mark the device unhealthy.
pub const MAX_CYCLE_FAILURES: u32 = 3;
/// How long everything must stay healthy before a degraded device recovers.
pub const RECOVER_STABLE_SECS: u64 = 120;
/// Score reported while healthy.
pub const SCORE_HEALTHY: u8 = 100;
/// Score reported while degraded.
pub const SCORE_DEGRADED: u8 = 0;

/// Why a device is (not) collecting, as shown in the settings device list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthReason {
    /// Everything looks fine.
    Healthy,
    /// VRChat websocket repeatedly dropped inside the window.
    WebsocketUnstable,
    /// VRChat rejected the session (needs a re-login).
    AuthExpired,
    /// The sync database kept failing.
    SyncFailing,
    /// Signals recovered but the stability window has not elapsed yet.
    Recovering,
}

impl HealthReason {
    /// Stable identifier used on the wire and in the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::WebsocketUnstable => "websocketUnstable",
            Self::AuthExpired => "authExpired",
            Self::SyncFailing => "syncFailing",
            Self::Recovering => "recovering",
        }
    }
}

/// Point-in-time view handed to the heartbeat and to the election.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthSnapshot {
    /// 0 when degraded, 100 when healthy.
    pub score: u8,
    /// Machine-readable reason (see [`HealthReason::as_str`]).
    pub reason: &'static str,
    /// Websocket currently up.
    pub websocket_ok: bool,
    /// Consecutive failed sync cycles.
    pub cycle_failures: u32,
    /// Websocket uptime inside the window, in percent (0 when unknown).
    pub uptime_pct: u32,
}

impl Default for HealthSnapshot {
    fn default() -> Self {
        Self {
            score: SCORE_HEALTHY,
            reason: HealthReason::Healthy.as_str(),
            websocket_ok: true,
            cycle_failures: 0,
            uptime_pct: 100,
        }
    }
}

impl HealthSnapshot {
    /// Whether this device may collect at all.
    pub fn is_healthy(&self) -> bool {
        self.score > 0
    }
}

#[derive(Debug)]
struct Inner {
    websocket_ok: bool,
    /// Start of the current uptime bucket; `ok_acc`/`fail_acc` hold the rest.
    bucket_start: Instant,
    ok_acc: Duration,
    fail_acc: Duration,
    auth_expired: bool,
    cycle_failures: u32,
    degraded_since: Option<Instant>,
    healthy_since: Option<Instant>,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            websocket_ok: true,
            bucket_start: Instant::now(),
            ok_acc: WINDOW,
            fail_acc: Duration::ZERO,
            auth_expired: false,
            cycle_failures: 0,
            degraded_since: None,
            healthy_since: None,
        }
    }
}

static STATE: Mutex<Option<Inner>> = Mutex::new(None);

fn with_state<T>(f: impl FnOnce(&mut Inner) -> T) -> T {
    let mut guard = STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let inner = guard.get_or_insert_with(Inner::default);
    f(inner)
}

impl Inner {
    /// Roll the current uptime bucket into the window accumulators.
    fn settle(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.bucket_start);
        if elapsed.is_zero() {
            return;
        }
        if self.websocket_ok {
            self.ok_acc += elapsed;
        } else {
            self.fail_acc += elapsed;
        }
        self.bucket_start = now;
        // Keep a sliding window instead of a lifetime ratio.
        let total = self.ok_acc + self.fail_acc;
        if total > WINDOW {
            let excess = total - WINDOW;
            let take_fail = self.fail_acc.min(excess);
            self.fail_acc -= take_fail;
            self.ok_acc = self.ok_acc.saturating_sub(excess - take_fail);
        }
    }

    fn uptime_pct(&self, now: Instant) -> u32 {
        let mut ok = self.ok_acc;
        let mut fail = self.fail_acc;
        let elapsed = now.saturating_duration_since(self.bucket_start);
        if self.websocket_ok {
            ok += elapsed;
        } else {
            fail += elapsed;
        }
        let total = ok + fail;
        if total.is_zero() {
            return 100;
        }
        ((ok.as_millis() * 100) / total.as_millis()).min(100) as u32
    }

    /// Decay accumulated failures once the websocket has been up long enough.
    fn decay_failures(&mut self, now: Instant) {
        if self.websocket_ok && self.fail_acc.is_zero() && self.ok_acc >= Duration::from_secs(300) {
            self.cycle_failures = self.cycle_failures.min(1);
        }
        let _ = now;
    }
}

/// Record a realtime websocket status transition.
///
/// `status` is the lower-case label produced by `RuntimeOutput`
/// (`connected`, `connecting`, `disconnecting`, `disconnected`, `authfailure`, …).
pub fn record_websocket_status(status: &str, reason: Option<&str>) {
    let status = status.trim().to_ascii_lowercase();
    with_state(|inner| {
        let now = Instant::now();
        inner.settle(now);
        match status.as_str() {
            "connected" => {
                inner.websocket_ok = true;
                inner.auth_expired = false;
            }
            "authfailure" => {
                // Session rejected: no amount of reconnecting helps.
                inner.websocket_ok = false;
                inner.auth_expired = true;
            }
            "disconnected" | "disconnecting" | "stopped" => {
                inner.websocket_ok = false;
                if reason.is_some_and(|r| r.to_ascii_lowercase().contains("auth")) {
                    inner.auth_expired = true;
                }
            }
            // `connecting` and anything unknown keep the previous verdict.
            _ => {}
        }
        inner.decay_failures(now);
    });
}

/// Record the outcome of one sync cycle.
pub fn record_sync_cycle(ok: bool) {
    with_state(|inner| {
        if ok {
            inner.cycle_failures = 0;
        } else {
            inner.cycle_failures = inner.cycle_failures.saturating_add(1);
        }
    });
}

/// Current health, applying the degrade/recover hysteresis.
pub fn snapshot() -> HealthSnapshot {
    with_state(|inner| {
        let now = Instant::now();
        inner.settle(now);
        let uptime_pct = inner.uptime_pct(now);

        let failing_reason = if inner.auth_expired {
            Some(HealthReason::AuthExpired)
        } else if inner.cycle_failures >= MAX_CYCLE_FAILURES {
            Some(HealthReason::SyncFailing)
        } else if uptime_pct < MIN_UPTIME_PCT {
            Some(HealthReason::WebsocketUnstable)
        } else {
            None
        };

        match failing_reason {
            Some(reason) => {
                inner.degraded_since.get_or_insert(now);
                inner.healthy_since = None;
                HealthSnapshot {
                    score: SCORE_DEGRADED,
                    reason: reason.as_str(),
                    websocket_ok: inner.websocket_ok,
                    cycle_failures: inner.cycle_failures,
                    uptime_pct,
                }
            }
            None => {
                // Recovering: hold the degraded score until the signal has been
                // clean for RECOVER_STABLE_SECS.
                let recovering = match inner.degraded_since {
                    None => false,
                    Some(since) => {
                        let stable = now.saturating_duration_since(since);
                        stable < Duration::from_secs(RECOVER_STABLE_SECS)
                    }
                };
                if recovering {
                    HealthSnapshot {
                        score: SCORE_DEGRADED,
                        reason: HealthReason::Recovering.as_str(),
                        websocket_ok: inner.websocket_ok,
                        cycle_failures: inner.cycle_failures,
                        uptime_pct,
                    }
                } else {
                    inner.degraded_since = None;
                    inner.healthy_since.get_or_insert(now);
                    HealthSnapshot {
                        score: SCORE_HEALTHY,
                        reason: HealthReason::Healthy.as_str(),
                        websocket_ok: inner.websocket_ok,
                        cycle_failures: inner.cycle_failures,
                        uptime_pct,
                    }
                }
            }
        }
    })
}

/// Test support: wipe all accumulated signals.
#[cfg(test)]
pub fn reset_for_tests() {
    let mut guard = STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    /// The signal store is process-global, so tests must not interleave.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn setup() -> MutexGuard<'static, ()> {
        let guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        reset_for_tests();
        guard
    }

    #[test]
    fn defaults_to_healthy() {
        let _guard = setup();
        let health = snapshot();
        assert_eq!(health.score, SCORE_HEALTHY);
        assert_eq!(health.reason, HealthReason::Healthy.as_str());
        assert!(health.is_healthy());
    }

    #[test]
    fn auth_failure_degrades_immediately() {
        let _guard = setup();
        record_websocket_status("authFailure", Some("token expired"));
        let health = snapshot();
        assert_eq!(health.score, SCORE_DEGRADED);
        assert_eq!(health.reason, HealthReason::AuthExpired.as_str());
        assert!(
            !health.websocket_ok,
            "auth failure implies the socket is down"
        );
    }

    #[test]
    fn three_failed_cycles_degrade_and_one_success_does_not_recover_yet() {
        let _guard = setup();
        record_sync_cycle(false);
        record_sync_cycle(false);
        assert!(snapshot().is_healthy(), "two failures are tolerated");
        record_sync_cycle(false);
        let degraded = snapshot();
        assert_eq!(degraded.score, SCORE_DEGRADED);
        assert_eq!(degraded.reason, HealthReason::SyncFailing.as_str());

        record_sync_cycle(true);
        let recovering = snapshot();
        assert_eq!(
            recovering.reason,
            HealthReason::Recovering.as_str(),
            "hysteresis holds the degraded score until the window elapses"
        );
        assert!(!recovering.is_healthy());
    }

    #[test]
    fn disconnected_websocket_does_not_flip_health_before_the_window() {
        let _guard = setup();
        record_websocket_status("disconnected", None);
        // The window still remembers plenty of uptime.
        let health = snapshot();
        assert_eq!(health.score, SCORE_HEALTHY);
        assert!(!health.websocket_ok);
    }

    #[test]
    fn auth_flag_clears_after_a_successful_connection() {
        let _guard = setup();
        record_websocket_status("authFailure", None);
        assert_eq!(snapshot().reason, HealthReason::AuthExpired.as_str());
        record_websocket_status("connected", None);
        // Still recovering, but no longer auth-expired.
        let health = snapshot();
        assert_ne!(health.reason, HealthReason::AuthExpired.as_str());
    }

    #[test]
    fn reasons_are_stable_strings() {
        let _guard = setup();
        assert_eq!(HealthReason::Healthy.as_str(), "healthy");
        assert_eq!(HealthReason::SyncFailing.as_str(), "syncFailing");
        assert_eq!(
            HealthReason::WebsocketUnstable.as_str(),
            "websocketUnstable"
        );
        assert_eq!(HealthReason::AuthExpired.as_str(), "authExpired");
        assert_eq!(HealthReason::Recovering.as_str(), "recovering");
    }
}
