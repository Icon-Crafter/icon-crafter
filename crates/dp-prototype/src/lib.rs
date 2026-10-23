//! Interactive prototyping engine for Icon Crafter: a validated interaction
//! graph plus a deterministic runtime that executes it in the editor preview and
//! in shared prototype links.
//!
//! * [`expr`] is the tiny arithmetic/logic expression language used by
//!   conditions and variable bindings;
//! * [`runtime`] owns presentation state (current frame, overlay stack,
//!   navigation history, runtime variables) and executes interactions;
//! * [`PrototypeRuntime::validate_flow`] checks that every navigation/overlay
//!   target in a flow exists and is a frame, so playback never hits a dangling
//!   reference.
//!
//! The model types themselves live in `dp-model` and are validated there; this
//! crate only executes definitions that already passed validation.

pub mod expr;
pub mod runtime;

pub use expr::{BinaryOp, Expr, ExprError, UnaryOp, Value};
pub use runtime::{
    ActiveTransition, DEFAULT_TRANSITION_MS, MAX_HISTORY, MAX_OVERLAYS, OverlayInstance,
    PrototypeRuntime, RuntimeError, RuntimeEvent,
};

/// Version of the prototype engine API surface.
pub const PROTOTYPE_VERSION: &str = "0.1.0";
