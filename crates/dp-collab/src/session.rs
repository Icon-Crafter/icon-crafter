//! Transport-independent collaboration session.
//!
//! The desktop client and the server both speak [`ClientMessage`] /
//! [`ServerMessage`] as JSON over the WebSocket. This module owns the client's
//! protocol state machine: outgoing batch queue, acknowledgements, gap
//! detection, resynchronisation from a server snapshot and presence tracking.
//! It is deliberately transport-free so it can be tested without a socket.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use dp_model::Document;

use crate::clock::ActorId;
use crate::op::{Envelope, OpError, Presence};
use crate::replica::Replica;

/// Maximum peers tracked in the presence list.
pub const MAX_PRESENCE: usize = 64;
/// WebSocket protocol version negotiated with the server.
pub const SYNC_PROTOCOL_VERSION: u32 = 1;

/// Client to server frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First frame after connecting.
    Hello {
        /// Client actor.
        actor: ActorId,
        /// Client protocol version.
        protocol: u32,
    },
    /// A batch of document operations.
    Ops {
        /// Ops batch.
        envelope: Envelope,
    },
    /// Requests a snapshot because the client detected a gap.
    SyncRequest {
        /// First server sequence the client is missing.
        from: u64,
    },
    /// Acknowledges the latest server sequence processed by the client.
    Ack {
        /// Server sequence.
        server_seq: u64,
    },
    /// Ephemeral presence heartbeat.
    Presence {
        /// Presence payload.
        presence: Presence,
    },
}

/// Server to client frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Accepted connection, with the current document snapshot.
    Welcome {
        /// Server sequence the snapshot corresponds to.
        server_seq: u64,
        /// Canonical document JSON (same format as `dp_persist` document.json).
        document_json: String,
    },
    /// Broadcast batch of accepted operations.
    Ops {
        /// Server-assigned sequence number.
        server_seq: u64,
        /// The batch.
        envelope: Envelope,
    },
    /// Acknowledges a client batch.
    Ack {
        /// Client batch id.
        batch_id: Uuid,
        /// Server sequence assigned to it.
        server_seq: u64,
    },
    /// Snapshot response for `SyncRequest`.
    Snapshot {
        /// Server sequence the snapshot corresponds to.
        server_seq: u64,
        /// Canonical document JSON.
        document_json: String,
    },
    /// A peer joined or updated presence.
    Presence {
        /// Presence payload.
        presence: Presence,
    },
    /// A peer left.
    PresenceLeft {
        /// Actor that left.
        actor: ActorId,
    },
    /// Rejected batch (permission/validation); the sender must drop it.
    Reject {
        /// Client batch id.
        batch_id: Uuid,
        /// Machine-readable reason.
        code: String,
        /// Human-readable detail.
        message: String,
    },
}

/// Events surfaced to the editor/UI.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// A local batch was created and should be sent.
    LocalBatch {
        /// Batch id.
        batch_id: Uuid,
        /// Sequence assigned by the client.
        seq: u64,
        /// Operation count.
        ops: usize,
    },
    /// A server batch was applied.
    RemoteBatch {
        /// Server sequence.
        server_seq: u64,
        /// Actor that authored the ops.
        actor: ActorId,
        /// Number of ops applied.
        ops: usize,
        /// Number of conflicts reported by per-field LWW.
        conflicts: usize,
    },
    /// A local batch was acknowledged.
    Acked {
        /// Batch id.
        batch_id: Uuid,
        /// Server sequence.
        server_seq: u64,
    },
    /// A local batch was rejected by the server.
    Rejected {
        /// Batch id.
        batch_id: Uuid,
        /// Machine-readable reason.
        code: String,
        /// Human-readable detail.
        message: String,
    },
    /// Server sequence gap detected; the client must resynchronise.
    ResyncRequired {
        /// Next expected server sequence.
        expected: u64,
        /// Received sequence.
        got: u64,
    },
    /// Session state was replaced by a server snapshot (unsent local ops lost).
    Resynced {
        /// Server sequence of the snapshot.
        server_seq: u64,
        /// Local batches that were discarded.
        lost_batches: usize,
    },
    /// Presence changed for an actor.
    PresenceChanged {
        /// Actor with updated presence.
        actor: ActorId,
    },
}

