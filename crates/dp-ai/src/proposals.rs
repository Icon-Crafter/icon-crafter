//! Structured design intents proposed by an AI provider and validated here.
//!
//! The model never touches [`dp_model::Document`]. It returns JSON intents; this
//! module validates them against the document, builds ordinary
//! [`dp_model::Transaction`]s and applies them through the shared undoable
//! transaction path. Destructive intents require explicit confirmation.

use std::collections::HashMap;

use dp_model::{
    ops, Color, Document, Fill, History, Node, NodeId, NodeKind, NodePatch, PageId, Transaction,
    Transform,
};
use serde::{Deserialize, Serialize};

/// Maximum intents accepted from one proposal.
pub const MAX_INTENTS: usize = 256;
/// Maximum node dimension accepted from a proposal.
pub const MAX_DIMENSION: f64 = 20_000.0;

/// One structured design intent (the AI contract).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum IntentOp {
    /// Create a frame (artboard).
    AddFrame {
        /// Layer name.
        name: String,
        /// X in page coordinates.
        x: f64,
        /// Y in page coordinates.
        y: f64,
        /// Width.
        width: f64,
        /// Height.
        height: f64,
        /// Optional hex fill.
        #[serde(default)]
        fill: Option<String>,
    },
    /// Create a rectangle inside a frame created earlier in the same proposal.
    AddRectangle {
        /// Frame name from a preceding `add_frame` intent (None = page root).
        #[serde(default)]
        frame: Option<String>,
        /// Layer name.
        name: String,
        /// X relative to the frame.
        x: f64,
        /// Y relative to the frame.
        y: f64,
        /// Width.
        width: f64,
        /// Height.
        height: f64,
        /// Optional hex fill.
        #[serde(default)]
        fill: Option<String>,
        /// Corner radius.
        #[serde(default)]
        radius: f64,
    },
    /// Create a text layer inside a frame created earlier in the same proposal.
    AddText {
        /// Frame name from a preceding `add_frame` intent (None = page root).
        #[serde(default)]
        frame: Option<String>,
        /// Layer name.
        name: String,
        /// Text content.
        characters: String,
        /// X relative to the frame.
        x: f64,
        /// Y relative to the frame.
        y: f64,
        /// Font size.
        #[serde(default = "default_font_size")]
        font_size: f64,
        /// Optional hex colour.
        #[serde(default)]
        color: Option<String>,
    },
    /// Replace the fill of an existing node.
    SetFill {
        /// Target node.
        node: String,
        /// Hex colour.
        color: String,
    },
    /// Rename an existing node.
    RenameLayer {
        /// Target node.
        node: String,
        /// New name.
        name: String,
    },
    /// Delete an existing node (destructive: needs confirmation).
    DeleteNode {
        /// Target node.
        node: String,
    },
}

fn default_font_size() -> f64 {
    16.0
}

impl IntentOp {
    /// Human summary used in the proposal preview.
    pub fn summary(&self) -> String {
        match self {
            IntentOp::AddFrame { name, .. } => format!("Frame “{name}”"),
            IntentOp::AddRectangle { name, frame, .. } => match frame {
                Some(frame) => format!("Rectangle “{name}” in “{frame}”"),
                None => format!("Rectangle “{name}”"),
            },
            IntentOp::AddText { name, frame, .. } => match frame {
                Some(frame) => format!("Text “{name}” in “{frame}”"),
                None => format!("Text “{name}”"),
            },
            IntentOp::SetFill { node, color } => format!("Set fill of {node} to {color}"),
            IntentOp::RenameLayer { node, name } => format!("Rename {node} to “{name}”"),
            IntentOp::DeleteNode { node } => format!("Delete {node}"),
        }
    }

    /// Whether applying this intent destroys content.
    pub fn is_destructive(&self) -> bool {
        matches!(self, IntentOp::DeleteNode { .. })
    }
}

/// A validated, executable proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// Provider label/model that produced the intents.
    pub source: String,
    /// One-line summary per intent, in order.
    pub summary: Vec<String>,
    /// Sequential undoable transactions (applied in order, each undoable).
    pub steps: Vec<Transaction>,
    /// Non-fatal notes (clamped dimensions, ignored unknown frames, ...).
    pub warnings: Vec<String>,
    /// Whether confirmation is required before applying.
    pub requires_confirmation: bool,
    /// Whether the document changed when the proposal was built (never — building
    /// happens against a clone; this exists for tests/UI clarity).
    pub destructive: bool,
}

