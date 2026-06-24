//! Sink registry / active **Delivery sink** (IDEAS.md #4, CONTEXT.md).
//!
//! Delivery is to exactly **one active Delivery sink** at any moment, and the
//! only kind of sink is a **wrapper sink** — a running `talk-to`, delivery
//! pushed to its exact PTY. With no wrapper registered there is **no active
//! sink**: there is nowhere to type, so a triggered utterance is Held-for-replay.
//!
//! Switching is sequential, never concurrent: launching a wrapper makes its sink
//! active (lifecycle-implicit switching — there is no explicit switch command in
//! v1). When the active wrapper deregisters or dies, the **most-recently-registered
//! still-live** wrapper sink takes over (the newest-live handoff); the active
//! sink falls back to *none* only when the **last** wrapper exits. An utterance's
//! target sink is **bound when it is triggered**, and is never silently
//! redirected: if the bound wrapper sink is gone at delivery, the Transcript is
//! **Held-for-replay**.

/// Identifies a registered **wrapper sink** (a running `talk-to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SinkId(pub u64);

/// Where a bound utterance's Transcript should actually go at delivery time,
/// computed from its trigger-time binding and the registry state now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Push to this live wrapper sink — its PTY target is exact, so there is no
    /// "wrong-window" risk.
    Wrapper(SinkId),
    /// The bound wrapper sink is gone (or nothing was bound) — **Held-for-replay**,
    /// never silently redirected to whatever is active now.
    Held,
}

/// Tracks registered wrapper sinks and which single sink is active.
///
/// `live` holds the live wrapper ids in **registration order** (oldest first,
/// newest last) so the newest-live handoff can pick the most-recently-registered
/// survivor when the active wrapper leaves.
#[derive(Debug)]
pub struct SinkRegistry {
    live: Vec<u64>,
    /// The id of the wrapper that is the active sink, or `None` when no wrapper
    /// is registered (nowhere to deliver).
    active: Option<u64>,
    next_id: u64,
}

impl Default for SinkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SinkRegistry {
    /// A fresh registry with no wrapper — there is no active sink yet.
    pub fn new() -> Self {
        Self {
            live: Vec::new(),
            active: None,
            next_id: 0,
        }
    }

    /// Register a new wrapper sink; it becomes the active sink. Launching a
    /// `talk-to` makes its sink active (lifecycle-implicit switching, v1).
    pub fn register(&mut self) -> SinkId {
        let id = self.next_id;
        self.next_id += 1;
        self.live.push(id); // newest-last, preserving registration order
        self.active = Some(id);
        SinkId(id)
    }

    /// Deregister a wrapper sink (it exited or its connection dropped). If it was
    /// the **active** sink, the **most-recently-registered still-live** wrapper
    /// takes over (the newest-live handoff); the active sink becomes *none* only
    /// when no wrapper remains. Deregistering a non-active wrapper leaves the
    /// active sink untouched.
    pub fn deregister(&mut self, id: SinkId) {
        self.live.retain(|&live_id| live_id != id.0);
        if self.active == Some(id.0) {
            self.active = self.live.last().copied();
        }
    }

    /// The sink active right now, or `None` when no wrapper is registered.
    /// Snapshot this at trigger time to bind an utterance's target sink.
    pub fn active(&self) -> Option<SinkId> {
        self.active.map(SinkId)
    }

    /// How many **wrapper sinks** are currently registered (live). Surfaced by
    /// `ghostty-voice-ctl status`.
    pub fn wrapper_count(&self) -> usize {
        self.live.len()
    }

    /// Is this wrapper sink still registered (its `talk-to` still alive)?
    pub fn is_live(&self, id: SinkId) -> bool {
        self.live.contains(&id.0)
    }

