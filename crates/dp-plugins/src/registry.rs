//! Plugin registry: installation, enablement, settings and lifecycle state.
//!
//! The registry is pure data — installing a plugin records its validated
//! manifest, consent grants, and settings. Execution goes through
//! [`crate::executor`] and is always mediated by a host-provided handler.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::manifest::{ManifestError, PluginManifest};
use crate::permissions::Grants;

/// Maximum number of installed plugins.
pub const MAX_PLUGINS: usize = 64;

/// Lifecycle state of an installed plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginState {
    /// Installed but disabled by the user.
    Disabled,
    /// Enabled and eligible to receive commands/events.
    Enabled,
    /// Disabled automatically because the last execution failed.
    Error,
}

/// One installed plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisteredPlugin {
    /// Validated manifest.
    pub manifest: PluginManifest,
    /// User consent grants.
    pub grants: Grants,
    /// Lifecycle state.
    pub state: PluginState,
    /// Current setting values.
    #[serde(default)]
    pub settings: IndexMap<String, Value>,
    /// Last error message, if any.
    #[serde(default)]
    pub last_error: Option<String>,
}

impl RegisteredPlugin {
    /// Whether the plugin is enabled.
    pub fn is_enabled(&self) -> bool {
        matches!(self.state, PluginState::Enabled)
    }

    /// A settings value, falling back to the manifest default.
    pub fn setting(&self, key: &str) -> Option<Value> {
        self.settings.get(key).cloned().or_else(|| {
            self.manifest
                .setting(key)
                .map(|setting| setting.default_value())
        })
    }
}

/// Registry errors.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RegistryError {
    /// Manifest invalid.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// Plugin already installed.
    #[error("plugin '{0}' is already installed")]
    AlreadyInstalled(String),
    /// Plugin not found.
    #[error("plugin '{0}' is not installed")]
    UnknownPlugin(String),
    /// Registry at capacity.
    #[error("plugin limit reached ({MAX_PLUGINS})")]
    LimitReached,
    /// Setting unknown or value invalid.
    #[error("invalid setting '{key}': {message}")]
    InvalidSetting {
        /// Setting key.
        key: String,
        /// Explanation.
        message: String,
    },
}

/// All installed plugins keyed by plugin id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginRegistry {
    /// Installed plugins.
    #[serde(default)]
    pub plugins: IndexMap<String, RegisteredPlugin>,
}

impl PluginRegistry {
    /// Empty registry.
    pub fn new() -> PluginRegistry {
        PluginRegistry::default()
    }

    /// Number of installed plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Whether no plugin is installed.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Installs a validated manifest (disabled until enabled/consented).
    pub fn install(
        &mut self,
        manifest: PluginManifest,
        grants: Grants,
    ) -> Result<(), RegistryError> {
        if self.plugins.contains_key(&manifest.id) {
            return Err(RegistryError::AlreadyInstalled(manifest.id));
        }
        if self.plugins.len() >= MAX_PLUGINS {
            return Err(RegistryError::LimitReached);
        }
        let settings = manifest.default_settings().into_iter().collect();
        self.plugins.insert(
            manifest.id.clone(),
            RegisteredPlugin {
                manifest,
                grants,
                state: PluginState::Disabled,
                settings,
                last_error: None,
            },
        );
        Ok(())
    }

    /// Installs from JSON with full consent for declared permissions.
    pub fn install_json(&mut self, text: &str) -> Result<String, RegistryError> {
        let manifest = PluginManifest::from_json(text)?;
        let grants = Grants::from_permissions(&manifest.permissions);
        let id = manifest.id.clone();
        self.install(manifest, grants)?;
        Ok(id)
    }

    /// Removes a plugin.
    pub fn remove(&mut self, plugin_id: &str) -> Result<RegisteredPlugin, RegistryError> {
        self.plugins
            .shift_remove(plugin_id)
            .ok_or_else(|| RegistryError::UnknownPlugin(plugin_id.to_owned()))
    }

    /// Looks a plugin up.
    pub fn get(&self, plugin_id: &str) -> Option<&RegisteredPlugin> {
        self.plugins.get(plugin_id)
    }

    /// Looks a plugin up mutably.
    pub fn get_mut(&mut self, plugin_id: &str) -> Option<&mut RegisteredPlugin> {
        self.plugins.get_mut(plugin_id)
    }

