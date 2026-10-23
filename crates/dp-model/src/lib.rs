//! Design document model for Icon Crafter.
//!
//! `dp-model` owns the single source of truth for a design file: the page and
//! node tree, paint and effect styles, components and variables, prototype
//! interactions, transactions and undo history. Everything else in the
//! workspace (rendering, layout, collaboration, import/export, the desktop
//! engine) reads and writes this model; nothing keeps a second representation
//! of document state.
//!
//! # Invariants
//!
//! * A document always has at least one page and a valid `active_page`.
//! * Node ids are unique within a document; parents and children agree.
//! * The node tree is acyclic and no deeper than [`validation::MAX_DEPTH`].
//! * Mutations go through [`Transaction`]s so undo, collaboration and tests
//!   observe the exact same edit stream.

pub mod color;
pub mod document;
pub mod geometry;
pub mod ids;
pub mod node;
pub mod ops;
pub mod page;
pub mod paint;
pub mod prototype;
pub mod style;
pub mod transactions;
pub mod validation;
pub mod variables;

pub use color::Color;
pub use document::{
    AssetRegistry, ComponentMeta, ComponentRegistry, ComponentSetMeta, DevicePreset, DocKind,
    Document, DocumentSettings, FontAssetMeta, GridSettings, ImageAssetMeta, PrototypeFlow,
    PrototypeSettings, SCHEMA_VERSION,
};
pub use geometry::{
    AutoLayout, AutoSize, Constraints, CounterAlign, FillRule, HorizontalConstraint,
    LayoutDirection, LineHeight, LineHeightUnit, NodeGeometry, PathSide, PrimaryAlign, SizeSpec,
    Sizing, TextAlign, TextCase, TextContent, TextDecoration, TextOnPath, TextParagraph, TextStyle,
    VerticalAlign,
};
pub use ids::{
    AssetId, CollectionId, ComponentId, ComponentSetId, DocumentId, EntityId, FlowId,
    InteractionId, ModeId, NodeId, PageId, StyleId, VariableId,
};
pub use node::{ComponentRole, InstanceOverride, Node, NodeKind, PrototypeNode, Transform};
pub use ops::{
    Align, DistributeAxis, align_nodes, apply_transaction, boolean_nodes, delete_nodes,
    distribute_nodes, duplicate_nodes, group_nodes, insert_nodes, nudge, patch_nodes, reorder,
    ungroup,
};
pub use page::{Page, PageError};
pub use paint::{
    BlendMode, Effect, Fill, GradientStop, ImageFit, LineCap, LineJoin, Paint, Stroke, StrokeAlign,
};
pub use prototype::{
    Action, Easing, Interaction, OverlayPosition, ScrollAlign, SlideDirection, Transition,
    TransitionKind, Trigger,
};
pub use style::{NodeStyle, SharedEffectStyle, SharedPaintStyle, SharedTextStyle, StyleRegistry};
pub use transactions::{Edit, History, NodePatch, Transaction};
pub use validation::{ValidationIssue, ValidationSeverity, has_errors, validate};
pub use variables::{Variable, VariableCollection, VariableRegistry, VariableType, VariableValue};

pub use dp_geom;

use thiserror::Error;

/// Version of the model crate, used in diagnostics.
pub const MODEL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Errors produced by document operations.
#[derive(Debug, Error)]
pub enum ModelError {
    /// A page id does not exist.
    #[error("page {0} does not exist")]
    PageNotFound(PageId),
    /// A node id does not exist.
    #[error("node {0} does not exist")]
    NodeNotFound(NodeId),
    /// The document is structurally invalid for the requested operation.
    #[error("invalid document: {0}")]
    InvalidDocument(String),
    /// The document uses a schema this build does not understand.
    #[error("unsupported document schema {found}; this build supports up to {supported}")]
    UnsupportedSchema {
        /// Schema version found in the document.
        found: u32,
        /// Newest schema this build can open.
        supported: u32,
    },
    /// JSON (de)serialization failed.
    #[error("json error: {0}")]
    Json(String),
    /// The operation is recognised but not supported by this build yet.
    #[error("unsupported operation: {0}")]
    Unsupported(String),
    /// A page invariant was violated.
    #[error(transparent)]
    Page(#[from] PageError),
}

impl From<serde_json::Error> for ModelError {
    fn from(error: serde_json::Error) -> ModelError {
        ModelError::Json(error.to_string())
    }
}

/// serde default for opacities and similar 0..1 factors.
pub(crate) fn one() -> f64 {
    1.0
}

/// serde default for booleans that default to true (visibility).
pub(crate) fn truth() -> bool {
    true
}

/// serde default for revision counters.
pub(crate) fn rev_one() -> u64 {
    1
}
