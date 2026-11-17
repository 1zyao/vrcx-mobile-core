//! Process-wide flag: may this device record live feed observations?
//!
//! Several devices sharing one sync database all hold the same VRChat account,
//! so they all receive the same realtime friend events. `feed_*` rows are
//! natural-keyed on a **client-stamped** `created_at`, so two devices observing
//! one event produce two different keys and no constraint can merge them.
//!
//! [`crate::state::collector_election`] picks a single collector from the sync
//! device registry and flips this flag. Devices that lose the election keep
//! their VRChat websocket open — the UI stays live — but skip writing live feed
//! rows. They still receive everything through sync, so nothing is lost and
//! nothing is duplicated.
//!
//! Default is `true` (fail-open): a wrong election costs a duplicate row, while
//! a silent stop loses data permanently. Same failure direction VRCX-K chose
//! for `VRCX_FeedCollector`.
//!
//! Backfilled rows (Recorder fill-in, game-log import) are deliberately **not**
//! gated: they are replayed history, not live observations, and blocking them
//! would drop data that only this device has.

use std::sync::atomic::{AtomicBool, Ordering};

static LIVE_FEED_COLLECTOR: AtomicBool = AtomicBool::new(true);

/// Set whether this device records live feed observations.
///
/// Returns `true` when the value actually changed, so callers can log
/// transitions instead of every poll.
pub fn set_live_feed_collector(collecting: bool) -> bool {
    LIVE_FEED_COLLECTOR.swap(collecting, Ordering::AcqRel) != collecting
}

/// Whether this device currently records live feed observations.
pub fn is_live_feed_collector() -> bool {
    LIVE_FEED_COLLECTOR.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::{is_live_feed_collector, set_live_feed_collector};

    /// The flag is process-global, so this single test owns it end to end.
    #[test]
    fn defaults_on_and_reports_transitions() {
        assert!(is_live_feed_collector(), "fail-open default");
        assert!(set_live_feed_collector(false), "first flip reports change");
        assert!(!is_live_feed_collector());
        assert!(
            !set_live_feed_collector(false),
            "same value is not a change"
        );
        assert!(set_live_feed_collector(true));
        assert!(is_live_feed_collector());
    }
}