/// AI subsystem errors.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AiError {
    /// Provider failure.
    #[error("AI provider error: {0}")]
    Provider(String),
    /// Model returned text that is not the intent schema.
    #[error("AI returned invalid intents: {0}")]
    InvalidResponse(String),
    /// An intent referenced something that does not exist.
    #[error("AI intent is not applicable: {0}")]
    InvalidIntent(String),
    /// Too many intents.
    #[error("AI returned more than {MAX_INTENTS} intents")]
    TooManyIntents,
    /// Destructive change not confirmed.
    #[error("this change deletes content; confirmation is required")]
    ConfirmationRequired,
    /// Document/transaction error.
    #[error("document error: {0}")]
    Model(String),
}

impl From<dp_model::ModelError> for AiError {
    fn from(error: dp_model::ModelError) -> AiError {
        AiError::Model(error.to_string())
    }
}

/// Parses the raw model text into intents and validates them against the
/// document, producing undoable transactions for a preview.
pub fn propose(
    document: &Document,
    source: &str,
    intents: &[IntentOp],
    page: Option<PageId>,
) -> Result<Proposal, AiError> {
    if intents.len() > MAX_INTENTS {
        return Err(AiError::TooManyIntents);
    }
    let page_id = page.unwrap_or_else(|| document.active_page_id());
    if document.page(page_id).is_none() {
        return Err(AiError::InvalidIntent(format!("unknown page {page_id}")));
    }
    let mut working = document.clone();
    let mut scratch = History::new();
    let mut steps = Vec::new();
    let mut summary = Vec::new();
    let mut warnings = Vec::new();
    let mut frames: HashMap<String, NodeId> = HashMap::new();
    let mut destructive = false;

    for intent in intents {
        summary.push(intent.summary());
        if intent.is_destructive() {
            destructive = true;
        }
        let transaction = build_step(&working, page_id, intent, &mut frames, &mut warnings)?;
        match transaction {
            Some(transaction) => {
                ops::apply_transaction(&mut working, &mut scratch, transaction.clone())?;
                steps.push(transaction);
            }
            None => warnings.push(format!("skipped: {}", intent.summary())),
        }
    }
    if steps.is_empty() {
        return Err(AiError::InvalidIntent(
            "the proposal contains no applicable operations".to_owned(),
        ));
    }
    Ok(Proposal {
        source: source.to_owned(),
        summary,
        steps,
        warnings,
        requires_confirmation: destructive,
        destructive,
    })
}

fn build_step(
    document: &Document,
    page: PageId,
    intent: &IntentOp,
    frames: &mut HashMap<String, NodeId>,
    warnings: &mut Vec<String>,
) -> Result<Option<Transaction>, AiError> {
    match intent {
        IntentOp::AddFrame {
            name,
            x,
            y,
            width,
            height,
            fill,
        } => {
            check_dimensions(*width, *height, warnings);
            let mut node = Node::frame(name, *x, *y, width.clamp(1.0, MAX_DIMENSION), height.clamp(1.0, MAX_DIMENSION));
            if let Some(fill) = fill {
                node.style.fills = vec![Fill::solid(parse_color(fill)?)];
            }
            frames.insert(name.clone(), node.id);
            Ok(Some(ops::insert_nodes(
                document,
                page,
                vec![(node, None, None)],
                "AI: add frame",
            )))
        }
        IntentOp::AddRectangle {
            frame,
            name,
            x,
            y,
            width,
            height,
            fill,
            radius,
        } => {
            check_dimensions(*width, *height, warnings);
            let parent = resolve_frame(document, frame, frames)?;
            let mut node = Node::new(
                NodeKind::Rectangle,
                name,
                Transform::new(*x, *y, width.clamp(1.0, MAX_DIMENSION), height.clamp(1.0, MAX_DIMENSION)),
            );
            if let Some(fill) = fill {
                node.style.fills = vec![Fill::solid(parse_color(fill)?)];
            }
            if *radius > 0.0 {
                let radius = radius.clamp(0.0, width.min(*height) / 2.0);
                node.style.corner_radii = dp_geom::CornerRadii::uniform(radius);
            }
            Ok(Some(ops::insert_nodes(
                document,
                page,
                vec![(node, parent, None)],
                "AI: add rectangle",
            )))
        }
        IntentOp::AddText {
            frame,
            name,
            characters,
            x,
            y,
            font_size,
            color,
        } => {
            if characters.chars().count() > 10_000 {
                return Err(AiError::InvalidIntent("text is longer than 10,000 characters".to_owned()));
            }
            let parent = resolve_frame(document, frame, frames)?;
            let mut node = Node::text(name, characters, *x, *y);
            if let Some(content) = node.text.as_mut() {
                content.style.font_size = font_size.clamp(6.0, 400.0);
                if let Some(color) = color {
                    content.style.fills = vec![Fill::solid(parse_color(color)?)];
                }
            }
            Ok(Some(ops::insert_nodes(
                document,
                page,
                vec![(node, parent, None)],
                "AI: add text",
            )))
        }
        IntentOp::SetFill { node, color } => {
            let node_id = parse_node(document, node)?;
            let mut style = document
                .node(node_id)
                .map(|node| node.style.clone())
                .ok_or_else(|| AiError::InvalidIntent("unknown node".to_owned()))?;
            style.fills = vec![Fill::solid(parse_color(color)?)];
            let patch = NodePatch {
                style: Some(style),
                ..NodePatch::default()
            };
            Ok(Some(ops::patch_nodes(
                document,
                &[node_id],
                patch,
                "AI: set fill",
                None,
            )?))
        }
        IntentOp::RenameLayer { node, name } => {
            let node_id = parse_node(document, node)?;
            let patch = NodePatch {
                name: Some(name.clone()),
                ..NodePatch::default()
            };
            Ok(Some(ops::patch_nodes(
                document,
                &[node_id],
                patch,
                "AI: rename layer",
                None,
            )?))
        }
        IntentOp::DeleteNode { node } => {
            let node_id = parse_node(document, node)?;
            Ok(Some(ops::delete_nodes(document, &[node_id], "AI: delete node")?))
        }
    }
}