    /// Route a Transcript whose target sink was bound to `bound` at trigger time,
    /// using the registry state *now*. A bound wrapper that has since died — or a
    /// binding to nothing (no wrapper was active at trigger time) — routes to
    /// [`Route::Held`], never redirected to whatever is active now.
    pub fn route(&self, bound: Option<SinkId>) -> Route {
        match bound {
            Some(id) if self.is_live(id) => Route::Wrapper(id),
            _ => Route::Held,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_registry_has_no_active_sink() {
        let r = SinkRegistry::new();
        assert_eq!(r.active(), None);
    }

    #[test]
    fn registering_a_wrapper_makes_it_the_active_sink() {
        let mut r = SinkRegistry::new();
        let id = r.register();
        assert_eq!(r.active(), Some(id));
        assert!(r.is_live(id));
    }

    #[test]
    fn deregistering_the_only_wrapper_leaves_no_active_sink() {
        let mut r = SinkRegistry::new();
        let id = r.register();
        r.deregister(id);
        assert_eq!(r.active(), None);
        assert!(!r.is_live(id));
    }

    #[test]
    fn exactly_one_sink_is_active_even_with_several_registered() {
        let mut r = SinkRegistry::new();
        let _a = r.register();
        let b = r.register();
        // The most-recently launched wrapper is the single active sink.
        assert_eq!(r.active(), Some(b));
    }

    #[test]
    fn deregistering_a_non_active_wrapper_leaves_the_active_one_untouched() {
        let mut r = SinkRegistry::new();
        let a = r.register();
        let b = r.register(); // b is active
        r.deregister(a);
        assert_eq!(r.active(), Some(b));
        assert!(!r.is_live(a));
        assert!(r.is_live(b));
    }

    #[test]
    fn deregistering_the_active_wrapper_hands_off_to_the_newest_live_wrapper() {
        // Multi-wrapper correctness: with several wrappers registered, closing the
        // active one hands off to the most-recently-registered still-live wrapper —
        // NOT down to *no sink*, and NOT to the oldest survivor. Three wrappers
        // prove it picks the newest survivor (b), not the oldest (a).
        let mut r = SinkRegistry::new();
        let a = r.register();
        let b = r.register();
        let c = r.register(); // c is active
        assert_eq!(r.active(), Some(c));
        r.deregister(c);
        assert_eq!(
            r.active(),
            Some(b),
            "hands off to the newest still-live wrapper, not the oldest or none",
        );
        assert!(r.is_live(a) && r.is_live(b) && !r.is_live(c));
    }

    #[test]
    fn deregistration_falls_back_to_no_sink_only_when_the_last_wrapper_exits() {
        let mut r = SinkRegistry::new();
        let a = r.register();
        let b = r.register();
        let c = r.register();
        r.deregister(c);
        assert_eq!(r.active(), Some(b));
        r.deregister(b);
        assert_eq!(r.active(), Some(a));
        r.deregister(a);
        assert_eq!(
            r.active(),
            None,
            "the active sink becomes none only when the last wrapper exits",
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
        assert_eq!(r.active(), Some(c));
        r.deregister(b);
        assert_eq!(r.active(), Some(c));
        // Now only c is live; closing it leaves no active sink.
        r.deregister(c);
        assert_eq!(r.active(), None);
    }

    #[test]
    fn a_handoff_never_redirects_an_utterance_bound_to_the_now_dead_wrapper() {
        // An utterance bound to wrapper A while it was active; A then dies and the
        // registry hands off to wrapper B. The utterance must still be Held — never
        // redirected to B just because B is the active sink now.
        let mut r = SinkRegistry::new();
        let a = r.register();
        let _b = r.register(); // b is active now; bind to a explicitly
        let bound = Some(a);
        // A dies; with b still live, the active sink stays a wrapper (b), but the
        // bound utterance is for the dead a.
        r.deregister(a);
        assert!(r.active().is_some());
        assert_eq!(
            r.route(bound),
            Route::Held,
            "a dead bound wrapper Holds even when a handoff kept a wrapper active",
        );
    }

    #[test]
    fn wrapper_count_tracks_the_number_of_live_wrappers() {
        // Surfaced by `status`: how many wrapper sinks are registered.
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
    fn a_binding_to_nothing_is_held() {
        // No wrapper was active at trigger time → there is nowhere to deliver.
        let r = SinkRegistry::new();
        assert_eq!(r.route(None), Route::Held);
    }

    #[test]
    fn a_live_wrapper_binding_routes_to_that_wrapper() {
        let mut r = SinkRegistry::new();
        let id = r.register();
        assert_eq!(r.route(Some(id)), Route::Wrapper(id));
    }

    #[test]
    fn a_dead_wrapper_binding_is_held_not_redirected() {
        // Trigger-time binding: snapshot the active sink, then the wrapper dies.
        let mut r = SinkRegistry::new();
        let bound = r.register();
        r.deregister(bound);
        assert_eq!(r.route(Some(bound)), Route::Held);
    }

    #[test]
    fn a_binding_is_never_silently_redirected_to_a_later_wrapper() {
        // Utterance bound to wrapper A while it was active; wrapper B launches and
        // becomes active. A is still live, so the utterance still goes to A — not
        // redirected to whatever is active now.
        let mut r = SinkRegistry::new();
        let a = r.register();
        let bound = r.active(); // Some(a)
        let b = r.register(); // B now active
        assert_eq!(r.active(), Some(b));
        assert_eq!(r.route(bound), Route::Wrapper(a));
    }
}
