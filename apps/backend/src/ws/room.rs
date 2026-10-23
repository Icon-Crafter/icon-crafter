//! Per-file WebSocket room registry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::ws::protocol::PeerInfo;

/// Broadcast channel capacity; slow clients further behind are dropped.
const BROADCAST_CAPACITY: usize = 256;
/// Hard cap on the number of simultaneously active rooms.
pub const MAX_ROOMS: usize = 4096;

/// One connected participant.
#[derive(Debug, Clone)]
pub struct Peer {
    /// Server-assigned client id.
    pub client_id: Uuid,
    /// User id.
    pub actor_id: Uuid,
    /// Display name.
    pub display_name: String,
    /// Join timestamp.
    pub joined_at: DateTime<Utc>,
}

impl Peer {
    /// Convert to the wire representation.
    pub fn to_info(&self) -> PeerInfo {
        PeerInfo {
            client_id: self.client_id,
            actor_id: self.actor_id,
            display_name: self.display_name.clone(),
            joined_at: self.joined_at.to_rfc3339(),
        }
    }
}

/// A broadcast room for one file.
#[derive(Debug)]
pub struct Room {
    file_id: Uuid,
    sender: broadcast::Sender<Arc<str>>,
    server_seq: AtomicI64,
    peers: RwLock<HashMap<Uuid, Peer>>,
}

impl Room {
    /// Create an empty room.
    pub fn new(file_id: Uuid) -> Self {
        let (sender, _receiver) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            file_id,
            sender,
            server_seq: AtomicI64::new(0),
            peers: RwLock::new(HashMap::new()),
        }
    }

    /// File this room belongs to.
    pub fn file_id(&self) -> Uuid {
        self.file_id
    }

    /// Subscribe to broadcasts for this room.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<str>> {
        self.sender.subscribe()
    }

    /// Last observed server sequence.
    pub fn current_seq(&self) -> i64 {
        self.server_seq.load(Ordering::Acquire)
    }

    /// Record an observed server sequence; never moves backwards.
    pub fn observe_seq(&self, seq: i64) {
        if seq > 0 {
            self.server_seq.fetch_max(seq, Ordering::AcqRel);
        }
    }

    /// Broadcast an encoded server message to every subscriber.
    ///
    /// Returns the number of receivers that received it. `0` is normal when no
    /// other peer is connected; the room does not keep history here.
    pub fn broadcast(&self, message: &str) -> usize {
        self.sender.send(Arc::<str>::from(message)).unwrap_or(0)
    }

    /// Register a peer. Returns `true` when it was not present before.
    pub fn join(&self, peer: Peer) -> bool {
        let mut peers = self
            .peers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        peers.insert(peer.client_id, peer).is_none()
    }

    /// Remove a peer.
    pub fn leave(&self, client_id: Uuid) -> Option<Peer> {
        let mut peers = self
            .peers
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        peers.remove(&client_id)
    }

    /// Snapshot of current peers, ordered by join time then id.
    pub fn peers(&self) -> Vec<Peer> {
        let peers = self
            .peers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut peers: Vec<Peer> = peers.values().cloned().collect();
        peers.sort_by(|a, b| {
            a.joined_at
                .cmp(&b.joined_at)
                .then_with(|| a.client_id.cmp(&b.client_id))
        });
        peers
    }

    /// Number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// Whether the room has no peers.
    pub fn is_empty(&self) -> bool {
        self.peer_count() == 0
    }
}

/// Registry of live rooms keyed by file id.
#[derive(Debug, Default)]
pub struct RoomRegistry {
    rooms: RwLock<HashMap<Uuid, Arc<Room>>>,
}

