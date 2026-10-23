//! AI provider abstraction.
//!
//! The core editor never requires a paid model. The offline provider is a real,
//! deterministic provider that turns a prompt into structured design intents, so
//! every AI feature can be exercised and tested without a network call. A
//! remote provider is optional and configured through environment variables; the
//! provider only returns text — document changes are validated here and applied
//! as ordinary undoable transactions, never directly by the model.

use serde::{Deserialize, Serialize};

use crate::proposals::IntentOp;

/// A provider request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// System prompt that pins the contract.
    pub system: String,
    /// The user/task prompt.
    pub prompt: String,
    /// Whether the response must be JSON.
    pub expect_json: bool,
}

/// Provider response with the raw text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Completion {
    /// Model or provider label for logs.
    pub model: String,
    /// Raw text returned by the provider.
    pub text: String,
}

/// Provider failures (never panics; the UI shows these messages).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProviderError {
    /// Remote transport failed.
    #[error("provider request failed: {0}")]
    Transport(String),
    /// Provider returned no usable text.
    #[error("provider returned an empty response")]
    Empty,
    /// The optional HTTP feature is not compiled in.
    #[error("this build has no remote AI provider (enable the 'http' feature)")]
    NotBuilt,
    /// Missing configuration.
    #[error("AI provider is not configured: {0}")]
    NotConfigured(String),
}

/// A pluggable text provider.
pub trait AiProvider {
    /// Stable provider id for telemetry/status, e.g. "offline".
    fn id(&self) -> &'static str;
    /// Whether the provider can currently answer.
    fn is_available(&self) -> bool;
    /// Human label for the UI.
    fn label(&self) -> String;
    /// Runs a completion.
    fn complete(&mut self, request: &CompletionRequest) -> Result<Completion, ProviderError>;
}

/// The built-in deterministic provider.
///
/// It is not a stand-in mock: it implements the *same* contract the remote
/// provider does (system + prompt -> JSON intents) using a real prompt parser,
/// so layout generation, renaming and preview flows work offline.
#[derive(Debug, Clone, Default)]
pub struct OfflineProvider {
    /// Last prompt, kept so rename/explain prompts can be echoed deterministically.
    last_prompt: Option<String>,
}

impl OfflineProvider {
    /// New offline provider.
    pub fn new() -> OfflineProvider {
        OfflineProvider::default()
    }

