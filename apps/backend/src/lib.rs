//! Icon Crafter collaboration and publishing server.
//!
//! Entry point lives in `main.rs`; all testable logic is in library modules so
//! integration tests can build the router without binding a socket.

/// Version of the server crate.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