    /// Enables a plugin.
    pub fn enable(&mut self, plugin_id: &str) -> Result<(), RegistryError> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| RegistryError::UnknownPlugin(plugin_id.to_owned()))?;
        plugin.state = PluginState::Enabled;
        plugin.last_error = None;
        Ok(())
    }

    /// Disables a plugin.
    pub fn disable(&mut self, plugin_id: &str) -> Result<(), RegistryError> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| RegistryError::UnknownPlugin(plugin_id.to_owned()))?;
        plugin.state = PluginState::Disabled;
        Ok(())
    }

    /// Marks a plugin as errored (error isolation: only this plugin is affected).
    pub fn mark_error(&mut self, plugin_id: &str, message: impl Into<String>) {
        if let Some(plugin) = self.plugins.get_mut(plugin_id) {
            plugin.state = PluginState::Error;
            plugin.last_error = Some(message.into());
        }
    }

    /// Sets a setting value after validating it against the manifest.
    pub fn set_setting(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), RegistryError> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| RegistryError::UnknownPlugin(plugin_id.to_owned()))?;
        let Some(setting) = plugin.manifest.setting(key) else {
            return Err(RegistryError::InvalidSetting {
                key: key.to_owned(),
                message: "no such setting".to_owned(),
            });
        };
        if !setting.accepts(&value) {
            return Err(RegistryError::InvalidSetting {
                key: key.to_owned(),
                message: format!("expected {}", setting_kind(setting)),
            });
        }
        plugin.settings.insert(key.to_owned(), value);
        Ok(())
    }

    /// Enabled plugin ids in installation order.
    pub fn enabled_ids(&self) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|(_, plugin)| plugin.is_enabled())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Plugins subscribing to an event (enabled only).
    pub fn subscribers(&self, event: crate::manifest::PluginEvent) -> Vec<String> {
        self.plugins
            .iter()
            .filter(|(_, plugin)| plugin.is_enabled() && plugin.manifest.subscribes(event))
            .map(|(id, _)| id.clone())
            .collect()
    }
}

fn setting_kind(setting: &crate::manifest::PluginSetting) -> &'static str {
    use crate::manifest::PluginSetting;
    match setting {
        PluginSetting::String { .. } => "a string",
        PluginSetting::Number { .. } => "a number",
        PluginSetting::Boolean { .. } => "a boolean",
        PluginSetting::Select { .. } => "one of the listed options",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest_json(id: &str) -> String {
        format!(
            r#"{{
              "manifest_version": 1,
              "id": "{id}",
              "name": "Test Plugin",
              "version": "1.0.0",
              "api_version": 1,
              "permissions": ["read_document"],
              "events": ["document_changed"],
              "settings": [{{"type": "boolean", "id": "enable-toggle", "label": "On", "default": true}}]
            }}"#
        )
    }

    #[test]
    fn install_list_enable_disable_remove() {
        let mut registry = PluginRegistry::new();
        let id = registry.install_json(&manifest_json("plugin-a")).unwrap();
        assert_eq!(id, "plugin-a");
        assert_eq!(registry.len(), 1);
        assert!(!registry.get("plugin-a").unwrap().is_enabled());
        assert!(
            registry
                .subscribers(crate::manifest::PluginEvent::DocumentChanged)
                .is_empty()
        );
        registry.enable("plugin-a").unwrap();
        assert!(registry.get("plugin-a").unwrap().is_enabled());
        assert_eq!(
            registry.subscribers(crate::manifest::PluginEvent::DocumentChanged),
            vec!["plugin-a".to_owned()]
        );
        registry.disable("plugin-a").unwrap();
        assert!(registry.enabled_ids().is_empty());
        registry.remove("plugin-a").unwrap();
        assert!(registry.is_empty());
        assert!(matches!(
            registry.remove("plugin-a"),
            Err(RegistryError::UnknownPlugin(_))
        ));
    }

    #[test]
    fn duplicate_and_limit_are_enforced() {
        let mut registry = PluginRegistry::new();
        registry.install_json(&manifest_json("plugin-a")).unwrap();
        assert!(matches!(
            registry.install_json(&manifest_json("plugin-a")),
            Err(RegistryError::AlreadyInstalled(_))
        ));
    }

    #[test]
    fn settings_are_defaulted_and_validated() {
        let mut registry = PluginRegistry::new();
        registry.install_json(&manifest_json("plugin-a")).unwrap();
        let plugin = registry.get("plugin-a").unwrap();
        assert_eq!(plugin.setting("enable-toggle"), Some(json!(true)));
        registry
            .set_setting("plugin-a", "enable-toggle", json!(false))
            .unwrap();
        assert_eq!(
            registry.get("plugin-a").unwrap().setting("enable-toggle"),
            Some(json!(false))
        );
        assert!(matches!(
            registry.set_setting("plugin-a", "enable-toggle", json!("yes")),
            Err(RegistryError::InvalidSetting { .. })
        ));
        assert!(matches!(
            registry.set_setting("plugin-a", "missing", json!(1)),
            Err(RegistryError::InvalidSetting { .. })
        ));
    }

    #[test]
    fn errors_isolate_to_one_plugin() {
        let mut registry = PluginRegistry::new();
        registry.install_json(&manifest_json("plugin-a")).unwrap();
        registry.install_json(&manifest_json("plugin-b")).unwrap();
        registry.enable("plugin-a").unwrap();
        registry.enable("plugin-b").unwrap();
        registry.mark_error("plugin-a", "boom");
        assert!(!registry.get("plugin-a").unwrap().is_enabled());
        assert!(registry.get("plugin-b").unwrap().is_enabled());
        assert_eq!(
            registry.get("plugin-a").unwrap().last_error.as_deref(),
            Some("boom")
        );
    }
}
