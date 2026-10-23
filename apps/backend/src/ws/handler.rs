//! WebSocket collaboration relay.
//!
//! Each connection joins a per-file [`Room`]. Accepted op batches are
//! journaled through [`OpLog`] (which assigns the per-file monotonic server
//! sequence) and then rebroadcast to the room. The relay deliberately keeps
//! document semantics out of the transport: dp-collab can be wired in behind
//! the same messages later.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::authz::{self, Capability, Role};
use crate::error::AppResult;
use crate::state::AppState;
use crate::ws::oplog::AppendOutcome;
use crate::ws::protocol::{
    ClientMessage, MAX_FRAME_BYTES, OpsBatch, PROTOCOL_VERSION, PeerInfo, Permissions,
    ServerMessage, validate_batch,
};
use crate::ws::room::{Peer, Room};

/// Interval between transport-level heartbeats.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// Outbound queue depth per connection; when the writer falls behind the
/// connection is dropped rather than letting memory grow.
const OUTBOUND_CAPACITY: usize = 128;

/// Upgrade an authenticated request to a WebSocket connection.
pub async fn ws_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Path(file_id): Path<Uuid>,
    _headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> AppResult<Response> {
    let access = authz::require_file(&state.db, user.user_id, file_id, Capability::View).await?;
    let role = access.role;
    Ok(ws
        .max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            run_connection(state, user, file_id, role, socket).await;
        }))
}

