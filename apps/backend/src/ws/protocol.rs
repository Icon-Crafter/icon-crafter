//! WebSocket wire protocol types.
//!
//! The framing follows `docs/COLLAB-PROTOCOL.md`: JSON text frames with a
//! `type` discriminator, monotonically increasing server sequence numbers and
//! op batches that are acknowledged or rejected as a whole.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Protocol version spoken by this server.
pub const PROTOCOL_VERSION: u32 = 1;
/// Hard limit for one incoming WebSocket frame.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
/// Hard limit for the number of operations in one batch.
pub const MAX_OPS_PER_BATCH: usize = 500;
/// Maximum length of a single serialized operation (defensive bound).
pub const MAX_OP_BYTES: usize = 64 * 1024;

/// One batch of document operations submitted by a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsBatch {
    /// Client-generated unique batch id (idempotency key).
    pub batch_id: Uuid,
    /// Ordered operations. Treated as opaque by the relay and journal.
    pub ops: Vec<Value>,
    /// Optional hybrid logical clock stamp supplied by the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<Value>,
}

/// A message sent by a WebSocket client.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Submit one op batch.
    Ops {
        /// Optional base server sequence the client built on.
        #[serde(default)]
        base_seq: Option<i64>,
        /// Batch payload.
        batch: OpsBatch,
    },
    /// Ephemeral presence payload (cursor metadata, selection mode, ...).
    Presence {
        /// Arbitrary JSON presence payload.
        presence: Value,
    },
    /// Cursor position on the canvas.
    Cursor {
        /// Canvas X.
        x: f64,
        /// Canvas Y.
        y: f64,
        /// Optional page the cursor is on.
        #[serde(default)]
        page_id: Option<String>,
    },
    /// Selected node ids.
    Selection {
        /// Selected node ids.
        node_ids: Vec<String>,
    },
    /// Application-level ping.
    Ping {
        /// Optional correlation value echoed back.
        #[serde(default)]
        nonce: Option<String>,
    },
    /// Leave the room (the connection is closed afterwards).
    Leave,
}

/// Peer snapshot included in the welcome message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Server-assigned client id.
    pub client_id: Uuid,
    /// User id.
    pub actor_id: Uuid,
    /// Display name.
    pub display_name: String,
    /// Join timestamp (RFC 3339).
    pub joined_at: String,
}

/// Permissions granted to the connecting client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    /// Effective file role.
    pub role: String,
    /// Capability names granted by the role.
    pub capabilities: Vec<String>,
    /// Whether the client may relay op batches.
    pub can_edit: bool,
}

/// A message sent by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Handshake response, always the first message.
    Welcome {
        /// Protocol version.
        protocol: u32,
        /// File id.
        file_id: Uuid,
        /// Server-assigned id for this connection.
        client_id: Uuid,
        /// Acting user id.
        actor_id: Uuid,
        /// Current maximum journal sequence for the file.
        server_seq: i64,
        /// Effective permissions.
        permissions: Permissions,
        /// Current peers (excluding this client).
        peers: Vec<PeerInfo>,
    },
    /// A batch was accepted and journaled.
    Ack {
        /// Batch id.
        batch_id: Uuid,
        /// Server sequence assigned to the batch.
        server_seq: i64,
        /// Always `true` for ack (rejects are separate messages).
        accepted: bool,
        /// Whether an earlier identical batch was replayed.
        duplicate: bool,
    },
    /// A batch was rejected as a whole.
    Reject {
        /// Batch id when one could be parsed.
        batch_id: Option<Uuid>,
        /// Machine-readable reason.
        reason: String,
        /// Human-readable detail.
        message: String,
    },
    /// A batch from another client, relayed in server order.
    Rebroadcast {
        /// Server sequence.
        server_seq: i64,
        /// Authoring user.
        actor_id: Uuid,
        /// Authoring connection.
        client_id: Uuid,
        /// The batch.
        batch: OpsBatch,
    },
    /// Ephemeral presence update.
    Presence {
        /// Originating client.
        client_id: Uuid,
        /// Originating user.
        actor_id: Uuid,
        /// Presence payload.
        presence: Value,
    },
    /// Peer cursor update.
    Cursor {
        /// Originating client.
        client_id: Uuid,
        /// Originating user.
        actor_id: Uuid,
        /// Canvas X.
        x: f64,
        /// Canvas Y.
        y: f64,
        /// Page id.
        page_id: Option<String>,
    },
    /// Peer selection update.
    Selection {
        /// Originating client.
        client_id: Uuid,
        /// Originating user.
        actor_id: Uuid,
        /// Selected node ids.
        node_ids: Vec<String>,
    },
    /// Pong reply.
    Pong {
        /// Correlation value from the ping.
        nonce: Option<String>,
    },
    /// A peer joined the room.
    PeerJoined {
        /// Server-assigned peer client id.
        client_id: Uuid,
        /// Peer user id.
        actor_id: Uuid,
        /// Peer display name.
        display_name: String,
    },
    /// A peer left the room.
    PeerLeft {
        /// Server-assigned peer client id.
        client_id: Uuid,
        /// Peer user id.
        actor_id: Uuid,
    },
    /// Non-fatal protocol error; the connection may continue or close.
    Error {
        /// Machine-readable error code.
        code: String,
        /// Human-readable message.
        message: String,
    },
}

