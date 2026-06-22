//! Sink registry / active **Delivery sink** (IDEAS.md #4, CONTEXT.md).
//!
//! Delivery is to exactly **one active Delivery sink** at any moment. There are
//! two kinds:
//! - the **focused-window sink** — the default floor, types into whatever window
//!   is focused via `ydotool` (still gated by the Freshness window);
//! - a **wrapper sink** — a running `talk-to`, delivery pushed to its exact PTY.
//!
//! Switching is sequential, never concurrent: launching a wrapper makes its sink
//! active; when the wrapper deregisters or dies, the focused-window sink
//! reactivates (lifecycle-implicit switching — there is no explicit switch
//! command in v1). An utterance's target sink is **bound when it is triggered**,
//! and is never silently redirected: if the bound wrapper sink is gone at
//! delivery, the Transcript is **Held-for-replay**, not dumped into whatever is
//! focused now.

use std::collections::HashSet;

/// Identifies a registered **wrapper sink** (a running `talk-to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SinkId(pub u64);

/// The Delivery sink active *right now*. Exactly one is active at a time. This is
/// also what gets snapshotted as an utterance's bound target at trigger time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveSink {
    /// The default floor: type into whatever window is focused (`ydotool`).
    FocusedWindow,
    /// A registered wrapper sink (`talk-to`) — delivery is to its exact PTY.
    Wrapper(SinkId),
}

/// Where a bound utterance's Transcript should actually go at delivery time,
/// computed from its trigger-time binding and the registry state now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Deliver via the focused-window sink (the daemon still applies the
    /// Freshness window to it).
    FocusedWindow,
    /// Push to this live wrapper sink. The Freshness window does **not** apply —
    /// the PTY target is exact, so there is no "wrong-window" risk.
    Wrapper(SinkId),
    /// The bound wrapper sink is gone — **Held-for-replay**, never silently
    /// redirected to whatever window is focused now.
    Held,
}

/// Tracks registered wrapper sinks and which single sink is active.
#[derive(Debug)]
pub struct SinkRegistry {
    live: HashSet<u64>,
    active: ActiveSink,
    next_id: u64,
}

impl Default for SinkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SinkRegistry {
    /// A fresh registry with only the focused-window sink — the default floor.
    pub fn new() -> Self {
        Self {
            live: HashSet::new(),
            active: ActiveSink::FocusedWindow,
            next_id: 0,
        }
    }

    /// Register a new wrapper sink; it becomes the active sink. Launching a
    /// `talk-to` makes its sink active (lifecycle-implicit switching, v1).
    pub fn register(&mut self) -> SinkId {
        let id = self.next_id;
        self.next_id += 1;
        self.live.insert(id);
        self.active = ActiveSink::Wrapper(SinkId(id));
        SinkId(id)
    }

    /// Deregister a wrapper sink (it exited or its connection dropped). If it was
    /// the active sink, the focused-window sink reactivates — the default floor.
    pub fn deregister(&mut self, id: SinkId) {
        self.live.remove(&id.0);
        if self.active == ActiveSink::Wrapper(id) {
            self.active = ActiveSink::FocusedWindow;
        }
    }

    /// The sink active right now. Snapshot this at trigger time to bind an
    /// utterance's target sink.
    pub fn active(&self) -> ActiveSink {
        self.active
    }

    /// Is this wrapper sink still registered (its `talk-to` still alive)?
    pub fn is_live(&self, id: SinkId) -> bool {
        self.live.contains(&id.0)
    }

    /// Route a Transcript whose target sink was bound to `bound` at trigger time,
    /// using the registry state *now*. A bound wrapper that has since died routes
    /// to [`Route::Held`] — never redirected to the current focus.
    pub fn route(&self, bound: ActiveSink) -> Route {
        match bound {
            ActiveSink::FocusedWindow => Route::FocusedWindow,
            ActiveSink::Wrapper(id) => {
                if self.is_live(id) {
                    Route::Wrapper(id)
                } else {
                    Route::Held
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_registry_is_on_the_focused_window_sink() {
        let r = SinkRegistry::new();
        assert_eq!(r.active(), ActiveSink::FocusedWindow);
    }

    #[test]
    fn registering_a_wrapper_makes_it_the_active_sink() {
        let mut r = SinkRegistry::new();
        let id = r.register();
        assert_eq!(r.active(), ActiveSink::Wrapper(id));
        assert!(r.is_live(id));
    }

    #[test]
    fn deregistering_the_active_wrapper_reactivates_the_focused_window_sink() {
        let mut r = SinkRegistry::new();
        let id = r.register();
        r.deregister(id);
        assert_eq!(r.active(), ActiveSink::FocusedWindow);
        assert!(!r.is_live(id));
    }

    #[test]
    fn exactly_one_sink_is_active_even_with_several_registered() {
        let mut r = SinkRegistry::new();
        let _a = r.register();
        let b = r.register();
        // The most-recently launched wrapper is the single active sink.
        assert_eq!(r.active(), ActiveSink::Wrapper(b));
    }

    #[test]
    fn deregistering_a_non_active_wrapper_leaves_the_active_one_untouched() {
        let mut r = SinkRegistry::new();
        let a = r.register();
        let b = r.register(); // b is active
        r.deregister(a);
        assert_eq!(r.active(), ActiveSink::Wrapper(b));
        assert!(!r.is_live(a));
        assert!(r.is_live(b));
    }

    #[test]
    fn a_focused_window_binding_routes_to_the_focused_window_sink() {
        let r = SinkRegistry::new();
        assert_eq!(r.route(ActiveSink::FocusedWindow), Route::FocusedWindow);
    }

    #[test]
    fn a_live_wrapper_binding_routes_to_that_wrapper() {
        let mut r = SinkRegistry::new();
        let id = r.register();
        assert_eq!(r.route(ActiveSink::Wrapper(id)), Route::Wrapper(id));
    }

    #[test]
    fn a_dead_wrapper_binding_is_held_not_redirected() {
        // Trigger-time binding: snapshot the active sink, then the wrapper dies.
        let mut r = SinkRegistry::new();
        let bound = {
            let id = r.register();
            r.active(); // bound = Wrapper(id)
            ActiveSink::Wrapper(id)
        };
        if let ActiveSink::Wrapper(id) = bound {
            r.deregister(id);
        }
        assert_eq!(r.route(bound), Route::Held);
    }

    #[test]
    fn a_binding_is_never_silently_redirected_to_a_later_wrapper() {
        // Utterance bound to wrapper A while it was active; wrapper B launches and
        // becomes active. A is still live, so the utterance still goes to A — not
        // redirected to whatever is active now.
        let mut r = SinkRegistry::new();
        let a = r.register();
        let bound = r.active(); // Wrapper(a)
        let b = r.register(); // B now active
        assert_eq!(r.active(), ActiveSink::Wrapper(b));
        assert_eq!(r.route(bound), Route::Wrapper(a));
    }

    #[test]
    fn a_focused_window_binding_is_never_redirected_to_a_wrapper_that_launched_later() {
        // Bound to focused-window at trigger; a wrapper registers afterward. The
        // utterance still goes to the focused-window sink, never the new wrapper.
        let mut r = SinkRegistry::new();
        let bound = r.active(); // FocusedWindow
        let _w = r.register();
        assert_eq!(r.route(bound), Route::FocusedWindow);
    }
}
