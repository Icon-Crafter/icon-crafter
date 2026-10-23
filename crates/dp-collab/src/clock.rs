//! Hybrid logical clocks and actor identity.
//!
//! Every collaborative operation carries a [`Hlc`] stamp. Stamps give a total
//! order across clients (Lamport counter, ties broken by actor id) used for
//! last-writer-wins register updates, and insertion ids that make concurrent
//! tree inserts converge deterministically.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identity of one collaborating client session.
///
/// A new actor id is minted for every session (tab/device); it is not the user
/// id. Presence and conflict reporting map actors to users.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActorId(Uuid);

impl ActorId {
    /// Creates a fresh random actor id.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wraps an existing uuid (e.g. resumed from a session store).
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Wraps raw bytes (used for deterministic seed actors in tests).
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// The underlying uuid.
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for ActorId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Hybrid logical clock stamp: logical counter plus actor tie-breaker.
///
/// Ordering is `(lamport, actor)` which is total and identical on every client,
/// which is exactly what last-writer-wins needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hlc {
    /// Monotonic logical counter.
    pub lamport: u64,
    /// Actor that produced the stamp.
    pub actor: ActorId,
}

impl Hlc {
    /// Creates a stamp.
    pub fn new(lamport: u64, actor: ActorId) -> Self {
        Self { lamport, actor }
    }

    /// The zero stamp for an actor (used for seeding local state).
    pub fn zero(actor: ActorId) -> Self {
        Self {
            lamport: 0,
            actor,
        }
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.lamport, self.actor)
    }
}

/// Monotonic clock for one actor.
#[derive(Clone, Debug)]
pub struct HlcClock {
    actor: ActorId,
    last: u64,
}

impl HlcClock {
    /// Starts a clock at zero for `actor`.
    pub fn new(actor: ActorId) -> Self {
        Self { actor, last: 0 }
    }

    /// The actor this clock stamps for.
    pub fn actor(&self) -> ActorId {
        self.actor
    }

    /// Last emitted logical time.
    pub fn last(&self) -> u64 {
        self.last
    }

    /// Produces the next stamp for a local operation.
    pub fn tick(&mut self) -> Hlc {
        self.last += 1;
        Hlc::new(self.last, self.actor)
    }

    /// Observes a remote stamp and keeps the clock ahead of it.
    ///
    /// The returned stamp is the logical time after the merge; it is used as the
    /// base for the next local tick so causality is preserved.
    pub fn observe(&mut self, remote: Hlc) -> u64 {
        self.last = self.last.max(remote.lamport) + 1;
        self.last
    }

    /// Merges a remote logical time without consuming a stamp.
    pub fn merge(&mut self, remote_lamport: u64) {
        self.last = self.last.max(remote_lamport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_order_by_lamport_then_actor() {
        let a = ActorId::from_uuid(Uuid::from_u128(1));
        let b = ActorId::from_uuid(Uuid::from_u128(2));
        assert!(Hlc::new(1, a) < Hlc::new(2, a));
        assert!(Hlc::new(2, a) < Hlc::new(2, b));
        assert_eq!(Hlc::new(5, a), Hlc::new(5, a));
    }

    #[test]
    fn ticks_are_monotonic() {
        let mut clock = HlcClock::new(ActorId::new());
        let first = clock.tick();
        let second = clock.tick();
        assert!(second > first);
        assert_eq!(first.lamport + 1, second.lamport);
    }

    #[test]
    fn observing_remote_time_moves_clock_ahead() {
        let a = ActorId::from_uuid(Uuid::from_u128(1));
        let b = ActorId::from_uuid(Uuid::from_u128(2));
        let mut clock = HlcClock::new(a);
        clock.tick();
        clock.observe(Hlc::new(42, b));
        let next = clock.tick();
        assert_eq!(next.lamport, 44);
        assert_eq!(next.actor, a);
    }

    #[test]
    fn actor_id_roundtrips_json() {
        let actor = ActorId::new();
        let json = serde_json::to_string(&actor).unwrap();
        let back: ActorId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, actor);
    }
}