/// Session error.
#[derive(Debug, Error)]
pub enum SessionError {
    /// Envelope validation failed.
    #[error("protocol error: {0}")]
    Op(#[from] OpError),
    /// Snapshot document could not be parsed.
    #[error("invalid snapshot: {0}")]
    Snapshot(String),
}

/// Result of applying one server frame.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ReceiveReport {
    /// Whether the document changed.
    pub changed: bool,
    /// Events produced.
    pub events: Vec<SessionEvent>,
}

/// Client-side collaboration session.
#[derive(Debug)]
pub struct CollabSession {
    actor: ActorId,
    replica: Replica,
    next_seq: u64,
    inflight: Vec<Envelope>,
    /// Highest server sequence applied so far.
    server_seq: u64,
    /// Set once a gap was detected; further batches are ignored until resync.
    needs_resync: Option<u64>,
    presence: Vec<Presence>,
}

impl CollabSession {
    /// Creates a session for a local document.
    pub fn new(document: &Document, actor: ActorId) -> CollabSession {
        CollabSession {
            actor,
            replica: Replica::seed(document, actor),
            next_seq: 1,
            inflight: Vec::new(),
            server_seq: 0,
            needs_resync: None,
            presence: Vec::new(),
        }
    }

    /// Local actor.
    pub fn actor(&self) -> ActorId {
        self.actor
    }

    /// Converged document.
    pub fn document(&self) -> Result<Document, OpError> {
        self.replica.to_document()
    }

    /// The underlying replica (mutate document through it).
    pub fn replica(&mut self) -> &mut Replica {
        &mut self.replica
    }

    /// The underlying replica (read).
    pub fn replica_ref(&self) -> &Replica {
        &self.replica
    }

    /// Unacknowledged batches.
    pub fn inflight(&self) -> &[Envelope] {
        &self.inflight
    }

    /// Highest applied server sequence.
    pub fn server_seq(&self) -> u64 {
        self.server_seq
    }

    /// Missing server sequence when resynchronisation is required.
    pub fn resync_from(&self) -> Option<u64> {
        self.needs_resync
    }

    /// Current presence list (sorted by actor).
    pub fn presence(&self) -> &[Presence] {
        &self.presence
    }

    /// Builds the handshake frame.
    pub fn hello(&self) -> ClientMessage {
        ClientMessage::Hello {
            actor: self.actor,
            protocol: SYNC_PROTOCOL_VERSION,
        }
    }

    /// Flushes queued local ops into a batch ready to send.
    pub fn flush(&mut self) -> Result<Option<ClientMessage>, SessionError> {
        let Some(envelope) = self.replica.flush()? else {
            return Ok(None);
        };
        self.next_seq += 1;
        self.inflight.push(envelope.clone());
        Ok(Some(ClientMessage::Ops { envelope }))
    }

    /// Applies a server frame.
    pub fn receive(&mut self, message: ServerMessage) -> Result<ReceiveReport, SessionError> {
        let mut report = ReceiveReport::default();
        match message {
            ServerMessage::Welcome {
                server_seq,
                document_json,
            } => {
                self.resync_with(server_seq, &document_json, &mut report)?;
                report.changed = true;
            }
            ServerMessage::Snapshot {
                server_seq,
                document_json,
            } => {
                self.resync_with(server_seq, &document_json, &mut report)?;
                report.changed = true;
            }
            ServerMessage::Ops {
                server_seq,
                envelope,
            } => {
                if let Some(expected) = self.needs_resync {
                    report.events.push(SessionEvent::ResyncRequired {
                        expected,
                        got: server_seq,
                    });
                    return Ok(report);
                }
                if server_seq == self.server_seq {
                    // Duplicate delivery; ignore but acknowledge below.
                    return Ok(report);
                }
                if server_seq != self.server_seq + 1 {
                    let expected = self.server_seq + 1;
                    self.needs_resync = Some(expected);
                    report.events.push(SessionEvent::ResyncRequired {
                        expected,
                        got: server_seq,
                    });
                    return Ok(report);
                }
                let reports = self.replica.apply_envelope(&envelope)?;
                self.server_seq = server_seq;
                let actor = envelope.actor;
                let ops = envelope.ops.len();
                let conflicts: usize = reports.iter().map(|report| report.conflicts.len()).sum();
                let changed = reports.iter().any(|report| report.changed);
                report.events.push(SessionEvent::RemoteBatch {
                    server_seq,
                    actor,
                    ops,
                    conflicts,
                });
                report.changed = changed;
            }
            ServerMessage::Ack {
                batch_id,
                server_seq,
            } => {
                let before = self.inflight.len();
                self.inflight.retain(|envelope| envelope.batch_id != batch_id);
                if self.inflight.len() != before {
                    self.server_seq = self.server_seq.max(server_seq);
                    self.needs_resync = None;
                    report.events.push(SessionEvent::Acked {
                        batch_id,
                        server_seq,
                    });
                }
            }
            ServerMessage::Reject {
                batch_id,
                code,
                message,
            } => {
                self.inflight.retain(|envelope| envelope.batch_id != batch_id);
                report.events.push(SessionEvent::Rejected {
                    batch_id,
                    code,
                    message,
                });
            }
            ServerMessage::Presence { presence } => {
                let actor = presence.actor;
                self.upsert_presence(presence);
                report.events.push(SessionEvent::PresenceChanged { actor });
            }
            ServerMessage::PresenceLeft { actor } => {
                self.presence.retain(|peer| peer.actor != actor);
                report.events.push(SessionEvent::PresenceChanged { actor });
            }
        }
        Ok(report)
    }