async fn run_connection(
    state: AppState,
    user: AuthUser,
    file_id: Uuid,
    role: Role,
    socket: WebSocket,
) {
    let client_id = Uuid::new_v4();
    let room = state.rooms.get_or_create(file_id);
    let mut room_rx = room.subscribe();
    let peer = Peer {
        client_id,
        actor_id: user.user_id,
        display_name: user.display_name.clone(),
        joined_at: chrono::Utc::now(),
    };
    if !room.join(peer) {
        tracing::warn!(%client_id, %file_id, "duplicate client id joined a room");
    }
    let permissions = permissions_for(role);
    let mut server_seq = match state.op_log.max_seq(file_id).await {
        Ok(seq) => seq,
        Err(error) => {
            tracing::warn!(%file_id, error = %error, "failed to read the op journal head");
            room.current_seq()
        }
    };
    if server_seq < room.current_seq() {
        server_seq = room.current_seq();
    }
    room.observe_seq(server_seq);

    let peers: Vec<PeerInfo> = room
        .peers()
        .iter()
        .filter(|peer| peer.client_id != client_id)
        .map(Peer::to_info)
        .collect();

    let (mut sink, mut stream) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Message>(OUTBOUND_CAPACITY);
    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let welcome = ServerMessage::Welcome {
        protocol: PROTOCOL_VERSION,
        file_id,
        client_id,
        actor_id: user.user_id,
        server_seq,
        permissions: permissions.clone(),
        peers,
    };
    if send(&outbound_tx, welcome).await.is_err() {
        finish_connection(&state, &room, file_id, client_id);
        let _ = writer.await;
        return;
    }
    room.broadcast(
        &ServerMessage::PeerJoined {
            client_id,
            actor_id: user.user_id,
            display_name: user.display_name.clone(),
        }
        .to_json(),
    );

    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut close_reason: Option<&'static str> = None;

    loop {
        tokio::select! {
            biased;
            _ = heartbeat.tick() => {
                if outbound_tx.send(Message::Ping(Bytes::new())).await.is_err() {
                    close_reason = Some("client_not_reading");
                    break;
                }
            }
            incoming = stream.next() => {
                match incoming {
                    None => break,
                    Some(Err(error)) => {
                        tracing::debug!(%client_id, error = %error, "websocket receive failed");
                        break;
                    }
                    Some(Ok(message)) => {
                        match message {
                            Message::Text(text) => {
                                let keep_going = handle_text(
                                    &state,
                                    &room,
                                    &outbound_tx,
                                    file_id,
                                    client_id,
                                    user.user_id,
                                    &permissions,
                                    &text,
                                )
                                .await;
                                if !keep_going {
                                    break;
                                }
                            }
                            Message::Binary(_) => {
                                let _ = send(
                                    &outbound_tx,
                                    ServerMessage::error(
                                        "invalid_message",
                                        "binary frames are not supported; send JSON text frames",
                                    ),
                                )
                                .await;
                            }
                            Message::Ping(payload) => {
                                if outbound_tx.send(Message::Pong(payload)).await.is_err() {
                                    close_reason = Some("client_not_reading");
                                    break;
                                }
                            }
                            Message::Pong(_) => {}
                            Message::Close(_) => break,
                        }
                    }
                }
            }
            broadcast = room_rx.recv() => {
                match broadcast {
                    Ok(text) => {
                        if outbound_tx
                            .send(Message::Text(text.to_string().into()))
                            .await
                            .is_err()
                        {
                            close_reason = Some("client_not_reading");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(%client_id, %file_id, skipped, "dropping slow websocket client");
                        let _ = send(
                            &outbound_tx,
                            ServerMessage::error(
                                "slow_client",
                                format!("connection fell behind by {skipped} messages"),
                            ),
                        )
                        .await;
                        close_reason = Some("slow_client");
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    if let Some(reason) = close_reason {
        tracing::debug!(%client_id, %file_id, reason, "closing websocket");
    }
    finish_connection(&state, &room, file_id, client_id);
    drop(outbound_tx);
    let _ = writer.await;
}

fn finish_connection(state: &AppState, room: &Arc<Room>, file_id: Uuid, client_id: Uuid) {
    if let Some(peer) = room.leave(client_id) {
        room.broadcast(
            &ServerMessage::PeerLeft {
                client_id,
                actor_id: peer.actor_id,
            }
            .to_json(),
        );
    }
    state.rooms.remove_if_empty(file_id);
}

// The relay context (state, room, outbound queue, ids, permissions) is passed
// explicitly because this function is the single dispatch point for all client
// message variants; grouping it into a struct would only move the same fields.
#[allow(clippy::too_many_arguments)]
async fn handle_text(
    state: &AppState,
    room: &Arc<Room>,
    outbound: &mpsc::Sender<Message>,
    file_id: Uuid,
    client_id: Uuid,
    actor_id: Uuid,
    permissions: &Permissions,
    text: &str,
) -> bool {
    let message: ClientMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(error) => {
            let _ = send(
                outbound,
                ServerMessage::Reject {
                    batch_id: None,
                    reason: "malformed".to_owned(),
                    message: format!("invalid client message: {error}"),
                },
            )
            .await;
            return true;
        }
    };
    match message {
        ClientMessage::Ops { base_seq, batch } => {
            handle_ops(
                state,
                room,
                outbound,
                file_id,
                client_id,
                actor_id,
                permissions,
                base_seq,
                batch,
            )
            .await;
            true
        }
        ClientMessage::Ping { nonce } => {
            let _ = send(outbound, ServerMessage::Pong { nonce }).await;
            true
        }
        ClientMessage::Presence { presence } => {
            room.broadcast(
                &ServerMessage::Presence {
                    client_id,
                    actor_id,
                    presence,
                }
                .to_json(),
            );
            true
        }
        ClientMessage::Cursor { x, y, page_id } => {
            if !x.is_finite() || !y.is_finite() {
                let _ = send(
                    outbound,
                    ServerMessage::error("invalid_cursor", "cursor coordinates must be finite"),
                )
                .await;
                return true;
            }
            room.broadcast(
                &ServerMessage::Cursor {
                    client_id,
                    actor_id,
                    x,
                    y,
                    page_id,
                }
                .to_json(),
            );
            true
        }
        ClientMessage::Selection { node_ids } => {
            if node_ids.len() > 10_000 {
                let _ = send(
                    outbound,
                    ServerMessage::error("invalid_selection", "too many selected nodes"),
                )
                .await;
                return true;
            }
            room.broadcast(
                &ServerMessage::Selection {
                    client_id,
                    actor_id,
                    node_ids,
                }
                .to_json(),
            );
            true
        }
        ClientMessage::Leave => false,
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_ops(
    state: &AppState,
    room: &Arc<Room>,
    outbound: &mpsc::Sender<Message>,
    file_id: Uuid,
    client_id: Uuid,
    actor_id: Uuid,
    permissions: &Permissions,
    base_seq: Option<i64>,
    batch: OpsBatch,
) {
    let batch_id = batch.batch_id;
    if !permissions.can_edit {
        let _ = send(
            outbound,
            ServerMessage::Reject {
                batch_id: Some(batch_id),
                reason: "permission".to_owned(),
                message: format!(
                    "your role `{}` does not allow relaying document edits",
                    permissions.role
                ),
            },
        )
        .await;
        return;
    }
    if let Err(message) = validate_batch(&batch) {
        let _ = send(
            outbound,
            ServerMessage::Reject {
                batch_id: Some(batch_id),
                reason: "validation".to_owned(),
                message,
            },
        )
        .await;
        return;
    }
    let current_seq = room.current_seq();
    if let Some(base) = base_seq
        && (base < 0 || base > current_seq)
    {
        let _ = send(
            outbound,
            ServerMessage::Reject {
                batch_id: Some(batch_id),
                reason: "stale".to_owned(),
                message: format!("base_seq {base} is outside the accepted range 0..={current_seq}"),
            },
        )
        .await;
        return;
    }

    match state
        .op_log
        .append(file_id, actor_id, base_seq, &batch)
        .await
    {
        Ok(outcome) => {
            let outcome: AppendOutcome = outcome;
            let _ = send(
                outbound,
                ServerMessage::Ack {
                    batch_id,
                    server_seq: outcome.seq,
                    accepted: true,
                    duplicate: outcome.duplicate,
                },
            )
            .await;
            if !outcome.duplicate {
                room.observe_seq(outcome.seq);
                room.broadcast(
                    &ServerMessage::Rebroadcast {
                        server_seq: outcome.seq,
                        actor_id,
                        client_id,
                        batch,
                    }
                    .to_json(),
                );
            } else {
                room.observe_seq(outcome.seq);
            }
        }
        Err(error) => {
            tracing::error!(%file_id, %batch_id, error = %error, "failed to journal an op batch");
            let _ = send(
                outbound,
                ServerMessage::Reject {
                    batch_id: Some(batch_id),
                    reason: "internal".to_owned(),
                    message: "the batch could not be journaled; retry later".to_owned(),
                },
            )
            .await;
        }
    }
}

async fn send(outbound: &mpsc::Sender<Message>, message: ServerMessage) -> Result<(), ()> {
    outbound
        .send(Message::Text(message.to_json().into()))
        .await
        .map_err(|_| ())
}

fn permissions_for(role: Role) -> Permissions {
    Permissions {
        role: role.as_str().to_owned(),
        capabilities: role
            .management_capability()
            .capabilities_upto()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        can_edit: role.capability() >= Capability::Edit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_map_to_permissions() {
        let viewer = permissions_for(Role::Viewer);
        assert!(!viewer.can_edit);
        assert_eq!(viewer.capabilities, vec!["view"]);
        let editor = permissions_for(Role::Editor);
        assert!(editor.can_edit);
        assert_eq!(editor.capabilities, vec!["view", "comment", "edit"]);
        let owner = permissions_for(Role::Owner);
        assert!(owner.can_edit);
        assert!(owner.capabilities.contains(&"manage".to_owned()));
    }

    #[test]
    fn heartbeat_and_queue_bounds_are_sane() {
        assert!(HEARTBEAT_INTERVAL >= Duration::from_secs(5));
        assert!(HEARTBEAT_INTERVAL <= Duration::from_secs(120));
        assert!(OUTBOUND_CAPACITY >= 16);
    }
}
