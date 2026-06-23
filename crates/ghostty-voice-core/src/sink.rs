//! Sink registry / active **Delivery sink** (IDEAS.md #4, CONTEXT.md).
//!
//! Delivery is to exactly **one active Delivery sink** at any moment. There are
//! two kinds:
//! - the **focused-window sink** — the default floor, types into whatever window
//!   is focused via `ydotool` (still gated by the Freshness window);
//! - a **wrapper sink** — a running `talk-to`, delivery pushed to its exact PTY.
//!
//! Switching is sequential, never concurrent: launching a wrapper makes its sink
//! active (lifecycle-implicit switching — there is no explicit switch command in
//! v1). When the active wrapper deregisters or dies, the **most-recently-registered
//! still-live** wrapper sink takes over (the newest-live handoff, task-11); the
//! focused-window sink reactivates only when the **last** wrapper exits. An
//! utterance's target sink is **bound when it is triggered**,
//! and is never silently redirected: if the bound wrapper sink is gone at
//! delivery, the Transcript is **Held-for-replay**, not dumped into whatever is
//! focused now.

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
///
/// `live` holds the live wrapper ids in **registration order** (oldest first,
/// newest last) so the newest-live handoff (task-11) can pick the
/// most-recently-registered survivor when the active wrapper leaves.
#[derive(Debug)]
pub struct SinkRegistry {
    live: Vec<u64>,
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
            live: Vec::new(),
            active: ActiveSink::FocusedWindow,
            next_id: 0,
        }
    }

    /// Register a new wrapper sink; it becomes the active sink. Launching a
    /// `talk-to` makes its sink active (lifecycle-implicit switching, v1).
    pub fn register(&mut self) -> SinkId {
        let id = self.next_id;
        self.next_id += 1;
        self.live.push(id); // newest-last, preserving registration order
        self.active = ActiveSink::Wrapper(SinkId(id));
        SinkId(id)
    }

    /// Deregister a wrapper sink (it exited or its connection dropped). If it was
    /// the **active** sink, the **most-recently-registered still-live** wrapper
    /// takes over (the newest-live handoff); the focused-window sink reactivates
    /// only when no wrapper remains. Deregistering a non-active wrapper leaves the
    /// active sink untouched.
    pub fn deregister(&mut self, id: SinkId) {
        self.live.retain(|&live_id| live_id != id.0);
        if self.active == ActiveSink::Wrapper(id) {
            self.active = match self.live.last() {
                Some(&newest) => ActiveSink::Wrapper(SinkId(newest)),
                None => ActiveSink::FocusedWindow,
            };
        }
    }

    /// The sink active right now. Snapshot this at trigger time to bind an
    /// utterance's target sink.
    pub fn active(&self) -> ActiveSink {
        self.active
    }

    /// How many **wrapper sinks** are currently registered (live). Surfaced by
    /// `ghostty-voice-ctl status` (task-10.1).
    pub fn wrapper_count(&self) -> usize {
        self.live.len()
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
    fn deregistering_the_active_wrapper_hands_off_to_the_newest_live_wrapper() {
        // Multi-wrapper correctness (task-11): with several wrappers registered,
        // closing the active one hands off to the most-recently-registered
        // still-live wrapper — NOT down to the focused-window sink, and NOT to the
        // oldest survivor. Three wrappers prove it picks the newest survivor (b),
        // not the oldest (a).
        let mut r = SinkRegistry::new();
        let a = r.register();
        let b = r.register();
        let c = r.register(); // c is active
        assert_eq!(r.active(), ActiveSink::Wrapper(c));
        r.deregister(c);
        assert_eq!(
            r.active(),
            ActiveSink::Wrapper(b),
            "hands off to the newest still-live wrapper, not the oldest or focused-window",
        );
        assert!(r.is_live(a) && r.is_live(b) && !r.is_live(c));
    }

    #[test]
    fn deregistration_peels_back_to_focused_window_only_when_the_last_wrapper_exits() {
        let mut r = SinkRegistry::new();
        let a = r.register();
        let b = r.register();
        let c = r.register();
        r.deregister(c);
        assert_eq!(r.active(), ActiveSink::Wrapper(b));
        r.deregister(b);
        assert_eq!(r.active(), ActiveSink::Wrapper(a));
        r.deregister(a);
        assert_eq!(
            r.active(),
            ActiveSink::FocusedWindow,
            "the focused-window sink returns only when the last wrapper exits",
        );
    }

    #[test]
    fn deregistering_a_non_active_wrapper_never_changes_the_active_sink() {
        // Even with three wrappers, removing any non-active one leaves the active
        // sink untouched (it only changes the handoff candidates for later).
        let mut r = SinkRegistry::new();
        let a = r.register();
        let b = r.register();
        let c = r.register(); // c active
        r.deregister(a);
        assert_eq!(r.active(), ActiveSink::Wrapper(c));
        r.deregister(b);
        assert_eq!(r.active(), ActiveSink::Wrapper(c));
        // Now only c is live; closing it falls through to the focused-window sink.
        r.deregister(c);
        assert_eq!(r.active(), ActiveSink::FocusedWindow);
    }

    #[test]
    fn a_handoff_never_redirects_an_utterance_bound_to_the_now_dead_wrapper() {
        // An utterance bound to wrapper A while it was active; A then dies and the
        // registry hands off to wrapper B. The utterance must still be Held — never
        // redirected to B just because B is the active sink now.
        let mut r = SinkRegistry::new();
        let a = r.register();
        let _b = r.register(); // b is active now; bind to a explicitly
        let bound = ActiveSink::Wrapper(a);
        // A dies; with b still live, the active sink stays a wrapper (b), but the
        // bound utterance is for the dead a.
        r.deregister(a);
        assert!(matches!(r.active(), ActiveSink::Wrapper(_)));
        assert_eq!(
            r.route(bound),
            Route::Held,
            "a dead bound wrapper Holds even when a handoff kept a wrapper active",
        );
    }

    #[test]
    fn wrapper_count_tracks_the_number_of_live_wrappers() {
        // Surfaced by `status` (task-10.1): how many wrapper sinks are registered.
        let mut r = SinkRegistry::new();
        assert_eq!(r.wrapper_count(), 0);
        let a = r.register();
        let b = r.register();
        assert_eq!(r.wrapper_count(), 2);
        r.deregister(a);
        assert_eq!(r.wrapper_count(), 1);
        r.deregister(b);
        assert_eq!(r.wrapper_count(), 0);
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