    /// Builds an acknowledgement frame for the last applied server sequence.
    pub fn ack(&self) -> Option<ClientMessage> {
        (self.server_seq > 0).then_some(ClientMessage::Ack {
            server_seq: self.server_seq,
        })
    }

    /// Builds a sync request for the missing sequence.
    pub fn sync_request(&self) -> Option<ClientMessage> {
        self.needs_resync.map(|from| ClientMessage::SyncRequest { from })
    }

    /// Publishes presence.
    pub fn presence_message(&mut self, mut presence: Presence) -> ClientMessage {
        presence.last_seen_ms = presence.last_seen_ms.max(1);
        self.upsert_presence(presence.clone());
        ClientMessage::Presence { presence }
    }

    fn resync_with(
        &mut self,
        server_seq: u64,
        document_json: &str,
        report: &mut ReceiveReport,
    ) -> Result<(), SessionError> {
        let document = Document::from_json_str(document_json)
            .map_err(|error| SessionError::Snapshot(error.to_string()))?;
        let lost_batches = self.inflight.len();
        let actor = self.actor;
        self.replica = Replica::seed(&document, actor);
        self.inflight.clear();
        self.next_seq = 1;
        self.server_seq = server_seq;
        self.needs_resync = None;
        report.events.push(SessionEvent::Resynced {
            server_seq,
            lost_batches,
        });
        Ok(())
    }

