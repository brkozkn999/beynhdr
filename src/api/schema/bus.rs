use serde::{Deserialize, Serialize};

/// `bus.send` parameters: queue a message for an agent or pane mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BusSendParams {
    /// Recipient mailbox owner: an agent name or a public pane id.
    pub to: String,
    /// Message body. UTF-8, non-empty, at most 8 KiB.
    pub text: String,
    /// Optional free-form sender label (agent name, pane id, or plugin id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// `bus.list` parameters: peek at pending messages without consuming them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BusListParams {
    /// Mailbox owner: an agent name or a public pane id.
    pub to: String,
}

/// `bus.read` parameters: drain pending messages from a mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BusReadParams {
    /// Mailbox owner: an agent name or a public pane id.
    pub to: String,
    /// Maximum number of pending messages to drain. Absent drains all pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
}

/// `bus.ack` parameters: remove a single pending message by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BusAckParams {
    /// Mailbox owner: an agent name or a public pane id.
    pub to: String,
    /// Message id as returned by `bus.send`, `bus.list`, or `bus.message`.
    pub message_id: u64,
}

/// A queued bus message. The same record is returned by `bus.send`,
/// `bus.list`, `bus.read`, and carried in `bus.message` events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BusMessage {
    /// Per-mailbox monotonically increasing id.
    pub id: u64,
    /// Public pane id that owned the mailbox when the message was accepted.
    pub to: String,
    /// Workspace that contained the recipient pane at send time.
    pub workspace_id: String,
    /// Sender label supplied by `bus.send`, when provided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Message body (UTF-8).
    pub text: String,
    /// Unix timestamp in milliseconds when the message was queued.
    pub queued_unix_ms: u64,
}
