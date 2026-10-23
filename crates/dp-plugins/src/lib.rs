//! Plugin host for Icon Crafter.
//!
//! Plugins never receive ambient authority. A plugin declares permissions in
//! its manifest and interacts with the document through a request/response API
//! whose every operation is validated and applied as an undoable transaction.

/// Version of the plugin host API surface.
pub const PLUGINS_VERSION: &str = "0.1.0";
