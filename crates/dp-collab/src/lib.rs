//! Real-time collaboration protocol for Icon Crafter.
//!
//! The protocol is transport-independent: the desktop client and the Axum
//! server both link this crate so there is exactly one definition of every
//! wire message and of the convergence rules applied to them.

/// Version of the collaboration protocol crate.
pub const COLLAB_VERSION: &str = "0.1.0";
