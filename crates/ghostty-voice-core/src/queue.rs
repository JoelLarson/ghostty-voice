//! Ordered delivery queue — the pure ordering core.
//!
//! Utterances are delivered in strict record-order with no interleaving: the
//! head utterance must reach a terminal state (typed, dropped, or held) before
//! the next may type. If the head is still transcribing, the queue blocks —
//! a ready later utterance does *not* jump ahead. The daemon drives this; the
//! invariant lives here, tested.

use std::collections::VecDeque;

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

    /// Enqueue a new (pending) utterance, returning its sequence number.
    pub fn enqueue(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.items.push_back(Item {
            seq,
            state: ItemState::Pending,
        });
        seq
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
    /// pending (strict order — never skip ahead) or the queue is empty.
    pub fn next_to_type(&self) -> Option<(u64, &str)> {
        let head = self.items.front()?;
        match &head.state {
            ItemState::Ready(transcript) => Some((head.seq, transcript)),
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
}
