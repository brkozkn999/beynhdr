//! In-memory per-recipient message queues behind the `bus.*` API methods.
//!
//! Mailboxes are keyed by the recipient's internal `PaneId`, so queued
//! messages stay attached to the pane across public pane renumbering
//! (`pane.move`). The store is transient runtime state: it is never written
//! into the session snapshot and drops entirely on server restart.
//!
//! All entry points run on the API dispatch path, so the store needs no
//! locking.

use std::collections::{HashMap, VecDeque};

use crate::layout::PaneId;

/// Maximum queued messages retained per recipient mailbox.
pub(crate) const MAX_MESSAGES_PER_MAILBOX: usize = 64;
/// Maximum UTF-8 byte length accepted for one `bus.send` message body.
pub(crate) const MAX_MESSAGE_BYTES: usize = 8 * 1024;
/// Maximum UTF-8 byte length accepted for the optional `from` label.
pub(crate) const MAX_FROM_BYTES: usize = 256;

/// Why a `bus.send` request was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BusEnqueueError {
    /// `text` was empty.
    EmptyMessage,
    /// `text` exceeded `MAX_MESSAGE_BYTES`.
    MessageTooLarge { bytes: usize },
    /// `from` was provided but empty after trimming.
    EmptyFrom,
    /// `from` exceeded `MAX_FROM_BYTES`.
    FromTooLong { bytes: usize },
    /// The mailbox already holds `MAX_MESSAGES_PER_MAILBOX` pending messages.
    QueueFull,
}

#[derive(Debug)]
struct Mailbox {
    /// Next message id for this mailbox. Ids are per-mailbox and
    /// monotonically increasing; they are never reused, even after the queue
    /// drains fully.
    next_id: u64,
    queue: VecDeque<crate::api::schema::BusMessage>,
}

impl Mailbox {
    fn new() -> Self {
        Self {
            next_id: 1,
            queue: VecDeque::new(),
        }
    }
}

/// Per-recipient FIFO mailboxes for the `bus.*` API surface.
#[derive(Debug, Default)]
pub(crate) struct AgentBus {
    mailboxes: HashMap<PaneId, Mailbox>,
}

impl AgentBus {
    /// Queue a message for `pane_id`, assigning the next per-mailbox id.
    ///
    /// `to`/`workspace_id` are the recipient's *public* ids at send time; they
    /// are snapshots for display, not the mailbox key. Returns the stored
    /// record and the resulting mailbox depth.
    pub(crate) fn send(
        &mut self,
        pane_id: PaneId,
        to: String,
        workspace_id: String,
        from: Option<String>,
        text: String,
        queued_unix_ms: u64,
    ) -> Result<(crate::api::schema::BusMessage, u32), BusEnqueueError> {
        if text.is_empty() {
            return Err(BusEnqueueError::EmptyMessage);
        }
        if text.len() > MAX_MESSAGE_BYTES {
            return Err(BusEnqueueError::MessageTooLarge { bytes: text.len() });
        }
        if let Some(from) = &from {
            if from.trim().is_empty() {
                return Err(BusEnqueueError::EmptyFrom);
            }
            if from.len() > MAX_FROM_BYTES {
                return Err(BusEnqueueError::FromTooLong { bytes: from.len() });
            }
        }
        let mailbox = self.mailboxes.entry(pane_id).or_insert_with(Mailbox::new);
        if mailbox.queue.len() >= MAX_MESSAGES_PER_MAILBOX {
            return Err(BusEnqueueError::QueueFull);
        }
        let id = mailbox.next_id;
        mailbox.next_id += 1;
        let message = crate::api::schema::BusMessage {
            id,
            to,
            workspace_id,
            from,
            text,
            queued_unix_ms,
        };
        mailbox.queue.push_back(message.clone());
        Ok((message, mailbox.queue.len() as u32))
    }

