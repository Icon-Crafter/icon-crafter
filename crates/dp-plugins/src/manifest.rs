//! Plugin manifests: what a plugin declares before it is installed.
//!
//! A manifest is data only — there is no code payload in the op-scoped sandbox
//! (the plugin's behaviour is a host-provided handler that may only issue typed
//! [`crate::sandbox::SandboxRequest`]s). Manifests are validated strictly and
//! rejected on unknown versions so a future format change cannot be silently
//! misread.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::permissions::{Permission, API_VERSION};

/// Supported manifest schema version.
pub const MANIFEST_VERSION: u32 = 1;
/// Maximum number of commands a plugin may declare.
pub const MAX_COMMANDS: usize = 32;
/// Maximum number of panels a plugin may declare.
pub const MAX_PANELS: usize = 8;
/// Maximum number of settings a plugin may declare.
pub const MAX_SETTINGS: usize = 32;

/// A setting a plugin exposes in its preferences UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginSetting {
    /// Free-form string.
    String {
        /// Stable key.
        id: String,
        /// Human label.
        label: String,
        /// Default value.
        #[serde(default)]
        default: String,
    },
    /// Numeric value.
    Number {
        /// Stable key.
        id: String,
        /// Human label.
        label: String,
        /// Default value.
        #[serde(default)]
        default: f64,
    },
    /// Boolean toggle.
    Boolean {
        /// Stable key.
        id: String,
        /// Human label.
        label: String,
        /// Default value.
        #[serde(default)]
        default: bool,
    },
    /// One of a fixed list.
    Select {
        /// Stable key.
        id: String,
        /// Human label.
        label: String,
        /// Allowed values.
        options: Vec<String>,
        /// Default value.
        default: String,
    },
}

impl PluginSetting {
    /// Stable key.
    pub fn id(&self) -> &str {
        match self {
            PluginSetting::String { id, .. }
            | PluginSetting::Number { id, .. }
            | PluginSetting::Boolean { id, .. }
            | PluginSetting::Select { id, .. } => id,
        }
    }

    /// Human label.
    pub fn label(&self) -> &str {
        match self {
            PluginSetting::String { label, .. }
            | PluginSetting::Number { label, .. }
            | PluginSetting::Boolean { label, .. }
            | PluginSetting::Select { label, .. } => label,
        }
    }

    /// Default value as JSON.
    pub fn default_value(&self) -> Value {
        match self {
            PluginSetting::String { default, .. } => Value::String(default.clone()),
            PluginSetting::Number { default, .. } => {
                serde_json::Number::from_f64(*default).map_or(Value::Null, Value::Number)
            }
            PluginSetting::Boolean { default, .. } => Value::Bool(*default),
            PluginSetting::Select { default, .. } => Value::String(default.clone()),
        }
    }

    /// Whether a JSON value is acceptable for this setting.
    pub fn accepts(&self, value: &Value) -> bool {
        match self {
            PluginSetting::String { .. } => value.is_string(),
            PluginSetting::Number { .. } => value.is_number(),
            PluginSetting::Boolean { .. } => value.is_boolean(),
            PluginSetting::Select { options, .. } => value
                .as_str()
                .is_some_and(|text| options.iter().any(|option| option == text)),
        }
    }

    fn validate(&self) -> Result<(), ManifestError> {
        validate_id(self.id(), "setting").map_err(|error| ManifestError::InvalidSetting(error.0))?;
        if self.label().trim().is_empty() {
            return Err(ManifestError::InvalidSetting(format!(
                "setting '{}' needs a label",
                self.id()
            )));
        }
        if let PluginSetting::Select {
            options, default, ..
        } = self
        {
            if options.is_empty() {
                return Err(ManifestError::InvalidSetting(format!(
                    "select setting '{}' has no options",
                    self.id()
                )));
            }
            if !options.iter().any(|option| option == default) {
                return Err(ManifestError::InvalidSetting(format!(
                    "select setting '{}' default is not an option",
                    self.id()
                )));
            }
        }
        Ok(())
    }
}