    fn upsert_presence(&mut self, presence: Presence) {
        if let Some(existing) = self
            .presence
            .iter_mut()
            .find(|peer| peer.actor == presence.actor)
        {
            *existing = presence;
        } else if self.presence.len() < MAX_PRESENCE {
            self.presence.push(presence);
        }
        self.presence.sort_by_key(|peer| peer.actor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ActorId;
    use dp_model::{Node, NodeId};
    use serde_json::json;

    fn document_with_rect() -> (Document, NodeId) {
        let mut document = Document::new("Synced");
        let page_id = document.active_page_id();
        let node = Node::rectangle("Card", 10.0, 10.0, 100.0, 60.0);
        let node_id = node.id;
        document
            .page_mut(page_id)
            .expect("page")
            .insert_node(node, None, None)
            .expect("insert");
        (document, node_id)
    }

    fn relay(actor: ActorId, seq: u64, envelope: Envelope) -> ServerMessage {
        ServerMessage::Ops {
            server_seq: seq,
            envelope: Envelope {
                actor,
                seq,
                ..envelope
            },
        }
    }

    #[test]
    fn handshake_and_local_batch_flow() {
        let (document, node_id) = document_with_rect();
        let page_id = document.active_page_id();
        let actor = ActorId::new();
        let mut session = CollabSession::new(&document, actor);
        assert_eq!(
            session.hello(),
            ClientMessage::Hello {
                actor,
                protocol: SYNC_PROTOCOL_VERSION
            }
        );
        session
            .replica()
            .local_set(page_id, node_id, crate::op::NodeField::Opacity, json!(0.5))
            .expect("local op");
        let message = session.flush().expect("flush").expect("batch");
        let ClientMessage::Ops { envelope } = message else {
            panic!("expected ops");
        };
        assert_eq!(envelope.actor, actor);
        assert_eq!(session.inflight().len(), 1);

        let report = session
            .receive(ServerMessage::Ack {
                batch_id: envelope.batch_id,
                server_seq: 1,
            })
            .expect("ack");
        assert_eq!(session.inflight().len(), 0);
        assert_eq!(session.server_seq(), 1);
        assert!(matches!(
            report.events.as_slice(),
            [SessionEvent::Acked { .. }]
        ));
    }

    #[test]
    fn remote_batches_require_consecutive_sequences() {
        let (document, node_id) = document_with_rect();
        let page_id = document.active_page_id();
        let local = ActorId::new();
        let remote = ActorId::new();
        let mut session = CollabSession::new(&document, local);

        // Build a remote envelope by replicating the same document remotely.
        let mut remote_replica = Replica::seed(&document, remote);
        remote_replica
            .local_set(page_id, node_id, crate::op::NodeField::Opacity, json!(0.25))
            .expect("remote op");
        let Some(remote_batch) = remote_replica.flush().expect("flush") else {
            panic!("expected a remote batch");
        };

        // A gap is detected first.
        let report = session
            .receive(relay(remote, 2, remote_batch.clone()))
            .expect("gap");
        assert_eq!(session.resync_from(), Some(1));
        assert!(report
            .events
            .iter()
            .any(|event| matches!(event, SessionEvent::ResyncRequired { expected: 1, got: 2 })));
        assert_eq!(
            session.sync_request(),
            Some(ClientMessage::SyncRequest { from: 1 })
        );

        // The snapshot recovery resets the sequence.
        let json = document.to_json_string().expect("json");
        let report = session
            .receive(ServerMessage::Snapshot {
                server_seq: 2,
                document_json: json,
            })
            .expect("snapshot");
        assert!(matches!(
            report.events.as_slice(),
            [SessionEvent::Resynced { server_seq: 2, .. }]
        ));
        assert_eq!(session.server_seq(), 2);

        // The next consecutive batch applies and converges the document.
        let report = session
            .receive(relay(remote, 3, remote_batch))
            .expect("apply");
        assert!(report.changed);
        let converged = session.document().expect("document");
        let page = converged.active_page();
        assert_eq!(page.get(node_id).expect("node").opacity, 0.25);
        assert!(!session.inflight().iter().any(|batch| batch.actor == remote));
    }

    #[test]
    fn duplicate_delivery_is_ignored_and_presence_is_bounded() {
        let (document, _) = document_with_rect();
        let actor = ActorId::new();
        let mut session = CollabSession::new(&document, actor);
        let peer = ActorId::new();
        let presence = Presence {
            actor: peer,
            name: "Ada".to_owned(),
            color: "#4fd1c5".to_owned(),
            page: document.active_page_id(),
            selection: Vec::new(),
            cursor: None,
            last_seen_ms: 100,
        };
        let message = session.presence_message(presence);
        assert!(matches!(message, ClientMessage::Presence { .. }));
        assert_eq!(session.presence().len(), 1);

        // Duplicate server sequence numbers do not change state.
        let envelope = Envelope {
            protocol: 1,
            batch_id: Uuid::new_v4(),
            actor: peer,
            seq: 1,
            ops: Vec::new(),
        };
        session.server_seq = 5;
        let report = session
            .receive(ServerMessage::Ops {
                server_seq: 5,
                envelope,
            })
            .expect("duplicate");
        assert!(report.events.is_empty());

        // Presence list is capped.
        for index in 0..(MAX_PRESENCE + 10) {
            let mut extra = Presence {
                actor: ActorId::new(),
                name: format!("Peer {index}"),
                color: "#ffffff".to_owned(),
                page: document.active_page_id(),
                selection: Vec::new(),
                cursor: None,
                last_seen_ms: (index as u64) + 1,
            };
            extra.cursor = Some(dp_geom::Point::new(index as f64, 0.0));
            session.presence_message(extra);
        }
        assert!(session.presence().len() <= MAX_PRESENCE);
    }

    #[test]
    fn server_messages_are_json_tagged() {
        let welcome = ServerMessage::Welcome {
            server_seq: 7,
            document_json: "{}".to_owned(),
        };
        let json = serde_json::to_value(&welcome).expect("serialize");
        assert_eq!(json["type"], "welcome");
        let back: ServerMessage = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, welcome);

        let ops = ClientMessage::Ops {
            envelope: Envelope {
                protocol: 1,
                batch_id: Uuid::new_v4(),
                actor: ActorId::new(),
                seq: 3,
                ops: Vec::new(),
            },
        };
        let json = serde_json::to_value(&ops).expect("serialize");
        assert_eq!(json["type"], "ops");
        assert_eq!(json["envelope"]["protocol"], 1);
    }
}
