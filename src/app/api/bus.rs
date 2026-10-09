//! `bus.*` API handlers: direct agent/pane messaging over per-recipient
//! mailboxes. The store lives in `crate::app::bus`; this module only resolves
//! targets, enforces wire-level errors, and emits `bus.message` events.

use super::responses::{encode_error, encode_error_body, encode_success};
use crate::api::schema::{
    BusAckParams, BusListParams, BusReadParams, BusSendParams, ErrorBody, EventData, EventEnvelope,
    EventKind, ResponseResult,
};
use crate::app::bus::{
    BusEnqueueError, MAX_FROM_BYTES, MAX_MESSAGES_PER_MAILBOX, MAX_MESSAGE_BYTES,
};
use crate::app::terminal_targets::TerminalTargetError;
use crate::app::App;

impl App {
    pub(super) fn handle_bus_send(&mut self, id: String, params: BusSendParams) -> String {
        self.bus.prune_closed(&self.state);
        let resolved = match self.resolve_terminal_target(&params.to) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.bus_target_error_body(err)),
        };
        let Some(pane_id) = self.public_pane_id(resolved.ws_idx, resolved.pane_id) else {
            return encode_error(
                id,
                "bus_target_not_found",
                format!("bus target {} has no public pane id", params.to),
            );
        };
        let workspace_id = self.public_workspace_id(resolved.ws_idx);
        let from = params.from.map(|from| from.trim().to_string());
        let queued_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);

        match self.bus.send(
            resolved.pane_id,
            pane_id,
            workspace_id,
            from,
            params.text,
            queued_unix_ms,
        ) {
            Ok((message, pending)) => {
                self.emit_event(EventEnvelope {
                    event: EventKind::BusMessage,
                    data: EventData::BusMessage {
                        message: message.clone(),
                        pending,
                    },
                });
                encode_success(id, ResponseResult::BusSend { message, pending })
            }
            Err(err) => encode_error_body(id, bus_enqueue_error_body(err)),
        }
    }

    /// `bus.list`: peek at pending messages without consuming them.
    pub(super) fn handle_bus_list(&mut self, id: String, params: BusListParams) -> String {
        self.bus.prune_closed(&self.state);
        let resolved = match self.resolve_terminal_target(&params.to) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.bus_target_error_body(err)),
        };
        encode_success(
            id,
            ResponseResult::BusList {
                messages: self.bus.pending(resolved.pane_id),
            },
        )
    }

    /// `bus.read`: drain pending messages in FIFO order. The messages are
    /// removed from the mailbox once returned.
    pub(super) fn handle_bus_read(&mut self, id: String, params: BusReadParams) -> String {
        self.bus.prune_closed(&self.state);
        let resolved = match self.resolve_terminal_target(&params.to) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.bus_target_error_body(err)),
        };
        let messages = self
            .bus
            .read(resolved.pane_id, params.max.map(|max| max as usize));
        encode_success(id, ResponseResult::BusRead { messages })
    }

    /// `bus.ack`: remove a single pending message by id. Reports `removed:
    /// false` when the id is not pending (already read, already acked, or
    /// never queued); acking is idempotent by design.
    pub(super) fn handle_bus_ack(&mut self, id: String, params: BusAckParams) -> String {
        self.bus.prune_closed(&self.state);
        let resolved = match self.resolve_terminal_target(&params.to) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.bus_target_error_body(err)),
        };
        let removed = self.bus.ack(resolved.pane_id, params.message_id);
        encode_success(
            id,
            ResponseResult::BusAck {
                message_id: params.message_id,
                removed,
            },
        )
    }

    fn bus_target_error_body(&self, err: TerminalTargetError) -> ErrorBody {
        match err {
            TerminalTargetError::NotFound { target } => ErrorBody {
                code: "bus_target_not_found".into(),
                message: format!("bus target {target} not found"),
            },
            TerminalTargetError::Ambiguous { target, candidates } => ErrorBody {
                code: "bus_target_ambiguous".into(),
                message: format!(
                    "bus target {target} is ambiguous; candidates: {}",
                    candidates
                        .into_iter()
                        .map(|candidate| format!(
                            "terminal_id={} pane_id={} workspace_id={} tab_id={} cwd={} status={:?}",
                            candidate.terminal_id,
                            candidate.pane_id,
                            candidate.workspace_id,
                            candidate.tab_id,
                            candidate.cwd.unwrap_or_else(|| "unknown".into()),
                            candidate.agent_status,
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            },
        }
    }
}

fn bus_enqueue_error_body(err: BusEnqueueError) -> ErrorBody {
    match err {
        BusEnqueueError::EmptyMessage => ErrorBody {
            code: "empty_bus_message".into(),
            message: "bus message text must not be empty".into(),
        },
        BusEnqueueError::MessageTooLarge { bytes } => ErrorBody {
            code: "bus_message_too_large".into(),
            message: format!(
                "bus message text is {bytes} bytes; maximum is {MAX_MESSAGE_BYTES} bytes"
            ),
        },
        BusEnqueueError::EmptyFrom => ErrorBody {
            code: "invalid_bus_from".into(),
            message: "bus sender label must not be empty".into(),
        },
        BusEnqueueError::FromTooLong { bytes } => ErrorBody {
            code: "invalid_bus_from".into(),
            message: format!(
                "bus sender label is {bytes} bytes; maximum is {MAX_FROM_BYTES} bytes"
            ),
        },
        BusEnqueueError::QueueFull => ErrorBody {
            code: "bus_queue_full".into(),
            message: format!(
                "recipient mailbox is full ({MAX_MESSAGES_PER_MAILBOX} pending messages); \
                 drain it with bus.read or remove entries with bus.ack"
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::{ErrorResponse, Method, Request, Subscription, SuccessResponse};
    use crate::app::AppPolicy;
    use crate::config::Config;
    use crate::workspace::Workspace;

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            AppPolicy::TEST,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("bus")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app
    }

    /// Public pane id of the single pane in the test workspace.
    fn only_pane_id(app: &App) -> String {
        let root = app.state.workspaces[0].tabs[0].root_pane;
        app.public_pane_id(0, root).unwrap()
    }

    fn send_request(app: &mut App, id: &str, to: &str, text: &str, from: Option<&str>) -> String {
        app.handle_api_request(Request {
            id: id.into(),
            method: Method::BusSend(BusSendParams {
                to: to.into(),
                text: text.into(),
                from: from.map(str::to_string),
            }),
        })
    }

    fn read_request(app: &mut App, id: &str, to: &str, max: Option<u32>) -> String {
        app.handle_api_request(Request {
            id: id.into(),
            method: Method::BusRead(BusReadParams { to: to.into(), max }),
        })
    }

    fn list_request(app: &mut App, id: &str, to: &str) -> String {
        app.handle_api_request(Request {
            id: id.into(),
            method: Method::BusList(BusListParams { to: to.into() }),
        })
    }

    fn error_code(response: &str) -> String {
        serde_json::from_str::<ErrorResponse>(response)
            .unwrap()
            .error
            .code
    }

    fn sent_message(response: &str) -> crate::api::schema::BusMessage {
        let success: SuccessResponse = serde_json::from_str(response).unwrap();
        match success.result {
            ResponseResult::BusSend { message, .. } => message,
            other => panic!("expected bus_send result, got {other:?}"),
        }
    }

    fn read_messages(response: &str) -> Vec<crate::api::schema::BusMessage> {
        let success: SuccessResponse = serde_json::from_str(response).unwrap();
        match success.result {
            ResponseResult::BusRead { messages } | ResponseResult::BusList { messages } => messages,
            other => panic!("expected bus message list, got {other:?}"),
        }
    }

    #[test]
    fn bus_send_then_read_returns_messages_in_fifo_order() {
        let mut app = test_app();
        let pane = only_pane_id(&app);

        send_request(&mut app, "s1", &pane, "first", Some("pane-a"));
        send_request(&mut app, "s2", &pane, "second", None);

        let messages = read_messages(&read_request(&mut app, "r1", &pane, None));
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].text, "first");
        assert_eq!(messages[0].id, 1);
        assert_eq!(messages[0].from.as_deref(), Some("pane-a"));
        assert_eq!(messages[0].to, pane);
        assert_eq!(messages[1].text, "second");
        assert_eq!(messages[1].id, 2);
        assert!(messages[1].from.is_none());

        // Read drains the mailbox.
        assert!(read_messages(&read_request(&mut app, "r2", &pane, None)).is_empty());
    }

    #[test]
    fn bus_list_peeks_without_consuming() {
        let mut app = test_app();
        let pane = only_pane_id(&app);
        send_request(&mut app, "s1", &pane, "hello", None);

        let peeked = read_messages(&list_request(&mut app, "l1", &pane));
        assert_eq!(peeked.len(), 1);
        let drained = read_messages(&read_request(&mut app, "r1", &pane, None));
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].text, "hello");
    }

    #[test]
    fn bus_read_max_limits_drain() {
        let mut app = test_app();
        let pane = only_pane_id(&app);
        send_request(&mut app, "s1", &pane, "a", None);
        send_request(&mut app, "s2", &pane, "b", None);

        let first = read_messages(&read_request(&mut app, "r1", &pane, Some(1)));
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].text, "a");
        let rest = read_messages(&list_request(&mut app, "l1", &pane));
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].text, "b");
    }

    #[test]
    fn bus_send_to_unknown_target_is_bus_target_not_found() {
        let mut app = test_app();
        let response = send_request(&mut app, "s1", "no-such-agent", "hello", None);
        assert_eq!(error_code(&response), "bus_target_not_found");
        // list/read on an unknown target fail the same way.
        assert_eq!(
            error_code(&list_request(&mut app, "l1", "no-such-agent")),
            "bus_target_not_found"
        );
        assert_eq!(
            error_code(&read_request(&mut app, "r1", "no-such-agent", None)),
            "bus_target_not_found"
        );
    }

    #[test]
    fn bus_send_rejects_empty_and_oversized_messages() {
        let mut app = test_app();
        let pane = only_pane_id(&app);

        assert_eq!(
            error_code(&send_request(&mut app, "e1", &pane, "", None)),
            "empty_bus_message"
        );
        let oversized = "x".repeat(MAX_MESSAGE_BYTES + 1);
        assert_eq!(
            error_code(&send_request(&mut app, "e2", &pane, &oversized, None)),
            "bus_message_too_large"
        );
        assert_eq!(
            error_code(&send_request(&mut app, "e3", &pane, "ok", Some("   "))),
            "invalid_bus_from"
        );
    }

    #[test]
    fn bus_send_reports_queue_full_at_capacity() {
        let mut app = test_app();
        let pane = only_pane_id(&app);
        for index in 0..MAX_MESSAGES_PER_MAILBOX {
            let response = send_request(&mut app, "s", &pane, &format!("m{index}"), None);
            assert!(serde_json::from_str::<SuccessResponse>(&response).is_ok());
        }
        assert_eq!(
            error_code(&send_request(&mut app, "full", &pane, "overflow", None)),
            "bus_queue_full"
        );
    }

    #[test]
    fn bus_send_emits_bus_message_event() {
        let mut app = test_app();
        let pane = only_pane_id(&app);

        send_request(&mut app, "s1", &pane, "ping", Some("tester"));

        let events = app.event_hub.events_after(0);
        let bus_events: Vec<_> = events
            .iter()
            .filter(|(_, event)| event.event == EventKind::BusMessage)
            .collect();
        assert_eq!(bus_events.len(), 1);
        let EventData::BusMessage { message, pending } = &bus_events[0].1.data else {
            panic!("expected bus_message event data");
        };
        assert_eq!(message.to, pane);
        assert_eq!(message.text, "ping");
        assert_eq!(message.from.as_deref(), Some("tester"));
        assert_eq!(*pending, 1);
    }

    #[test]
    fn bus_ack_removes_pending_message_by_id() {
        let mut app = test_app();
        let pane = only_pane_id(&app);
        let first = sent_message(&send_request(&mut app, "s1", &pane, "a", None));
        send_request(&mut app, "s2", &pane, "b", None);

        let acked: SuccessResponse = serde_json::from_str(&app.handle_api_request(Request {
            id: "ack1".into(),
            method: Method::BusAck(BusAckParams {
                to: pane.clone(),
                message_id: first.id,
            }),
        }))
        .unwrap();
        match acked.result {
            ResponseResult::BusAck {
                message_id,
                removed,
            } => {
                assert_eq!(message_id, first.id);
                assert!(removed);
            }
            other => panic!("expected bus_ack result, got {other:?}"),
        }

        // Second ack is a no-op, and the remaining message survives.
        let rest = read_messages(&list_request(&mut app, "l1", &pane));
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].text, "b");
    }

    #[test]
    fn bus_addresses_agents_by_name() {
        let mut app = test_app();
        let pane = only_pane_id(&app);
        let root = app.state.workspaces[0].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[0].terminal_id(root).unwrap().clone();
        app.state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_agent_name("worker-1".into());

        let message = sent_message(&send_request(&mut app, "s1", "worker-1", "hi", None));
        assert_eq!(message.to, pane);
        assert_eq!(
            read_messages(&read_request(&mut app, "r1", "worker-1", None)).len(),
            1
        );
    }

    #[test]
    fn bus_mailbox_follows_pane_across_renumbering() {
        let mut app = test_app();
        let pane = only_pane_id(&app);
        send_request(&mut app, "s1", &pane, "queued", None);
        // Renumber the pane's public id; internal PaneId stays stable.
        app.state.workspaces[0]
            .public_pane_numbers
            .values_mut()
            .for_each(|number| *number = 7);
        let new_pane = only_pane_id(&app);
        assert_ne!(new_pane, pane);
        let messages = read_messages(&read_request(&mut app, "r1", &new_pane, None));
        assert_eq!(messages.len(), 1);
        // The stored `to` snapshot still records the send-time address.
        assert_eq!(messages[0].to, pane);
    }

    #[test]
    fn bus_methods_parse_from_wire_json() {
        let request: Request = serde_json::from_value(serde_json::json!({
            "id": "1",
            "method": "bus.send",
            "params": { "to": "w1:p1", "text": "hi", "from": "agent-a" }
        }))
        .unwrap();
        match request.method {
            Method::BusSend(params) => {
                assert_eq!(params.to, "w1:p1");
                assert_eq!(params.from.as_deref(), Some("agent-a"));
            }
            other => panic!("expected bus.send, got {other:?}"),
        }

        let request: Request = serde_json::from_value(serde_json::json!({
            "id": "2",
            "method": "bus.read",
            "params": { "to": "w1:p1", "max": 3 }
        }))
        .unwrap();
        match request.method {
            Method::BusRead(params) => assert_eq!(params.max, Some(3)),
            other => panic!("expected bus.read, got {other:?}"),
        }

        // The bus.message subscription type parses.
        let subscription: Subscription =
            serde_json::from_value(serde_json::json!({ "type": "bus.message" })).unwrap();
        assert!(matches!(subscription, Subscription::BusMessage {}));
    }
}