/// A command exposed in the command palette / menus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginCommand {
    /// Stable id, unique inside the plugin.
    pub id: String,
    /// Palette title.
    pub title: String,
    /// Optional description.
    #[serde(default)]
    pub description: String,
}

/// A custom panel contributed to the editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginPanel {
    /// Stable id.
    pub id: String,
    /// Panel title.
    pub title: String,
}

/// Document events a plugin can subscribe to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginEvent {
    /// Any document mutation.
    DocumentChanged,
    /// Selection changed.
    SelectionChanged,
    /// Active page changed.
    PageChanged,
    /// Viewport changed.
    ViewportChanged,
    /// Theme changed.
    ThemeChanged,
}

/// A validated plugin manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Manifest schema version.
    pub manifest_version: u32,
    /// Reverse-DNS-ish kebab-case plugin id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Semantic version of the plugin itself.
    pub version: String,
    /// Plugin API version the plugin was written against.
    pub api_version: u32,
    /// Short description.
    #[serde(default)]
    pub description: String,
    /// Author credit.
    #[serde(default)]
    pub author: Option<String>,
    /// Homepage / docs URL.
    #[serde(default)]
    pub homepage: Option<String>,
    /// Declared permissions.
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// Declared commands.
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    /// Declared panels.
    #[serde(default)]
    pub panels: Vec<PluginPanel>,
    /// Subscribed events.
    #[serde(default)]
    pub events: Vec<PluginEvent>,
    /// Declared settings.
    #[serde(default)]
    pub settings: Vec<PluginSetting>,
}

/// Manifest validation failures.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ManifestError {
    /// JSON did not deserialize.
    #[error("manifest is not valid JSON for a plugin: {0}")]
    Parse(String),
    /// Manifest version newer/older than this host reads.
    #[error("unsupported manifest version {found} (supported: {supported})")]
    UnsupportedManifestVersion {
        /// Version found in the file.
        found: u32,
        /// Version this host supports.
        supported: u32,
    },
    /// Plugin id violates the naming rules.
    #[error("invalid plugin id: {0}")]
    InvalidId(String),
    /// Version string is not semver-like.
    #[error("invalid version '{0}' (expected MAJOR.MINOR.PATCH)")]
    InvalidVersion(String),
    /// API version mismatch.
    #[error("plugin uses API version {found}; this host supports {supported} (compatible plugins are rejected loudly)")]
    UnsupportedApi {
        /// Version declared by the plugin.
        found: u32,
        /// Version this host implements.
        supported: u32,
    },
    /// Two commands share an id.
    #[error("duplicate command id '{0}'")]
    DuplicateCommand(String),
    /// Two panels share an id.
    #[error("duplicate panel id '{0}'")]
    DuplicatePanel(String),
    /// Two settings share an id.
    #[error("duplicate setting id '{0}'")]
    DuplicateSetting(String),
    /// A setting is malformed.
    #[error("invalid setting: {0}")]
    InvalidSetting(String),
    /// Too many commands/panels/settings.
    #[error("plugin declares too many {0} (max {1})")]
    TooMany(&'static str, usize),
    /// Permission payload invalid.
    #[error("invalid permission: {0}")]
    InvalidPermission(String),
    /// Duplicate event subscription.
    #[error("duplicate event subscription '{0:?}'")]
    DuplicateEvent(PluginEvent),
}