/// Applies a proposal after optional confirmation.
///
/// Each step is pushed through the normal [`ops::apply_transaction`] path, so
/// every AI change is undoable with the rest of the history.
pub fn apply(
    document: &mut Document,
    history: &mut History,
    proposal: &Proposal,
    confirmed: bool,
) -> Result<usize, AiError> {
    if proposal.requires_confirmation && !confirmed {
        return Err(AiError::ConfirmationRequired);
    }
    let mut applied = 0;
    for transaction in &proposal.steps {
        ops::apply_transaction(document, history, transaction.clone())?;
        applied += 1;
    }
    Ok(applied)
}

fn resolve_frame(
    document: &Document,
    frame: &Option<String>,
    frames: &HashMap<String, NodeId>,
) -> Result<Option<NodeId>, AiError> {
    match frame {
        None => Ok(None),
        Some(name) => match frames.get(name) {
            Some(id) => Ok(Some(*id)),
            None => {
                // A frame that already exists in the document may be referenced by name.
                let page = document.active_page();
                Ok(page
                    .nodes
                    .values()
                    .find(|node| node.kind == NodeKind::Frame && &node.name == name)
                    .map(|node| node.id))
            }
        },
    }
}

fn parse_node(document: &Document, raw: &str) -> Result<NodeId, AiError> {
    let uuid = uuid::Uuid::parse_str(raw)
        .map_err(|_| AiError::InvalidIntent(format!("'{raw}' is not a node id")))?;
    let id = NodeId::from_uuid(uuid);
    document
        .node(id)
        .map(|_| id)
        .ok_or_else(|| AiError::InvalidIntent(format!("node {id} does not exist")))
}

fn parse_color(raw: &str) -> Result<Color, AiError> {
    Color::from_hex(raw).ok_or_else(|| AiError::InvalidIntent(format!("invalid colour '{raw}'")))
}

