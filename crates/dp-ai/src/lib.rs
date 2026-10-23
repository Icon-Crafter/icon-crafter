//! Optional AI subsystem for Icon Crafter.
//!
//! The AI never edits the document directly: it proposes structured document
//! operations that are validated, previewed and applied as undoable
//! transactions by the same engine as manual editing.

/// Version of the AI subsystem API surface.
pub const AI_VERSION: &str = "0.1.0";