    /// Pending messages for `pane_id` in FIFO order, without consuming them.
    pub(crate) fn pending(&self, pane_id: PaneId) -> Vec<crate::api::schema::BusMessage> {
        self.mailboxes
            .get(&pane_id)
            .map(|mailbox| mailbox.queue.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Pop up to `max` pending messages in FIFO order (`None` drains all).
    pub(crate) fn read(
        &mut self,
        pane_id: PaneId,
        max: Option<usize>,
    ) -> Vec<crate::api::schema::BusMessage> {
        let Some(mailbox) = self.mailboxes.get_mut(&pane_id) else {
            return Vec::new();
        };
        let take = max.unwrap_or(usize::MAX).min(mailbox.queue.len());
        mailbox.queue.drain(..take).collect()
    }

    /// Remove a single pending message by id. Returns `false` when no pending
    /// message with that id exists (already read, already acked, or never
    /// queued).
    pub(crate) fn ack(&mut self, pane_id: PaneId, message_id: u64) -> bool {
        let Some(mailbox) = self.mailboxes.get_mut(&pane_id) else {
            return false;
        };
        let Some(position) = mailbox
            .queue
            .iter()
            .position(|message| message.id == message_id)
        else {
            return false;
        };
        mailbox.queue.remove(position);
        true
    }

    /// Drop mailboxes whose recipient pane no longer exists. Queued messages
    /// are discarded together with the mailbox.
    pub(crate) fn prune_closed(&mut self, state: &crate::app::AppState) {
        self.mailboxes.retain(|pane_id, _| {
            state
                .workspaces
                .iter()
                .any(|workspace| workspace.pane_state(*pane_id).is_some())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(raw: u32) -> PaneId {
        PaneId::from_raw(raw)
    }

    fn send(bus: &mut AgentBus, raw_pane: u32, text: &str) {
        bus.send(
            pane(raw_pane),
            format!("w1:p{raw_pane}"),
            "w1".into(),
            None,
            text.to_string(),
            1_000,
        )
        .unwrap();
    }

    #[test]
    fn messages_dequeue_in_fifo_order() {
        let mut bus = AgentBus::default();
        send(&mut bus, 1, "first");
        send(&mut bus, 1, "second");
        send(&mut bus, 2, "other-recipient");

        let drained = bus.read(pane(1), None);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].text, "first");
        assert_eq!(drained[0].id, 1);
        assert_eq!(drained[1].text, "second");
        assert_eq!(drained[1].id, 2);
        // Other mailboxes are untouched.
        assert_eq!(bus.pending(pane(2)).len(), 1);
        // Drained messages are gone.
        assert!(bus.read(pane(1), None).is_empty());
    }

    #[test]
    fn read_max_limits_drain_without_losing_the_rest() {
        let mut bus = AgentBus::default();
        send(&mut bus, 1, "a");
        send(&mut bus, 1, "b");
        send(&mut bus, 1, "c");

        let drained = bus.read(pane(1), Some(2));
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].text, "a");
        assert_eq!(bus.pending(pane(1)).len(), 1);
        assert_eq!(bus.pending(pane(1))[0].text, "c");
    }

    #[test]
    fn mailbox_rejects_messages_beyond_capacity() {
        let mut bus = AgentBus::default();
        for index in 0..MAX_MESSAGES_PER_MAILBOX {
            send(&mut bus, 1, &format!("message-{index}"));
        }
        let error = bus
            .send(
                pane(1),
                "w1:p1".into(),
                "w1".into(),
                None,
                "one-too-many".into(),
                1_000,
            )
            .unwrap_err();
        assert_eq!(error, BusEnqueueError::QueueFull);
        assert_eq!(bus.pending(pane(1)).len(), MAX_MESSAGES_PER_MAILBOX);
    }

    #[test]
    fn mailbox_rejects_oversized_message() {
        let mut bus = AgentBus::default();
        let too_large = "x".repeat(MAX_MESSAGE_BYTES + 1);
        let error = bus
            .send(pane(1), "w1:p1".into(), "w1".into(), None, too_large, 1_000)
            .unwrap_err();
        assert_eq!(
            error,
            BusEnqueueError::MessageTooLarge {
                bytes: MAX_MESSAGE_BYTES + 1
            }
        );
        // Nothing was queued.
        assert!(bus.pending(pane(1)).is_empty());
    }

    #[test]
    fn mailbox_rejects_empty_message() {
        let mut bus = AgentBus::default();
        let error = bus
            .send(pane(1), "w1:p1".into(), "w1".into(), None, String::new(), 0)
            .unwrap_err();
        assert_eq!(error, BusEnqueueError::EmptyMessage);
    }

    #[test]
    fn ack_removes_a_pending_message_once() {
        let mut bus = AgentBus::default();
        send(&mut bus, 1, "a");
        send(&mut bus, 1, "b");

        assert!(bus.ack(pane(1), 1));
        assert!(!bus.ack(pane(1), 1), "second ack must not remove again");
        assert!(!bus.ack(pane(1), 99), "unknown id reports not removed");
        assert_eq!(bus.pending(pane(1)).len(), 1);
        assert_eq!(bus.pending(pane(1))[0].text, "b");
    }

    #[test]
    fn message_ids_stay_monotonic_across_drains() {
        let mut bus = AgentBus::default();
        send(&mut bus, 1, "a");
        bus.read(pane(1), None);
        let (message, _) = bus
            .send(
                pane(1),
                "w1:p1".into(),
                "w1".into(),
                None,
                "b".into(),
                1_000,
            )
            .unwrap();
        assert_eq!(message.id, 2);
    }

    #[test]
    fn prune_closed_drops_dead_recipients() {
        let mut bus = AgentBus::default();
        send(&mut bus, 1, "a");
        send(&mut bus, 2, "b");

        let mut state = crate::app::AppState::test_new();
        state.workspaces = vec![crate::workspace::Workspace::test_new("ws")];
        let live_pane = state.workspaces[0].tabs[0].root_pane;
        // Re-key one mailbox onto the live pane so only the dead one is pruned.
        let orphan = bus.mailboxes.remove(&pane(1)).unwrap();
        bus.mailboxes.insert(live_pane, orphan);

        bus.prune_closed(&state);
        assert!(bus.mailboxes.contains_key(&live_pane));
        assert!(!bus.mailboxes.contains_key(&pane(2)));
    }
}