impl ServerMessage {
    /// Serialize to JSON, falling back to a minimal error frame.
    ///
    /// Serialization of these plain structs cannot fail in practice, but the
    /// fallback keeps the relay panic-free.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            "{\"type\":\"error\",\"code\":\"serialization_failed\",\"message\":\"failed to encode server message\"}"
                .to_owned()
        })
    }

    /// Build an error frame.
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

/// Validate an op batch at the relay boundary.
pub fn validate_batch(batch: &OpsBatch) -> Result<(), String> {
    if batch.ops.is_empty() {
        return Err("a batch must contain at least one operation".to_owned());
    }
    if batch.ops.len() > MAX_OPS_PER_BATCH {
        return Err(format!(
            "a batch may contain at most {MAX_OPS_PER_BATCH} operations"
        ));
    }
    for (index, op) in batch.ops.iter().enumerate() {
        let encoded =
            serde_json::to_vec(op).map_err(|_| format!("operation {index} is not JSON"))?;
        if encoded.len() > MAX_OP_BYTES {
            return Err(format!(
                "operation {index} exceeds the {MAX_OP_BYTES} byte limit"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn batch(ops: Vec<Value>) -> OpsBatch {
        OpsBatch {
            batch_id: Uuid::new_v4(),
            ops,
            stamp: None,
        }
    }

    #[test]
    fn client_messages_parse() {
        let message: ClientMessage = serde_json::from_value(json!({
            "type": "ops",
            "base_seq": 12,
            "batch": {"batch_id": Uuid::nil(), "ops": [{"op": "insert"}]}
        }))
        .expect("parse ops");
        match message {
            ClientMessage::Ops { base_seq, batch } => {
                assert_eq!(base_seq, Some(12));
                assert_eq!(batch.batch_id, Uuid::nil());
                assert_eq!(batch.ops.len(), 1);
            }
            other => panic!("unexpected message: {other:?}"),
        }

        assert!(matches!(
            serde_json::from_value::<ClientMessage>(json!({"type": "ping"})).expect("parse ping"),
            ClientMessage::Ping { nonce: None }
        ));
        assert!(matches!(
            serde_json::from_value::<ClientMessage>(json!({"type": "leave"})).expect("parse leave"),
            ClientMessage::Leave
        ));
        assert!(serde_json::from_value::<ClientMessage>(json!({"type": "bogus"})).is_err());
    }

    #[test]
    fn server_messages_serialize_with_type_tag() {
        let json = ServerMessage::Pong {
            nonce: Some("n".to_owned()),
        }
        .to_json();
        assert!(json.contains("\"type\":\"pong\""));
        let json = ServerMessage::error("invalid_message", "bad").to_json();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"code\":\"invalid_message\""));
    }

    #[test]
    fn batch_validation_rejects_empty_and_oversized() {
        assert!(validate_batch(&batch(Vec::new())).is_err());
        let too_many = batch(vec![json!({"op": "noop"}); MAX_OPS_PER_BATCH + 1]);
        assert!(validate_batch(&too_many).is_err());
        let big_op = batch(vec![json!({"blob": "x".repeat(MAX_OP_BYTES)})]);
        assert!(validate_batch(&big_op).is_err());
        assert!(validate_batch(&batch(vec![json!({"op": "noop"})])).is_ok());
    }
}
