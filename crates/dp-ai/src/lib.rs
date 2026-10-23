//! Optional AI subsystem for Icon Crafter.
//!
//! The AI never edits the document directly: it proposes structured document
//! intents that are validated against the live document and turned into the same
//! undoable [`dp_model::Transaction`]s manual editing uses. The built-in offline
//! provider makes the feature fully usable (and testable) without a model API;
//! a remote OpenAI-compatible provider is optional, configured only through
//! environment variables, and never required by the editor.
//!
//! ## Applying AI changes
//!
//! ```no_run
//! # use dp_ai::{AiEngine, IntentOp};
//! # use dp_model::Document;
//! let mut document = Document::new("Demo");
//! let mut ai = AiEngine::offline();
//! let proposal = ai.generate_layout(&document, "login screen", 390, 844).unwrap();
//! // The UI shows proposal.summary / proposal.warnings first.
//! let mut history = dp_model::History::new();
//! ai.apply(&mut document, &mut history, &proposal, false).unwrap();
//! ```
//!
//! Destructive proposals report `requires_confirmation == true` and
//! [`AiEngine::apply`] refuses them until the caller confirms.

pub mod prompts;
pub mod proposals;
pub mod provider;

pub use prompts::{
    INTENT_SCHEMA, SYSTEM_ASSISTANT, SYSTEM_LAYOUT, comments_prompt, content_prompt,
    explain_prompt, layout_prompt, rename_prompt, review_prompt, style_prompt,
};
pub use proposals::{AiError, IntentOp, MAX_DIMENSION, MAX_INTENTS, Proposal, apply, propose};
pub use provider::{
    AiProvider, Completion, CompletionRequest, OfflineProvider, ProviderError, provider_from_env,
};

#[cfg(feature = "http")]
pub use provider::OpenAiCompatibleProvider;

use dp_model::{Document, History, PageId};

/// The AI engine: one provider plus the shared validation/apply pipeline.
pub struct AiEngine {
    provider: Box<dyn AiProvider>,
}

impl AiEngine {
    /// Engine with a custom provider.
    pub fn new(provider: Box<dyn AiProvider>) -> AiEngine {
        AiEngine { provider }
    }

    /// Engine using the built-in deterministic provider.
    pub fn offline() -> AiEngine {
        AiEngine::new(Box::new(OfflineProvider::new()))
    }

    /// Engine using the provider configured in the environment (falls back to
    /// offline when no remote endpoint/key is configured).
    pub fn from_env() -> AiEngine {
        AiEngine::new(provider_from_env())
    }

