//! Ordered delivery queue (S3) — the pure ordering core.
//!
//! Utterances are delivered in strict record-order with no interleaving: the
//! head utterance must reach a terminal state (typed, dropped, or held) before
//! the next may type. If the head is still transcribing, the queue blocks —
//! a ready later utterance does *not* jump ahead. The daemon drives this; the
//! invariant lives here, tested.

use std::collections::VecDeque;
use std::time::Duration;

use crate::delivery::{self, Delivery};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ItemState {
    /// Recording or transcribing — not yet ready.
    Pending,
    /// Transcript ready, awaiting its turn to type.
    Ready(String),
}

#[derive(Debug, Clone)]
struct Item {
    seq: u64,
    /// When this utterance's recording ended, as an offset on the daemon's
    /// monotonic clock. The freshness deadline is measured from here, so each
    /// utterance is judged on its own age — never a shared queue timer.
    record_end: Duration,
    state: ItemState,
}

/// A FIFO of utterances delivered in strict record-order.
#[derive(Debug, Default)]
pub struct DeliveryQueue {
    items: VecDeque<Item>,
    next_seq: u64,
}

impl DeliveryQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a new (pending) utterance recorded ending at `record_end`,
    /// returning its sequence number.
    pub fn enqueue_at(&mut self, record_end: Duration) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.items.push_back(Item {
            seq,
            record_end,
            state: ItemState::Pending,
        });
        seq
    }

    /// Enqueue with a zero record-end (ordering-only tests / callers that don't
    /// care about freshness).
    pub fn enqueue(&mut self) -> u64 {
        self.enqueue_at(Duration::ZERO)
    }

    /// Mark an utterance's transcript ready.
    pub fn set_ready(&mut self, seq: u64, transcript: String) {
        if let Some(item) = self.items.iter_mut().find(|i| i.seq == seq) {
            item.state = ItemState::Ready(transcript);
        }
    }

    /// Resolve an utterance terminally (typed, dropped, or held) and drop any
    /// resolved entries from the front so the next can advance.
    pub fn resolve(&mut self, seq: u64) {
        self.items.retain(|i| i.seq != seq);
    }

    /// The head utterance ready to type *now*, or `None` if the head is still
    /// pending (strict order — never skip ahead) or the queue is empty. The
    /// freshness-aware [`Self::head_delivery`] is what the daemon drains by; this
    /// is the decision-free view (delegating to it) used by tests.
    pub fn next_to_type(&self) -> Option<(u64, &str)> {
        self.head_delivery(Duration::ZERO, Duration::MAX)
            .map(|(seq, transcript, _)| (seq, transcript))
    }

    /// The head utterance ready to type *now*, with its freshness-based
    /// delivery decision (auto-type vs hold-for-replay). `None` if the head is
    /// still pending or the queue is empty. The decision uses the head's own
    /// record-end age, so a stale head is held while a later fresh utterance
    /// still types once it reaches the front.
    pub fn head_delivery(&self, now: Duration, window: Duration) -> Option<(u64, &str, Delivery)> {
        let head = self.items.front()?;
        match &head.state {
            ItemState::Ready(transcript) => {
                let since = now.saturating_sub(head.record_end);
                Some((head.seq, transcript, delivery::decide(since, window)))
            }
            ItemState::Pending => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn empty_queue_has_nothing_to_type() {
        assert!(DeliveryQueue::new().next_to_type().is_none());
    }

    #[test]
    fn a_later_ready_utterance_waits_for_an_earlier_pending_one() {
        let mut q = DeliveryQueue::new();
        let a = q.enqueue();
        let b = q.enqueue();
        q.set_ready(b, "second".to_owned());
        // #1 (a) is still pending -> #2 (b) must NOT jump ahead.
        assert_eq!(q.next_to_type(), None);
        q.set_ready(a, "first".to_owned());
        assert_eq!(q.next_to_type(), Some((a, "first")));
    }

    #[test]
    fn resolving_the_head_advances_to_the_next() {
        let mut q = DeliveryQueue::new();
        let a = q.enqueue();
        let b = q.enqueue();
        q.set_ready(a, "first".to_owned());
        q.set_ready(b, "second".to_owned());
        assert_eq!(q.next_to_type(), Some((a, "first")));
        q.resolve(a);
        assert_eq!(q.next_to_type(), Some((b, "second")));
        q.resolve(b);
        assert!(q.is_empty());
    }

    #[test]
    fn a_dropped_pending_head_unblocks_the_queue() {
        let mut q = DeliveryQueue::new();
        let a = q.enqueue();
        let b = q.enqueue();
        q.set_ready(b, "second".to_owned());
        assert_eq!(q.next_to_type(), None); // blocked on pending a
        q.resolve(a); // a dropped (e.g. empty/discarded)
        assert_eq!(q.next_to_type(), Some((b, "second")));
    }

    // ---- freshness at the head ------------------------------------------

    const WINDOW: Duration = Duration::from_secs(900);

    #[test]
    fn a_fresh_ready_head_is_delivered_as_auto_type() {
        let mut q = DeliveryQueue::new();
        let a = q.enqueue_at(Duration::from_secs(100));
        q.set_ready(a, "hello".to_owned());
        // produced 3 s after record-end -> well inside the window.
        let now = Duration::from_secs(103);
        assert_eq!(
            q.head_delivery(now, WINDOW),
            Some((a, "hello", Delivery::AutoType)),
        );
    }

    #[test]
    fn a_stale_ready_head_is_held_for_replay() {
        let mut q = DeliveryQueue::new();
        let a = q.enqueue_at(Duration::from_secs(0));
        q.set_ready(a, "hello".to_owned());
        // produced long after record-end, past the window.
        let now = WINDOW + Duration::from_secs(1);
        assert_eq!(
            q.head_delivery(now, WINDOW),
            Some((a, "hello", Delivery::HoldForReplay)),
        );
    }

    #[test]
    fn a_pending_head_has_no_delivery_decision_yet() {
        let mut q = DeliveryQueue::new();
        q.enqueue_at(Duration::from_secs(0));
        assert_eq!(q.head_delivery(Duration::from_secs(1), WINDOW), None);
    }

    #[test]
    fn record_end_is_tracked_per_utterance_not_shared() {
        let mut q = DeliveryQueue::new();
        let a = q.enqueue_at(Duration::from_secs(0)); // recorded long ago
        let b = q.enqueue_at(Duration::from_secs(1000)); // recorded recently
        q.set_ready(a, "old".to_owned());
        q.set_ready(b, "new".to_owned());
        let now = Duration::from_secs(1003);
        // head is a, which is stale on its own deadline.
        assert_eq!(
            q.head_delivery(now, WINDOW),
            Some((a, "old", Delivery::HoldForReplay)),
        );
        q.resolve(a);
        // b is fresh on its own deadline.
        assert_eq!(
            q.head_delivery(now, WINDOW),
            Some((b, "new", Delivery::AutoType)),
        );
    }
}
