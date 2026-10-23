//! AI assistance and plugin manager commands.
//!
//! AI changes never bypass the editor: proposals are built and validated by
//! `dp_ai` against a clone of the document, stored until the user applies or
//! discards them, and then applied through the ordinary undoable transaction
//! path. Destructive proposals require explicit confirmation.
//!
//! Plugins are managed through `dp_plugins::PluginRegistry`. Execution is
//! op-scoped and host-provided: there is no arbitrary-code plugin runtime yet,
//! which is documented in `docs/FEATURE-CHECKLIST.md` rather than hidden.

use dp_ai::{AiEngine, AiError};
use serde_json::Value;

use crate::{EditorError, EditorState};

fn ai_error(error: AiError) -> EditorError {
    match error {
        AiError::ConfirmationRequired => EditorError::InvalidArguments(
            "confirmation required: this AI change deletes content".to_owned(),
        ),
        other => EditorError::InvalidArguments(format!("AI: {other}")),
    }
}

fn registry_error(error: impl std::fmt::Display) -> EditorError {
    EditorError::InvalidArguments(format!("plugin: {error}"))
}

impl EditorState {
    // -----------------------------------------------------------------------
    // AI assistance
    // -----------------------------------------------------------------------

    /// Generates a layout proposal from a prompt and stores it for review.
    ///
    /// Returns the number of proposed changes. Nothing is applied yet: the UI
    /// shows the proposal summary and calls [`Self::ai_apply_last_proposal`].
    pub fn ai_generate_layout(
        &mut self,
        prompt: &str,
        width: u32,
        height: u32,
    ) -> Result<usize, EditorError> {
        if prompt.trim().is_empty() {
            return Err(EditorError::InvalidArguments(
                "AI prompt is empty".to_owned(),
            ));
        }
        let mut engine = AiEngine::from_env();
        let proposal = engine
            .generate_layout(
                &self.document,
                prompt,
                width.clamp(64, 10_000),
                height.clamp(64, 10_000),
            )
            .map_err(ai_error)?;
        let steps = proposal.steps.len();
        let confirmation = if proposal.requires_confirmation {
            " (needs confirmation: deletes content)"
        } else {
            ""
        };
        self.last_ai_proposal = Some(proposal);
        self.set_status(format!(
            "AI proposed {steps} change(s){confirmation} — review, then apply or discard"
        ));
        Ok(steps)
    }

    /// Applies the stored AI proposal as undoable transactions.
    pub fn ai_apply_last_proposal(&mut self, confirmed: bool) -> Result<usize, EditorError> {
        let Some(proposal) = self.last_ai_proposal.clone() else {
            return Err(EditorError::InvalidArguments(
                "no AI proposal to apply".to_owned(),
            ));
        };
        let applied = dp_ai::apply(&mut self.document, &mut self.history, &proposal, confirmed)
            .map_err(ai_error)?;
        self.last_ai_proposal = None;
        self.invalidate_layout();
        self.set_status(format!("AI applied {applied} change(s) (undoable)"));
        Ok(applied)
    }

    /// Discards the stored AI proposal. Returns whether one existed.
    pub fn ai_discard_proposal(&mut self) -> bool {
        let existed = self.last_ai_proposal.take().is_some();
        if existed {
            self.set_status("Discarded AI proposal");
        }
        existed
    }

    /// Suggests and applies layer renames for the selection (or up to 100 page
    /// layers). Returns the number of renamed layers.
    pub fn ai_rename_layers(&mut self) -> Result<usize, EditorError> {
        let page = self.document.active_page();
        let targets: Vec<(String, dp_model::NodeId)> = if self.selection.is_empty() {
            page.nodes
                .values()
                .take(100)
                .map(|node| (node.name.clone(), node.id))
                .collect()
        } else {
            self.selection
                .iter()
                .filter_map(|id| page.nodes.get(id).map(|node| (node.name.clone(), node.id)))
                .collect()
        };
        if targets.is_empty() {
            return Err(EditorError::InvalidArguments(
                "there are no layers to rename".to_owned(),
            ));
        }
        let names: Vec<String> = targets.iter().map(|(name, _)| name.clone()).collect();
        let mut engine = AiEngine::from_env();
        let mapping = engine.rename_layers(&names).map_err(ai_error)?;
        let mut applied = 0usize;
        for (old, new) in mapping {
            let new = new.trim().to_owned();
            if new.is_empty() || new.chars().count() > 200 {
                continue;
            }
            let Some((_, id)) = targets.iter().find(|(name, _)| name == &old) else {
                continue;
            };
            let patch = dp_model::NodePatch {
                name: Some(new),
                ..dp_model::NodePatch::default()
            };
            let transaction =
                dp_model::ops::patch_nodes(&self.document, &[*id], patch, "AI: rename layer", None)
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?;
            self.apply(transaction)?;
            applied += 1;
        }
        if applied == 0 {
            self.set_status("AI returned no rename suggestions");
        } else {
            self.set_status(format!("Renamed {applied} layer(s) with AI suggestions"));
        }
        Ok(applied)
    }

