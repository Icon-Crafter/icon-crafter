//! Real-time collaboration protocol for Icon Crafter.
//!
//! The protocol is transport-independent: the desktop client and the Axum
//! server both link this crate so there is exactly one definition of every
//! wire message and of the convergence rules applied to them.
//!
//! Layers:
//!
//! * [`clock`] — actor identity and hybrid logical clock stamps;
//! * [`rga`] — replicated growable array used for sibling ordering;
//! * [`op`] — wire operations, envelopes and presence payloads;
//! * [`replica`] — convergent replica state and document materialization;
//! * [`session`] — transport-independent client protocol state machine
//!   (handshake, batch queue, acknowledgements, gap detection, resync,
//!   presence) shared by the desktop client and the server tests.

pub mod clock;
pub mod op;
pub mod replica;
pub mod rga;
pub mod session;

pub use clock::{ActorId, Hlc, HlcClock};
pub use op::{Conflict, Envelope, NodeField, Op, OpError, PROTOCOL_VERSION, Presence, PropsMap};
pub use replica::{ApplyReport, Replica};
pub use rga::{RgaError, RgaList};
pub use session::{
    ClientMessage, CollabSession, ReceiveReport, SYNC_PROTOCOL_VERSION, ServerMessage,
    SessionError, SessionEvent,
};

/// Version of the collaboration protocol crate.
pub const COLLAB_VERSION: &str = "0.1.0";
