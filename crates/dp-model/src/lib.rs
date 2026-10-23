//! Core design-document model for Icon Crafter.
//!
//! The document model is the single source of truth for the editor, the
//! renderer, the collaboration layer and every export path. It is deliberately
//! free of rendering, UI and I/O concerns.

/// Version of the document model API surface (not the serialized schema).
pub const MODEL_VERSION: &str = "0.1.0";