fn check_dimensions(width: f64, height: f64, warnings: &mut Vec<String>) {
    if !width.is_finite() || !height.is_finite() {
        warnings.push("non-finite dimension was clamped".to_owned());
    } else if width > MAX_DIMENSION || height > MAX_DIMENSION {
        warnings.push(format!(
            "dimension was clamped to {MAX_DIMENSION}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_model::{ModelError, NodeKind};

    fn document() -> Document {
        Document::new("AI")
    }

    fn login_intents() -> Vec<IntentOp> {
        vec![
            IntentOp::AddFrame {
                name: "Login".into(),
                x: 0.0,
                y: 0.0,
                width: 390.0,
                height: 844.0,
                fill: Some("#ffffff".into()),
            },
            IntentOp::AddRectangle {
                frame: Some("Login".into()),
                name: "Button".into(),
                x: 24.0,
                y: 700.0,
                width: 342.0,
                height: 48.0,
                fill: Some("#ff6b4a".into()),
                radius: 8.0,
            },
            IntentOp::AddText {
                frame: Some("Login".into()),
                name: "Label".into(),
                characters: "Sign in".into(),
                x: 24.0,
                y: 714.0,
                font_size: 15.0,
                color: Some("#ffffff".into()),
            },
        ]
    }

    #[test]
    fn proposal_builds_valid_transactions_without_touching_document() {
        let document = document();
        let proposal = propose(&document, "offline", &login_intents(), None).unwrap();
        assert_eq!(proposal.steps.len(), 3);
        assert!(!proposal.requires_confirmation);
        assert!(document.active_page().nodes.is_empty(), "proposal building must be side-effect free");
    }

    #[test]
    fn applying_is_undoable() {
        let mut document = document();
        let mut history = History::new();
        let proposal = propose(&document, "offline", &login_intents(), None).unwrap();
        let applied = apply(&mut document, &mut history, &proposal, false).unwrap();
        assert_eq!(applied, 3);
        assert_eq!(document.active_page().nodes.len(), 3);
        assert_eq!(history.undo_len(), 3);
        for _ in 0..3 {
            history.undo(&mut document).unwrap();
        }
        assert_eq!(document.active_page().nodes.len(), 0);
    }

    #[test]
    fn destructive_requires_confirmation() {
        let mut document = document();
        let mut history = History::new();
        // Create a rectangle first so there is something to delete.
        let setup = propose(&document, "offline", &login_intents(), None).unwrap();
        apply(&mut document, &mut history, &setup, false).unwrap();
        let button = document
            .active_page()
            .nodes
            .values()
            .find(|node| node.name == "Button")
            .unwrap()
            .id;
        let delete = vec![IntentOp::DeleteNode {
            node: button.to_string(),
        }];
        let proposal = propose(&document, "offline", &delete, None).unwrap();
        assert!(proposal.requires_confirmation);
        assert!(matches!(
            apply(&mut document, &mut history, &proposal, false),
            Err(AiError::ConfirmationRequired)
        ));
        apply(&mut document, &mut history, &proposal, true).unwrap();
        assert!(document.node(button).is_none());
    }

    #[test]
    fn invalid_intents_are_rejected_with_reasons() {
        let document = document();
        let bad = vec![IntentOp::AddRectangle {
            frame: Some("Missing".into()),
            name: "X".into(),
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            fill: Some("not-a-colour".into()),
            radius: 0.0,
        }];
        assert!(matches!(
            propose(&document, "offline", &bad, None),
            Err(AiError::InvalidIntent(_))
        ));
        let unknown_node = vec![IntentOp::SetFill {
            node: "not-a-uuid".into(),
            color: "#ffffff".into(),
        }];
        assert!(matches!(
            propose(&document, "offline", &unknown_node, None),
            Err(AiError::InvalidIntent(_))
        ));
    }

    #[test]
    fn existing_frames_can_be_referenced_by_name() {
        let mut document = document();
        let mut history = History::new();
        let setup = propose(&document, "offline", &login_intents(), None).unwrap();
        apply(&mut document, &mut history, &setup, false).unwrap();
        let frame_id = document
            .active_page()
            .nodes
            .values()
            .find(|node| node.kind == NodeKind::Frame)
            .unwrap()
            .id;
        let extra = vec![IntentOp::AddText {
            frame: Some("Login".into()),
            name: "Footnote".into(),
            characters: "Terms".into(),
            x: 24.0,
            y: 800.0,
            font_size: 12.0,
            color: None,
        }];
        let proposal = propose(&document, "offline", &extra, None).unwrap();
        apply(&mut document, &mut history, &proposal, false).unwrap();
        let footnote = document
            .active_page()
            .nodes
            .values()
            .find(|node| node.name == "Footnote")
            .unwrap();
        assert_eq!(footnote.parent, Some(frame_id));
    }

    #[test]
    fn non_finite_and_huge_dimensions_are_clamped_with_warnings() {
        let document = document();
        let intents = vec![IntentOp::AddFrame {
            name: "Huge".into(),
            x: 0.0,
            y: 0.0,
            width: 1.0e9,
            height: f64::NAN,
            fill: None,
        }];
        let proposal = propose(&document, "offline", &intents, None).unwrap();
        assert!(!proposal.warnings.is_empty());
    }

    #[test]
    fn model_error_conversion_is_honest() {
        let error = AiError::from(ModelError::InvalidDocument("x".to_owned()));
        assert!(error.to_string().contains("document error"));
    }
}