    /// Runs a prose AI task (no document changes).
    ///
    /// `task` is one of `improve`, `explain`, `content`, `style`, `comments`.
    pub fn ai_text_task(&mut self, task: &str, input: &str) -> Result<String, EditorError> {
        let mut engine = AiEngine::from_env();
        let output = match task {
            "improve" | "suggest" => engine.suggest_improvements(input),
            "explain" => engine.explain_component(input),
            "content" | "placeholder" => engine.placeholder_content(input),
            "style" | "palette" => engine.suggest_style(input),
            "comments" => {
                let comments: Vec<String> = input
                    .lines()
                    .map(|line| line.trim().to_owned())
                    .filter(|line| !line.is_empty())
                    .collect();
                engine.summarize_comments(&comments)
            }
            other => {
                return Err(EditorError::InvalidArguments(format!(
                    "unknown AI task '{other}' (expected improve, explain, content, style or comments)"
                )))
            }
        }
        .map_err(ai_error)?;
        let text = output.trim().to_owned();
        let shown: String = text.chars().take(2_000).collect();
        self.set_status(format!("AI ({task}): {shown}"));
        Ok(text)
    }

    // -----------------------------------------------------------------------
    // Plugins
    // -----------------------------------------------------------------------

    /// Installs a plugin from manifest JSON. Returns the plugin id.
    pub fn plugin_install_json(&mut self, manifest_json: &str) -> Result<String, EditorError> {
        let id = self
            .plugins
            .install_json(manifest_json)
            .map_err(registry_error)?;
        self.set_status(format!(
            "Installed plugin {id} (disabled until you enable it)"
        ));
        Ok(id)
    }

    /// Installs the bundled example plugin (proves the manifest/permission
    /// pipeline without shipping third-party code).
    pub fn plugin_install_example(&mut self) -> Result<String, EditorError> {
        let text = include_str!("../../dp-plugins/examples/hello-plugin/plugin.json");
        self.plugin_install_json(text)
    }

    /// Removes an installed plugin. Returns its id.
    pub fn plugin_remove(&mut self, plugin_id: &str) -> Result<String, EditorError> {
        let removed = self.plugins.remove(plugin_id).map_err(registry_error)?;
        self.set_status(format!("Removed plugin {}", removed.manifest.name));
        Ok(removed.manifest.id)
    }

    /// Enables or disables a plugin. Returns the new state.
    pub fn plugin_set_enabled(
        &mut self,
        plugin_id: &str,
        enabled: bool,
    ) -> Result<bool, EditorError> {
        if enabled {
            self.plugins.enable(plugin_id).map_err(registry_error)?;
        } else {
            self.plugins.disable(plugin_id).map_err(registry_error)?;
        }
        self.set_status(format!(
            "Plugin {plugin_id} {}",
            if enabled { "enabled" } else { "disabled" }
        ));
        Ok(enabled)
    }

    /// Updates one typed plugin setting (validated by the manifest).
    pub fn plugin_set_setting(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: Value,
    ) -> Result<(), EditorError> {
        self.plugins
            .set_setting(plugin_id, key, value)
            .map_err(registry_error)?;
        self.set_status(format!("Updated setting '{key}' for {plugin_id}"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_model::DocKind;

    fn editor() -> EditorState {
        EditorState::new_empty(DocKind::Design)
    }

    #[test]
    fn ai_layout_is_previewed_before_applying() {
        let mut state = editor();
        let steps = state
            .ai_generate_layout("login screen for a mobile app", 390, 844)
            .expect("offline layout");
        assert!(steps >= 3);
        assert!(state.last_ai_proposal.is_some());
        assert!(
            state.page().nodes.is_empty(),
            "preview must not mutate the document"
        );
        let applied = state.ai_apply_last_proposal(false).expect("apply");
        assert_eq!(applied, steps);
        assert!(state.page().nodes.len() >= 3);
        assert!(state.last_ai_proposal.is_none());
    }

    #[test]
    fn ai_apply_without_proposal_is_rejected() {
        let mut state = editor();
        assert!(matches!(
            state.ai_apply_last_proposal(false),
            Err(EditorError::InvalidArguments(_))
        ));
    }

    #[test]
    fn ai_discard_clears_pending_proposal() {
        let mut state = editor();
        state.ai_generate_layout("dashboard", 1440, 900).unwrap();
        assert!(state.ai_discard_proposal());
        assert!(state.last_ai_proposal.is_none());
        assert!(!state.ai_discard_proposal());
    }

    #[test]
    fn ai_text_task_uses_offline_provider() {
        let mut state = editor();
        let text = state.ai_text_task("explain", "frame Login").expect("text");
        assert!(!text.is_empty());
        assert!(state.status.contains("AI (explain)"));
        assert!(state.ai_text_task("unknown-task", "").is_err());
    }

    #[test]
    fn plugins_install_enable_and_remove() {
        let mut state = editor();
        let id = state.plugin_install_example().expect("install");
        assert_eq!(state.plugins.len(), 1);
        assert!(!state.plugins.plugins[&id].is_enabled());
        state.plugin_set_enabled(&id, true).unwrap();
        assert!(state.plugins.plugins[&id].is_enabled());
        state
            .plugin_set_setting(&id, "greeting", Value::String("Hi".to_owned()))
            .unwrap();
        state.plugin_remove(&id).unwrap();
        assert!(state.plugins.is_empty());
    }

    #[test]
    fn invalid_plugin_manifest_is_rejected() {
        let mut state = editor();
        assert!(state.plugin_install_json("{\"id\":\"Bad Id\"}").is_err());
        assert!(state.plugins.is_empty());
    }
}