    /// Parses a layout prompt into deterministic intents.
    pub fn layout_intents(prompt: &str) -> Vec<IntentOp> {
        let lower = prompt.to_ascii_lowercase();
        let mut ops = Vec::new();
        let seed = prompt.bytes().fold(17u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32));
        let accent = match seed % 3 {
            0 => "#ff6b4a",
            1 => "#4fd1c5",
            _ => "#9d8cff",
        };
        if lower.contains("dashboard") || lower.contains("admin") {
            let frame = "Dashboard";
            ops.push(IntentOp::AddFrame {
                name: frame.into(),
                x: 0.0,
                y: 0.0,
                width: 1440.0,
                height: 900.0,
                fill: Some("#f8fafc".into()),
            });
            ops.push(IntentOp::AddRectangle {
                frame: Some(frame.into()),
                name: "Sidebar".into(),
                x: 0.0,
                y: 0.0,
                width: 248.0,
                height: 900.0,
                fill: Some("#0b0d11".into()),
                radius: 0.0,
            });
            ops.push(IntentOp::AddText {
                frame: Some(frame.into()),
                name: "Title".into(),
                characters: "Overview".into(),
                x: 288.0,
                y: 32.0,
                font_size: 28.0,
                color: Some("#0b0d11".into()),
            });
            for (index, label) in ["Revenue", "Active users", "Conversion"].iter().enumerate() {
                let column = index as f64;
                ops.push(IntentOp::AddRectangle {
                    frame: Some(frame.into()),
                    name: format!("{label} card"),
                    x: 288.0 + column * 300.0,
                    y: 96.0,
                    width: 272.0,
                    height: 148.0,
                    fill: Some("#ffffff".into()),
                    radius: 12.0,
                });
                ops.push(IntentOp::AddText {
                    frame: Some(frame.into()),
                    name: format!("{label} label"),
                    characters: (*label).into(),
                    x: 312.0 + column * 300.0,
                    y: 120.0,
                    font_size: 13.0,
                    color: Some("#6f7887".into()),
                });
            }
            return ops;
        }
        if lower.contains("login") || lower.contains("sign in") || lower.contains("signin") {
            let frame = "Login";
            ops.push(IntentOp::AddFrame {
                name: frame.into(),
                x: 0.0,
                y: 0.0,
                width: 390.0,
                height: 844.0,
                fill: Some("#ffffff".into()),
            });
            ops.push(IntentOp::AddText {
                frame: Some(frame.into()),
                name: "Heading".into(),
                characters: "Welcome back".into(),
                x: 24.0,
                y: 120.0,
                font_size: 28.0,
                color: Some("#0b0d11".into()),
            });
            ops.push(IntentOp::AddText {
                frame: Some(frame.into()),
                name: "Subtitle".into(),
                characters: "Sign in to continue".into(),
                x: 24.0,
                y: 164.0,
                font_size: 15.0,
                color: Some("#6f7887".into()),
            });
            ops.push(IntentOp::AddRectangle {
                frame: Some(frame.into()),
                name: "Email field".into(),
                x: 24.0,
                y: 240.0,
                width: 342.0,
                height: 48.0,
                fill: Some("#f1f3f7".into()),
                radius: 8.0,
            });
            ops.push(IntentOp::AddRectangle {
                frame: Some(frame.into()),
                name: "Password field".into(),
                x: 24.0,
                y: 300.0,
                width: 342.0,
                height: 48.0,
                fill: Some("#f1f3f7".into()),
                radius: 8.0,
            });
            ops.push(IntentOp::AddRectangle {
                frame: Some(frame.into()),
                name: "Sign in button".into(),
                x: 24.0,
                y: 372.0,
                width: 342.0,
                height: 48.0,
                fill: Some(accent.into()),
                radius: 8.0,
            });
            ops.push(IntentOp::AddText {
                frame: Some(frame.into()),
                name: "Button label".into(),
                characters: "Sign in".into(),
                x: 24.0,
                y: 386.0,
                font_size: 15.0,
                color: Some("#ffffff".into()),
            });
            return ops;
        }
        // Generic mobile screen.
        let frame = "Screen";
        ops.push(IntentOp::AddFrame {
            name: frame.into(),
            x: 0.0,
            y: 0.0,
            width: 390.0,
            height: 844.0,
            fill: Some("#ffffff".into()),
        });
        ops.push(IntentOp::AddRectangle {
            frame: Some(frame.into()),
            name: "Header".into(),
            x: 0.0,
            y: 0.0,
            width: 390.0,
            height: 96.0,
            fill: Some("#0b0d11".into()),
            radius: 0.0,
        });
        ops.push(IntentOp::AddText {
            frame: Some(frame.into()),
            name: "Header title".into(),
            characters: title_case(prompt),
            x: 24.0,
            y: 48.0,
            font_size: 20.0,
            color: Some("#ffffff".into()),
        });
        ops.push(IntentOp::AddRectangle {
            frame: Some(frame.into()),
            name: "Primary button".into(),
            x: 24.0,
            y: 760.0,
            width: 342.0,
            height: 48.0,
            fill: Some(accent.into()),
            radius: 8.0,
        });
        ops.push(IntentOp::AddText {
            frame: Some(frame.into()),
            name: "Button label".into(),
            characters: "Continue".into(),
            x: 24.0,
            y: 774.0,
            font_size: 15.0,
            color: Some("#ffffff".into()),
        });
        ops
    }
}

fn title_case(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return "Untitled screen".to_owned();
    }
    let mut characters = trimmed.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => trimmed.to_owned(),
    }
}

impl AiProvider for OfflineProvider {
    fn id(&self) -> &'static str {
        "offline"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn label(&self) -> String {
        "Offline (built-in)".to_owned()
    }

    fn complete(&mut self, request: &CompletionRequest) -> Result<Completion, ProviderError> {
        self.last_prompt = Some(request.prompt.clone());
        if !request.expect_json {
            return Ok(Completion {
                model: "offline-rules-v1".to_owned(),
                text: format!(
                    "Offline analysis: the prompt {} describes a layout; the built-in provider only produces structured intents, so connect a model provider for prose answers.",
                    if request.prompt.trim().is_empty() { "is empty" } else { "received" }
                ),
            });
        }
        let prompt = request.prompt.to_ascii_lowercase();
        let text = if prompt.starts_with("rename") {
            // Rename requests expect a JSON object mapping names to names.
            "{}".to_owned()
        } else {
            let intents = OfflineProvider::layout_intents(&request.prompt);
            serde_json::to_string(&intents).map_err(|error| ProviderError::Transport(error.to_string()))?
        };
        Ok(Completion {
            model: "offline-rules-v1".to_owned(),
            text,
        })
    }
}

/// A provider backed by an OpenAI-compatible chat completions endpoint.
///
/// Configured purely from environment variables (never hardcoded):
/// `ICONCRAFTER_AI_ENDPOINT`, `ICONCRAFTER_AI_MODEL`, `ICONCRAFTER_AI_KEY`.
/// The HTTP transport is compiled only with the `http` feature; without it the
/// constructor returns [`ProviderError::NotBuilt`] and the editor still works
/// with the offline provider.
#[cfg(feature = "http")]
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    endpoint: String,
    model: String,
    api_key: String,
}