impl RoomRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get an existing room.
    pub fn get(&self, file_id: Uuid) -> Option<Arc<Room>> {
        self.rooms
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&file_id)
            .cloned()
    }

    /// Get or create the room for a file.
    pub fn get_or_create(&self, file_id: Uuid) -> Arc<Room> {
        let mut rooms = self
            .rooms
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(room) = rooms.get(&file_id) {
            return Arc::clone(room);
        }
        // Bound memory growth if a lot of files are opened concurrently.
        if rooms.len() >= MAX_ROOMS {
            rooms.retain(|_, room| !room.is_empty());
        }
        let room = Arc::new(Room::new(file_id));
        rooms.insert(file_id, Arc::clone(&room));
        room
    }

    /// Drop a room once it has no peers. Returns `true` when removed.
    pub fn remove_if_empty(&self, file_id: Uuid) -> bool {
        let mut rooms = self
            .rooms
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let empty = rooms
            .get(&file_id)
            .map(|room| room.is_empty())
            .unwrap_or(false);
        if empty {
            rooms.remove(&file_id);
        }
        empty
    }

    /// Remove every empty room.
    pub fn prune_empty(&self) -> usize {
        let mut rooms = self
            .rooms
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = rooms.len();
        rooms.retain(|_, room| !room.is_empty());
        before - rooms.len()
    }

    /// Number of tracked rooms.
    pub fn room_count(&self) -> usize {
        self.rooms
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(actor_id: Uuid, name: &str) -> Peer {
        Peer {
            client_id: Uuid::new_v4(),
            actor_id,
            display_name: name.to_owned(),
            joined_at: Utc::now(),
        }
    }

    #[test]
    fn broadcast_reaches_all_subscribers() {
        let room = Room::new(Uuid::new_v4());
        let mut first = room.subscribe();
        let mut second = room.subscribe();
        let delivered = room.broadcast("{\"type\":\"ping\"}");
        assert_eq!(delivered, 2);
        assert_eq!(
            first.try_recv().expect("first").as_ref(),
            "{\"type\":\"ping\"}"
        );
        assert_eq!(
            second.try_recv().expect("second").as_ref(),
            "{\"type\":\"ping\"}"
        );
    }

    #[test]
    fn broadcast_with_no_subscribers_is_not_an_error() {
        let room = Room::new(Uuid::new_v4());
        assert_eq!(room.broadcast("{}"), 0);
    }

    #[test]
    fn join_leave_and_peer_snapshot() {
        let room = Room::new(Uuid::new_v4());
        let first = peer(Uuid::new_v4(), "Ada");
        let second = peer(Uuid::new_v4(), "Grace");
        assert!(room.join(first.clone()));
        assert!(!room.join(first.clone()));
        assert!(room.join(second));
        assert_eq!(room.peer_count(), 2);
        assert_eq!(room.peers().len(), 2);
        assert_eq!(
            room.leave(first.client_id).map(|p| p.display_name),
            Some("Ada".to_owned())
        );
        assert!(room.leave(first.client_id).is_none());
        assert_eq!(room.peer_count(), 1);
    }

    #[test]
    fn sequence_never_moves_backwards() {
        let room = Room::new(Uuid::new_v4());
        room.observe_seq(5);
        room.observe_seq(3);
        assert_eq!(room.current_seq(), 5);
        room.observe_seq(9);
        assert_eq!(room.current_seq(), 9);
        room.observe_seq(-4);
        assert_eq!(room.current_seq(), 9);
    }

    #[test]
    fn registry_reuses_and_prunes_rooms() {
        let registry = RoomRegistry::new();
        let file_id = Uuid::new_v4();
        let room = registry.get_or_create(file_id);
        assert!(Arc::ptr_eq(&room, &registry.get_or_create(file_id)));
        assert_eq!(registry.room_count(), 1);
        // A room with peers is not pruned.
        room.join(peer(Uuid::new_v4(), "Ada"));
        assert_eq!(registry.prune_empty(), 0);
        assert!(!registry.remove_if_empty(file_id));
        room.leave(room.peers()[0].client_id);
        assert_eq!(registry.prune_empty(), 1);
        assert_eq!(registry.room_count(), 0);
        assert!(registry.get(file_id).is_none());
    }

    #[test]
    fn room_removal_only_removes_empty_rooms() {
        let registry = RoomRegistry::new();
        let file_id = Uuid::new_v4();
        let room = registry.get_or_create(file_id);
        room.join(peer(Uuid::new_v4(), "Ada"));
        assert!(!registry.remove_if_empty(file_id));
        assert!(registry.get(file_id).is_some());
    }
}
