//! Isolated plugin command/event execution.
//!
//! The host owns the executable part of a plugin (the op-scoped sandbox has no
//! plugin-authored code). For every call the executor:
//! 1. checks the plugin is installed, enabled and declares the command/event;
//! 2. runs the host handler inside `catch_unwind` so a panicking plugin cannot
//!    take down the editor;
//! 3. executes every [`SandboxRequest`] through [`crate::sandbox::execute`],
//!    collecting proposals and conversions;
//! 4. records failures per plugin and flips only that plugin to `Error`.

use std::panic::AssertUnwindSafe;

use serde::{Deserialize, Serialize};

use crate::manifest::PluginEvent;
use crate::permissions::Grants;
use crate::registry::{PluginRegistry, PluginState};
use crate::sandbox::{execute, SandboxContext, SandboxError, SandboxProposal, SandboxRequest, SandboxResponse};

/// Maximum number of requests one handler invocation may issue.
pub const MAX_REQUESTS_PER_CALL: usize = 128;

/// What the host is asking a plugin to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HandlerCall {
    /// A subscribed document event.
    Event {
        /// Event that fired.
        event: PluginEvent,
    },
    /// A user-invoked command.
    Command {
        /// Command id from the manifest.
        command_id: String,
    },
}

/// The host-provided executable half of a plugin.
///
/// Implementations are registered by the host for a plugin id and may only
/// return typed requests; they receive no document reference and no I/O handle.
pub trait PluginHandler {
    /// Produces sandbox requests for one call.
    fn handle(
        &mut self,
        plugin_id: &str,
        call: &HandlerCall,
        context: &SandboxContext<'_>,
    ) -> Result<Vec<SandboxRequest>, String>;
}

/// One failure recorded during dispatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginErrorRecord {
    /// Plugin that failed.
    pub plugin_id: String,
    /// Failure description.
    pub message: String,
}

/// Outcome of one dispatch.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ExecutionReport {
    /// Plugins that were invoked successfully.
    pub delivered: Vec<String>,
    /// Validated proposals for the editor to confirm/apply.
    pub proposals: Vec<SandboxProposal>,
    /// Read responses returned to the caller/plugin host.
    pub responses: Vec<SandboxResponse>,
    /// Per-plugin failures (error isolation).
    pub errors: Vec<PluginErrorRecord>,
}

impl ExecutionReport {
    fn merge(&mut self, other: ExecutionReport) {
        self.delivered.extend(other.delivered);
        self.proposals.extend(other.proposals);
        self.responses.extend(other.responses);
        self.errors.extend(other.errors);
    }
}

/// Dispatches an event to every enabled subscriber.
pub fn dispatch_event(
    handler: &mut dyn PluginHandler,
    registry: &mut PluginRegistry,
    event: PluginEvent,
    context: &SandboxContext<'_>,
) -> ExecutionReport {
    let subscribers = registry.subscribers(event);
    let mut report = ExecutionReport::default();
    for plugin_id in subscribers {
        report.merge(invoke(
            handler,
            registry,
            &plugin_id,
            &HandlerCall::Event { event },
            context,
        ));
    }
    report
}

/// Runs a declared command for one plugin.
pub fn run_command(
    handler: &mut dyn PluginHandler,
    registry: &mut PluginRegistry,
    plugin_id: &str,
    command_id: &str,
    context: &SandboxContext<'_>,
) -> ExecutionReport {
    let mut report = invoke(
        handler,
        registry,
        plugin_id,
        &HandlerCall::Command {
            command_id: command_id.to_owned(),
        },
        context,
    );
    report.delivered.retain(|id| id == plugin_id);
    report
}