    /// Provider id (e.g. "offline").
    pub fn provider_id(&self) -> &'static str {
        self.provider.id()
    }

    /// Human label for the UI.
    pub fn provider_label(&self) -> String {
        self.provider.label()
    }

    /// Whether the provider can answer right now.
    pub fn is_available(&self) -> bool {
        self.provider.is_available()
    }

    fn ask_json(&mut self, system: &str, prompt: &str) -> Result<Vec<IntentOp>, AiError> {
        let completion = self
            .provider
            .complete(&CompletionRequest {
                system: system.to_owned(),
                prompt: prompt.to_owned(),
                expect_json: true,
            })
            .map_err(|error| AiError::Provider(error.to_string()))?;
        let text = strip_code_fence(&completion.text);
        serde_json::from_str::<Vec<IntentOp>>(&text)
            .map_err(|error| AiError::InvalidResponse(error.to_string()))
    }

    fn ask_text(&mut self, system: &str, prompt: &str) -> Result<String, AiError> {
        let completion = self
            .provider
            .complete(&CompletionRequest {
                system: system.to_owned(),
                prompt: prompt.to_owned(),
                expect_json: false,
            })
            .map_err(|error| AiError::Provider(error.to_string()))?;
        Ok(completion.text)
    }

    /// Generates a starting layout from a text prompt.
    pub fn generate_layout(
        &mut self,
        document: &Document,
        description: &str,
        width: u32,
        height: u32,
    ) -> Result<Proposal, AiError> {
        if description.trim().is_empty() {
            return Err(AiError::InvalidIntent("prompt is empty".to_owned()));
        }
        let prompt = layout_prompt(description, width, height);
        let intents = self.ask_json(SYSTEM_LAYOUT, &prompt)?;
        propose(document, self.provider_id(), &intents, None)
    }

    /// Proposes layout intents for a specific page.
    pub fn generate_layout_on(
        &mut self,
        document: &Document,
        page: PageId,
        description: &str,
        width: u32,
        height: u32,
    ) -> Result<Proposal, AiError> {
        let prompt = layout_prompt(description, width, height);
        let intents = self.ask_json(SYSTEM_LAYOUT, &prompt)?;
        propose(document, self.provider_id(), &intents, Some(page))
    }

    /// Renames layers using the model, returning a mapping of old -> new.
    ///
    /// The offline provider returns an empty mapping (it does not invent names),
    /// which callers must present honestly as "no suggestions".
    pub fn rename_layers(&mut self, layers: &[String]) -> Result<Vec<(String, String)>, AiError> {
        let prompt = rename_prompt(layers);
        let completion = self
            .provider
            .complete(&CompletionRequest {
                system: SYSTEM_ASSISTANT.to_owned(),
                prompt,
                expect_json: true,
            })
            .map_err(|error| AiError::Provider(error.to_string()))?;
        let text = strip_code_fence(&completion.text);
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mapping: std::collections::BTreeMap<String, String> = serde_json::from_str(&text)
            .map_err(|error| AiError::InvalidResponse(error.to_string()))?;
        Ok(mapping.into_iter().collect())
    }

    /// Suggests design improvements (prose, never mutates the document).
    pub fn suggest_improvements(&mut self, summary: &str) -> Result<String, AiError> {
        self.ask_text(SYSTEM_ASSISTANT, &review_prompt(summary))
    }

    /// Explains a component to a developer.
    pub fn explain_component(&mut self, summary: &str) -> Result<String, AiError> {
        self.ask_text(SYSTEM_ASSISTANT, &explain_prompt(summary))
    }

    /// Generates placeholder copy.
    pub fn placeholder_content(&mut self, topic: &str) -> Result<String, AiError> {
        self.ask_text(SYSTEM_ASSISTANT, &content_prompt(topic))
    }

    /// Suggests a palette/type scale.
    pub fn suggest_style(&mut self, brand: &str) -> Result<String, AiError> {
        self.ask_text(SYSTEM_ASSISTANT, &style_prompt(brand))
    }

    /// Summarises design feedback comments.
    pub fn summarize_comments(&mut self, comments: &[String]) -> Result<String, AiError> {
        self.ask_text(SYSTEM_ASSISTANT, &comments_prompt(comments))
    }

    /// Parses raw intent JSON (used by the IPC layer for provider-agnostic
    /// pipelines and tests).
    pub fn parse_intents(&self, raw: &str) -> Result<Vec<IntentOp>, AiError> {
        let text = strip_code_fence(raw);
        serde_json::from_str(text.as_str())
            .map_err(|error| AiError::InvalidResponse(error.to_string()))
    }

    /// Validates intents and builds a proposal without touching the document.
    pub fn propose_intents(
        &self,
        document: &Document,
        intents: &[IntentOp],
    ) -> Result<Proposal, AiError> {
        propose(document, self.provider_id(), intents, None)
    }

    /// Applies a proposal (destructive ones require `confirmed = true`).
    pub fn apply(
        &self,
        document: &mut Document,
        history: &mut History,
        proposal: &Proposal,
        confirmed: bool,
    ) -> Result<usize, AiError> {
        apply(document, history, proposal, confirmed)
    }
}

/// Strips ``` fences that models often wrap JSON in.
fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        let without_open = trimmed.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
        if let Some((body, _)) = without_open.rsplit_once("```") {
            return body.trim().to_owned();
        }
        return without_open.trim_end_matches("```").trim().to_owned();
    }
    trimmed.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Document {
        Document::new("AI Engine")
    }

    #[test]
    fn offline_engine_generates_and_applies_layout() {
        let mut engine = AiEngine::offline();
        assert_eq!(engine.provider_id(), "offline");
        assert!(engine.is_available());
        let document = document();
        let proposal = engine
            .generate_layout(&document, "login screen", 390, 844)
            .expect("offline layout");
        assert!(!proposal.steps.is_empty());
        assert!(proposal.summary.len() >= proposal.steps.len());
        assert!(!proposal.requires_confirmation);

        let mut document = document;
        let mut history = History::new();
        engine
            .apply(&mut document, &mut history, &proposal, false)
            .unwrap();
        assert!(document.active_page().nodes.len() >= 3);
    }

    #[test]
    fn parse_intents_strips_fences() {
        let engine = AiEngine::offline();
        let raw = "```json\n[{\"op\":\"add_frame\",\"name\":\"F\",\"x\":0,\"y\":0,\"width\":10,\"height\":10}]\n```";
        let intents = engine.parse_intents(raw).unwrap();
        assert_eq!(intents.len(), 1);
    }

    #[test]
    fn empty_prompt_is_rejected() {
        let mut engine = AiEngine::offline();
        let document = document();
        assert!(matches!(
            engine.generate_layout(&document, "   ", 390, 844),
            Err(AiError::InvalidIntent(_))
        ));
    }

    #[test]
    fn prose_helpers_are_honest_offline() {
        let mut engine = AiEngine::offline();
        let improvement = engine.suggest_improvements("frame Login").unwrap();
        assert!(improvement.contains("Offline analysis"));
        let explanation = engine.explain_component("frame Login").unwrap();
        assert!(!explanation.is_empty());
        let renames = engine.rename_layers(&["Rectangle 1".to_owned()]).unwrap();
        assert!(renames.is_empty());
        let summary = engine.summarize_comments(&["bigger".to_owned()]).unwrap();
        assert!(!summary.is_empty());
    }

    #[test]
    fn invalid_model_json_is_reported() {
        let engine = AiEngine::offline();
        let error = engine.parse_intents("not json").unwrap_err();
        assert!(matches!(error, AiError::InvalidResponse(_)));
    }
}