impl PluginManifest {
    /// Parses and validates a manifest from JSON text.
    pub fn from_json(text: &str) -> Result<PluginManifest, ManifestError> {
        let manifest: PluginManifest =
            serde_json::from_str(text).map_err(|error| ManifestError::Parse(error.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates every field and cross-constraint.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.manifest_version != MANIFEST_VERSION {
            return Err(ManifestError::UnsupportedManifestVersion {
                found: self.manifest_version,
                supported: MANIFEST_VERSION,
            });
        }
        validate_id(&self.id, "plugin id").map_err(|error| ManifestError::InvalidId(error.0))?;
        if self.name.trim().is_empty() || self.name.len() > 80 {
            return Err(ManifestError::InvalidId("plugin name is empty or too long".to_owned()));
        }
        validate_semver(&self.version)?;
        if self.api_version != API_VERSION {
            return Err(ManifestError::UnsupportedApi {
                found: self.api_version,
                supported: API_VERSION,
            });
        }
        if self.commands.len() > MAX_COMMANDS {
            return Err(ManifestError::TooMany("commands", MAX_COMMANDS));
        }
        let mut command_ids = HashSet::new();
        for command in &self.commands {
            validate_id(&command.id, "command").map_err(|error| ManifestError::InvalidId(error.0))?;
            if !command_ids.insert(command.id.clone()) {
                return Err(ManifestError::DuplicateCommand(command.id.clone()));
            }
            if command.title.trim().is_empty() {
                return Err(ManifestError::InvalidId(format!(
                    "command '{}' needs a title",
                    command.id
                )));
            }
        }
        if self.panels.len() > MAX_PANELS {
            return Err(ManifestError::TooMany("panels", MAX_PANELS));
        }
        let mut panel_ids = HashSet::new();
        for panel in &self.panels {
            validate_id(&panel.id, "panel").map_err(|error| ManifestError::InvalidId(error.0))?;
            if !panel_ids.insert(panel.id.clone()) {
                return Err(ManifestError::DuplicatePanel(panel.id.clone()));
            }
        }
        if self.settings.len() > MAX_SETTINGS {
            return Err(ManifestError::TooMany("settings", MAX_SETTINGS));
        }
        let mut setting_ids = HashSet::new();
        for setting in &self.settings {
            setting.validate()?;
            if !setting_ids.insert(setting.id().to_owned()) {
                return Err(ManifestError::DuplicateSetting(setting.id().to_owned()));
            }
        }
        let mut events = HashSet::new();
        for event in &self.events {
            if !events.insert(*event) {
                return Err(ManifestError::DuplicateEvent(*event));
            }
        }
        for permission in &self.permissions {
            permission
                .validate()
                .map_err(ManifestError::InvalidPermission)?;
        }
        if let Some(homepage) = &self.homepage
            && !(homepage.starts_with("https://") || homepage.starts_with("http://"))
        {
            return Err(ManifestError::InvalidId(
                "homepage must be an http(s) URL".to_owned(),
            ));
        }
        Ok(())
    }

    /// Whether the plugin subscribes to an event.
    pub fn subscribes(&self, event: PluginEvent) -> bool {
        self.events.contains(&event)
    }

    /// Looks up a command.
    pub fn command(&self, id: &str) -> Option<&PluginCommand> {
        self.commands.iter().find(|command| command.id == id)
    }

    /// Looks up a settings declaration.
    pub fn setting(&self, id: &str) -> Option<&PluginSetting> {
        self.settings.iter().find(|setting| setting.id() == id)
    }

    /// Default settings map.
    pub fn default_settings(&self) -> serde_json::Map<String, Value> {
        self.settings
            .iter()
            .map(|setting| (setting.id().to_owned(), setting.default_value()))
            .collect()
    }
}

struct IdError(String);

/// Plugin/command/panel/setting ids share one naming rule.
fn validate_id(value: &str, kind: &str) -> Result<(), IdError> {
    let valid = (3..=64).contains(&value.len())
        && value
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--");
    if valid {
        Ok(())
    } else {
        Err(IdError(format!(
            "{kind} '{value}' must be 3-64 lowercase letters, digits or single dashes"
        )))
    }
}

fn validate_semver(version: &str) -> Result<(), ManifestError> {
    let core = version.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    let valid = parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|character| character.is_ascii_digit()));
    if valid {
        Ok(())
    } else {
        Err(ManifestError::InvalidVersion(version.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_json() -> &'static str {
        r#"{
          "manifest_version": 1,
          "id": "acme-icon-tools",
          "name": "Acme Icon Tools",
          "version": "1.2.0",
          "api_version": 1,
          "description": "Icon helpers",
          "permissions": ["read_document", "write_document"],
          "commands": [{"id": "normalize-icons", "title": "Normalize icons"}],
          "panels": [{"id": "icon-panel", "title": "Icons"}],
          "events": ["document_changed"],
          "settings": [{"type": "boolean", "id": "snap", "label": "Snap", "default": true}]
        }"#
    }

    #[test]
    fn valid_manifest_parses() {
        let manifest = PluginManifest::from_json(valid_json()).expect("valid manifest");
        assert_eq!(manifest.id, "acme-icon-tools");
        assert!(manifest.subscribes(PluginEvent::DocumentChanged));
        assert_eq!(manifest.command("normalize-icons").unwrap().title, "Normalize icons");
        assert_eq!(
            manifest.setting("snap").unwrap().default_value(),
            Value::Bool(true)
        );
    }

    #[test]
    fn wrong_manifest_version_is_rejected() {
        let json = valid_json().replace("\"manifest_version\": 1", "\"manifest_version\": 2");
        assert!(matches!(
            PluginManifest::from_json(&json),
            Err(ManifestError::UnsupportedManifestVersion { found: 2, .. })
        ));
    }

    #[test]
    fn wrong_api_version_is_rejected() {
        let json = valid_json().replace("\"api_version\": 1", "\"api_version\": 7");
        assert!(matches!(
            PluginManifest::from_json(&json),
            Err(ManifestError::UnsupportedApi { found: 7, supported: 1 })
        ));
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for bad in ["UPPER", "a", "bad id", "-lead", "x--y", "trail-"] {
            let json = valid_json().replace("\"acme-icon-tools\"", &format!("\"{bad}\""));
            assert!(PluginManifest::from_json(&json).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn duplicate_commands_and_settings_are_rejected() {
        let json = valid_json().replace(
            "\"commands\": [{\"id\": \"normalize-icons\", \"title\": \"Normalize icons\"}]",
            "\"commands\": [{\"id\": \"normalize-icons\", \"title\": \"A\"}, {\"id\": \"normalize-icons\", \"title\": \"B\"}]",
        );
        assert!(matches!(
            PluginManifest::from_json(&json),
            Err(ManifestError::DuplicateCommand(_))
        ));

        let json = valid_json().replace(
            "\"settings\": [{\"type\": \"boolean\", \"id\": \"snap\", \"label\": \"Snap\", \"default\": true}]",
            "\"settings\": [{\"type\": \"boolean\", \"id\": \"snap\", \"label\": \"Snap\", \"default\": true}, {\"type\": \"number\", \"id\": \"snap\", \"label\": \"Snap again\", \"default\": 1.0}]",
        );
        assert!(matches!(
            PluginManifest::from_json(&json),
            Err(ManifestError::DuplicateSetting(_))
        ));
    }

    #[test]
    fn selects_require_matching_default() {
        let json = valid_json().replace(
            "\"settings\": [{\"type\": \"boolean\", \"id\": \"snap\", \"label\": \"Snap\", \"default\": true}]",
            "\"settings\": [{\"type\": \"select\", \"id\": \"mode\", \"label\": \"Mode\", \"options\": [\"a\", \"b\"], \"default\": \"c\"}]",
        );
        assert!(matches!(
            PluginManifest::from_json(&json),
            Err(ManifestError::InvalidSetting(_))
        ));
    }

    #[test]
    fn example_plugin_manifest_is_valid() {
        let text = include_str!("../examples/hello-plugin/plugin.json");
        let manifest = PluginManifest::from_json(text).expect("example manifest");
        assert_eq!(manifest.id, "hello-icon-crafter");
        assert_eq!(manifest.commands.len(), 1);
        assert!(manifest.permissions.contains(&Permission::ReadDocument));
    }
}
