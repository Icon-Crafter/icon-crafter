//! Plugin host for Icon Crafter: manifests, declared permissions, an op-scoped
//! sandbox, commands/panels/events, lifecycle and error isolation.
//!
//! ## Security model
//!
//! Plugins are **data**, not code. A manifest declares identity, API version,
//! permissions, commands, panels, event subscriptions and settings. The
//! executable half lives in the host: a [`PluginHandler`] may only issue typed
//! [`SandboxRequest`]s, each authorized against the plugin's granted and
//! declared permissions, validated against the live document, and turned into
//! *proposals* the editor applies as normal undoable transactions after
//! confirmation.
//!
//! This deliberately does **not** run arbitrary JavaScript/WASM, provide
//! filesystem/network access, or grant ambient document access. A future WASM
//! sandbox can reuse the same request/authorization surface.
//!
//! ## Example plugin
//!
//! See `crates/dp-plugins/examples/hello-plugin/` for a manifest-only example.

pub mod executor;
pub mod manifest;
pub mod permissions;
pub mod registry;
pub mod sandbox;

pub use executor::{
    ExecutionReport, HandlerCall, MAX_REQUESTS_PER_CALL, PluginErrorRecord, PluginHandler,
    dispatch_event, run_command,
};
pub use manifest::{
    MANIFEST_VERSION, MAX_COMMANDS, MAX_PANELS, MAX_SETTINGS, ManifestError, PluginCommand,
    PluginEvent, PluginManifest, PluginPanel, PluginSetting,
};
pub use permissions::{API_VERSION as PLUGIN_API_VERSION, Grants, Permission};
pub use registry::{MAX_PLUGINS, PluginRegistry, PluginState, RegisteredPlugin, RegistryError};
pub use sandbox::{
    MAX_NODES_PER_PROPOSAL, MAX_NODES_PER_REQUEST, MAX_STRING_BYTES, SandboxContext, SandboxError,
    SandboxProposal, SandboxRequest, SandboxResponse, execute, validate_setting_value,
};

/// Version of the plugin host API surface.
pub const PLUGIN_VERSION: &str = "0.1.0";