#[cfg(feature = "http")]
impl OpenAiCompatibleProvider {
    /// Reads configuration from the environment.
    pub fn from_env() -> Result<OpenAiCompatibleProvider, ProviderError> {
        let endpoint = std::env::var("ICONCRAFTER_AI_ENDPOINT")
            .map_err(|_| ProviderError::NotConfigured("ICONCRAFTER_AI_ENDPOINT is not set".to_owned()))?;
        let model = std::env::var("ICONCRAFTER_AI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_owned());
        let api_key = std::env::var("ICONCRAFTER_AI_KEY")
            .map_err(|_| ProviderError::NotConfigured("ICONCRAFTER_AI_KEY is not set".to_owned()))?;
        if endpoint.trim().is_empty() {
            return Err(ProviderError::NotConfigured("empty endpoint".to_owned()));
        }
        Ok(OpenAiCompatibleProvider {
            endpoint,
            model,
            api_key,
        })
    }
}

#[cfg(feature = "http")]
impl AiProvider for OpenAiCompatibleProvider {
    fn id(&self) -> &'static str {
        "openai-compatible"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn label(&self) -> String {
        format!("Remote ({} @ {})", self.model, self.endpoint)
    }

    fn complete(&mut self, request: &CompletionRequest) -> Result<Completion, ProviderError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": request.system},
                {"role": "user", "content": request.prompt}
            ],
            "temperature": if request.expect_json { 0.2 } else { 0.7 },
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;
        let response = client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|error| ProviderError::Transport(error.to_string()))?;
        let status = response.status();
        let payload: serde_json::Value = response
            .json()
            .map_err(|error| ProviderError::Transport(format!("HTTP {status}: {error}")))?;
        if !status.is_success() {
            let message = payload
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|message| message.as_str())
                .unwrap_or("provider rejected the request");
            return Err(ProviderError::Transport(format!("HTTP {status}: {message}")));
        }
        let text = payload
            .get("choices")
            .and_then(|choices| choices.get(0))
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_str())
            .unwrap_or_default()
            .to_owned();
        if text.trim().is_empty() {
            return Err(ProviderError::Empty);
        }
        Ok(Completion {
            model: self.model.clone(),
            text,
        })
    }
}

/// Creates the provider configured in the environment, falling back to offline.
pub fn provider_from_env() -> Box<dyn AiProvider> {
    #[cfg(feature = "http")]
    {
        if std::env::var("ICONCRAFTER_AI_ENDPOINT").is_ok() && std::env::var("ICONCRAFTER_AI_KEY").is_ok() {
            if let Ok(provider) = OpenAiCompatibleProvider::from_env() {
                return Box::new(provider);
            }
        }
    }
    Box::new(OfflineProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposals::IntentOp;

    fn request(prompt: &str, json: bool) -> CompletionRequest {
        CompletionRequest {
            system: "system".to_owned(),
            prompt: prompt.to_owned(),
            expect_json: json,
        }
    }

    #[test]
    fn offline_produces_valid_json_intents() {
        let mut provider = OfflineProvider::new();
        let completion = provider.complete(&request("Design a login screen", true)).unwrap();
        let intents: Vec<IntentOp> = serde_json::from_str(&completion.text).expect("valid intent json");
        assert!(!intents.is_empty());
        assert!(intents.iter().any(|op| matches!(op, IntentOp::AddFrame { .. })));
    }

    #[test]
    fn offline_is_deterministic() {
        let mut provider = OfflineProvider::new();
        let first = provider.complete(&request("dashboard for analysts", true)).unwrap();
        let second = provider.complete(&request("dashboard for analysts", true)).unwrap();
        assert_eq!(first.text, second.text);
    }

    #[test]
    fn offline_variants_cover_dashboard_login_and_generic() {
        let dashboard = OfflineProvider::layout_intents("admin dashboard");
        assert!(dashboard.iter().any(|op| matches!(op, IntentOp::AddRectangle { name, .. } if name == "Sidebar")));
        let login = OfflineProvider::layout_intents("login screen for a mobile app");
        assert!(login.iter().any(|op| matches!(op, IntentOp::AddRectangle { name, .. } if name == "Sign in button")));
        let generic = OfflineProvider::layout_intents("a screen");
        assert!(generic.iter().any(|op| matches!(op, IntentOp::AddFrame { name, .. } if name == "Screen")));
    }

    #[test]
    fn offline_answers_prose_requests_without_faking_model_output() {
        let mut provider = OfflineProvider::new();
        let completion = provider.complete(&request("Explain this component", false)).unwrap();
        assert!(completion.text.contains("Offline analysis"));
        assert_eq!(provider.label(), "Offline (built-in)");
        assert!(provider.is_available());
    }
}