fn invoke(
    handler: &mut dyn PluginHandler,
    registry: &mut PluginRegistry,
    plugin_id: &str,
    call: &HandlerCall,
    context: &SandboxContext<'_>,
) -> ExecutionReport {
    let mut report = ExecutionReport::default();
    let Some(plugin) = registry.get(plugin_id).cloned() else {
        report.errors.push(PluginErrorRecord {
            plugin_id: plugin_id.to_owned(),
            message: "plugin is not installed".to_owned(),
        });
        return report;
    };
    if !plugin.is_enabled() {
        report.errors.push(PluginErrorRecord {
            plugin_id: plugin_id.to_owned(),
            message: "plugin is disabled".to_owned(),
        });
        return report;
    }
    match call {
        HandlerCall::Command { command_id } => {
            if plugin.manifest.command(command_id).is_none() {
                report.errors.push(PluginErrorRecord {
                    plugin_id: plugin_id.to_owned(),
                    message: format!("command '{command_id}' is not declared"),
                });
                return report;
            }
        }
        HandlerCall::Event { event } => {
            if !plugin.manifest.subscribes(*event) {
                return report;
            }
        }
    }

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| handler.handle(plugin_id, call, context)));
    let requests = match result {
        Ok(Ok(requests)) => requests,
        Ok(Err(message)) => {
            registry.mark_error(plugin_id, message.clone());
            report.errors.push(PluginErrorRecord {
                plugin_id: plugin_id.to_owned(),
                message,
            });
            return report;
        }
        Err(_) => {
            let message = "plugin handler panicked".to_owned();
            registry.mark_error(plugin_id, message.clone());
            report.errors.push(PluginErrorRecord {
                plugin_id: plugin_id.to_owned(),
                message,
            });
            return report;
        }
    };
    if requests.len() > MAX_REQUESTS_PER_CALL {
        let message = format!("handler issued more than {MAX_REQUESTS_PER_CALL} requests");
        registry.mark_error(plugin_id, message.clone());
        report.errors.push(PluginErrorRecord {
            plugin_id: plugin_id.to_owned(),
            message,
        });
        return report;
    }

    let grants: Grants = plugin.grants.clone();
    for request in &requests {
        match execute(&plugin.manifest, &grants, context, request) {
            Ok(SandboxResponse::Proposed { proposal }) => report.proposals.push(proposal),
            Ok(response) => report.responses.push(response),
            Err(SandboxError::Denied(permission)) => {
                report.errors.push(PluginErrorRecord {
                    plugin_id: plugin_id.to_owned(),
                    message: format!("permission denied: {permission}"),
                });
                registry.mark_error(plugin_id, format!("permission denied: {permission}"));
                return report;
            }
            Err(error) => {
                report.errors.push(PluginErrorRecord {
                    plugin_id: plugin_id.to_owned(),
                    message: error.to_string(),
                });
                registry.mark_error(plugin_id, error.to_string());
                return report;
            }
        }
    }
    // Restore Enabled state on success (catch_unwind path above may have flipped it).
    if let Some(plugin) = registry.get_mut(plugin_id)
        && matches!(plugin.state, PluginState::Enabled)
    {
        plugin.last_error = None;
    }
    report.delivered.push(plugin_id.to_owned());
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::permissions::Grants;
    use crate::registry::PluginRegistry;
    use crate::sandbox::SandboxContext;
    use dp_model::Document;
    use serde_json::json;

    struct Handler {
        panic_for: Option<String>,
        deny_for: Option<String>,
    }

    impl PluginHandler for Handler {
        fn handle(
            &mut self,
            plugin_id: &str,
            _call: &HandlerCall,
            _context: &SandboxContext<'_>,
        ) -> Result<Vec<SandboxRequest>, String> {
            if self.panic_for.as_deref() == Some(plugin_id) {
                panic!("plugin exploded");
            }
            if self.deny_for.as_deref() == Some(plugin_id) {
                return Ok(vec![SandboxRequest::GetDocumentSummary]);
            }
            Ok(vec![SandboxRequest::GetDocumentSummary])
        }
    }

    fn install(registry: &mut PluginRegistry, id: &str, permissions: &str) {
        let text = format!(
            r#"{{
              "manifest_version": 1,
              "id": "{id}",
              "name": "Test",
              "version": "1.0.0",
              "api_version": 1,
              "permissions": {permissions},
              "events": ["document_changed"]
            }}"#
        );
        let manifest = PluginManifest::from_json(&text).unwrap();
        registry
            .install(manifest.clone(), Grants::from_permissions(&manifest.permissions))
            .unwrap();
        registry.enable(id).unwrap();
    }

    #[test]
    fn events_deliver_to_subscribers_and_isolate_panics() {
        let mut registry = PluginRegistry::new();
        install(&mut registry, "plugin-a", r#"["read_document"]"#);
        install(&mut registry, "plugin-b", r#"["read_document"]"#);
        let mut handler = Handler {
            panic_for: Some("plugin-a".to_owned()),
            deny_for: None,
        };
        let document = Document::new("Plugins");
        let context = SandboxContext {
            document: &document,
            selection: &[],
        };
        let report = dispatch_event(
            &mut handler,
            &mut registry,
            PluginEvent::DocumentChanged,
            &context,
        );
        assert_eq!(report.delivered, vec!["plugin-b".to_owned()]);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].plugin_id, "plugin-a");
        assert_eq!(registry.get("plugin-a").unwrap().state, PluginState::Error);
        assert_eq!(registry.get("plugin-b").unwrap().state, PluginState::Enabled);
    }

    #[test]
    fn permission_denial_isolates_plugin() {
        let mut registry = PluginRegistry::new();
        install(&mut registry, "plugin-a", r#"[]"#);
        let mut handler = Handler {
            panic_for: None,
            deny_for: None,
        };
        let document = Document::new("Plugins");
        let context = SandboxContext {
            document: &document,
            selection: &[],
        };
        let report = dispatch_event(
            &mut handler,
            &mut registry,
            PluginEvent::DocumentChanged,
            &context,
        );
        assert!(report.delivered.is_empty());
        assert!(report.errors[0].message.contains("permission denied"));
        assert_eq!(registry.get("plugin-a").unwrap().state, PluginState::Error);
    }

    #[test]
    fn undeclared_commands_are_rejected() {
        let mut registry = PluginRegistry::new();
        install(&mut registry, "plugin-a", r#"["read_document"]"#);
        let mut handler = Handler {
            panic_for: None,
            deny_for: None,
        };
        let document = Document::new("Plugins");
        let context = SandboxContext {
            document: &document,
            selection: &[],
        };
        let report = run_command(&mut handler, &mut registry, "plugin-a", "nope", &context);
        assert!(report.errors[0].message.contains("not declared"));
        assert_eq!(registry.get("plugin-a").unwrap().state, PluginState::Enabled);
    }

    #[test]
    fn disabled_plugins_are_skipped() {
        let mut registry = PluginRegistry::new();
        install(&mut registry, "plugin-a", r#"["read_document"]"#);
        registry.disable("plugin-a").unwrap();
        let mut handler = Handler {
            panic_for: None,
            deny_for: None,
        };
        let document = Document::new("Plugins");
        let context = SandboxContext {
            document: &document,
            selection: &[],
        };
        // Event dispatch only targets enabled subscribers.
        let report = dispatch_event(
            &mut handler,
            &mut registry,
            PluginEvent::DocumentChanged,
            &context,
        );
        assert!(report.delivered.is_empty());
        assert!(report.errors.is_empty());
        // Direct command invocation reports the disabled state.
        let report = run_command(&mut handler, &mut registry, "plugin-a", "x", &context);
        assert_eq!(report.errors[0].message, "plugin is disabled");
        // Unknown plugin error path.
        let report = run_command(&mut handler, &mut registry, "ghost", "x", &context);
        assert_eq!(report.errors[0].message, "plugin is not installed");
        let _ = json!({});
    }
}
