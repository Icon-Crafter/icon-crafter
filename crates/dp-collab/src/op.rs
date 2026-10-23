//! Wire operations and envelopes.
//!
//! Ops are the unit of replication: one property update, one insert, one
//! delete, one reparent. They carry the stamp that authored them, which is what
//! makes application order-independent. Envelopes group the ops a client emits
//! in one batch; the server persists an envelope atomically and relays it.

use dp_model::{Node, NodeId, Page, PageId};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::clock::{ActorId, Hlc};

/// Current wire protocol version.
pub const PROTOCOL_VERSION: u32 = 1;

/// A last-writer-wins node property.
///
/// Fields mirror the replicated subset of [`dp_model::Node`]. Structural fields
/// (id, parent, children) are handled by dedicated ops, never by `SetField`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeField {
    /// Display name.
    Name,
    /// Visibility flag.
    Visible,
    /// Lock flag.
    Locked,
    /// Opacity `0..=1`.
    Opacity,
    /// Blend mode.
    Blend,
    /// Position/size/rotation/flip.
    Transform,
    /// Fills, strokes, effects, radii.
    Style,
    /// Shape geometry.
    Geometry,
    /// Text content.
    Text,
    /// Auto-layout settings.
    Layout,
    /// Resize constraints.
    Constraints,
    /// Component role/overrides.
    Component,
    /// Prototype interactions.
    Prototype,
    /// Plugin-authored properties.
    Props,
    /// Plugin-private data.
    PluginData,
}

/// Protocol-level error.
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    /// Envelope uses a protocol this build does not understand.
    #[error("unsupported protocol version {found} (this build speaks {supported})")]
    UnsupportedProtocol {
        /// Version found on the wire.
        found: u32,
        /// Version supported by this build.
        supported: u32,
    },
    /// Envelope carries no operations.
    #[error("collaboration envelope contains no operations")]
    EmptyBatch,
    /// Ops in one client-authored envelope must share the envelope actor.
    #[error("op stamp actor does not match envelope actor")]
    MixedActors,
    /// A `SetField` value could not be decoded into the model type.
    #[error("invalid value for field {field:?}: {message}")]
    InvalidValue {
        /// Field being decoded.
        field: NodeField,
        /// Serde error message.
        message: String,
    },
    /// A structural op references a node that does not exist.
    #[error("node {0} is not part of the replica")]
    UnknownNode(NodeId),
    /// A structural op references a page that does not exist.
    #[error("page {0} is not part of the replica")]
    UnknownPage(PageId),
    /// Materializing a replica produced an invalid document.
    #[error("replicated document is invalid: {0}")]
    InvalidDocument(String),
}

impl NodeField {
    /// Applies a JSON value to the node property.
    pub fn apply(self, node: &mut Node, value: Value) -> Result<(), OpError> {
        match self {
            NodeField::Name => node.name = decode(self, value)?,
            NodeField::Visible => node.visible = decode(self, value)?,
            NodeField::Locked => node.locked = decode(self, value)?,
            NodeField::Opacity => node.opacity = decode(self, value)?,
            NodeField::Blend => node.blend = decode(self, value)?,
            NodeField::Transform => node.transform = decode(self, value)?,
            NodeField::Style => node.style = decode(self, value)?,
            NodeField::Geometry => node.geometry = decode_optional(self, value)?,
            NodeField::Text => node.text = decode_optional(self, value)?,
            NodeField::Layout => node.layout = decode_optional(self, value)?,
            NodeField::Constraints => node.constraints = decode_optional(self, value)?,
            NodeField::Component => node.component = decode_optional(self, value)?,
            NodeField::Prototype => node.prototype = decode_optional(self, value)?,
            NodeField::Props => node.props = decode(self, value)?,
            NodeField::PluginData => node.plugin_data = decode(self, value)?,
        }
        Ok(())
    }

    /// Reads the property as JSON (used for diffs and tests).
    pub fn extract(self, node: &Node) -> Result<Value, OpError> {
        let value = match self {
            NodeField::Name => serde_json::to_value(&node.name),
            NodeField::Visible => serde_json::to_value(node.visible),
            NodeField::Locked => serde_json::to_value(node.locked),
            NodeField::Opacity => serde_json::to_value(node.opacity),
            NodeField::Blend => serde_json::to_value(node.blend),
            NodeField::Transform => serde_json::to_value(node.transform),
            NodeField::Style => serde_json::to_value(&node.style),
            NodeField::Geometry => serde_json::to_value(&node.geometry),
            NodeField::Text => serde_json::to_value(&node.text),
            NodeField::Layout => serde_json::to_value(&node.layout),
            NodeField::Constraints => serde_json::to_value(node.constraints),
            NodeField::Component => serde_json::to_value(&node.component),
            NodeField::Prototype => serde_json::to_value(&node.prototype),
            NodeField::Props => serde_json::to_value(&node.props),
            NodeField::PluginData => serde_json::to_value(&node.plugin_data),
        };
        value.map_err(|error| OpError::InvalidValue {
            field: self,
            message: error.to_string(),
        })
    }

    /// Every replicated property, for stamp initialization on insert.
    pub const ALL: [NodeField; 15] = [
        NodeField::Name,
        NodeField::Visible,
        NodeField::Locked,
        NodeField::Opacity,
        NodeField::Blend,
        NodeField::Transform,
        NodeField::Style,
        NodeField::Geometry,
        NodeField::Text,
        NodeField::Layout,
        NodeField::Constraints,
        NodeField::Component,
        NodeField::Prototype,
        NodeField::Props,
        NodeField::PluginData,
    ];
}

fn decode<T: serde::de::DeserializeOwned>(field: NodeField, value: Value) -> Result<T, OpError> {
    serde_json::from_value(value).map_err(|error| OpError::InvalidValue {
        field,
        message: error.to_string(),
    })
}

fn decode_optional<T: serde::de::DeserializeOwned>(
    field: NodeField,
    value: Value,
) -> Result<Option<T>, OpError> {
    if value.is_null() {
        Ok(None)
    } else {
        decode(field, value).map(Some)
    }
}

/// One replicated operation.
///
/// Ops are short-lived values that travel in batches (one envelope per UI
/// action), so the larger insert/page variants are kept inline: boxing would add
/// an allocation to every op to shrink a struct that is never stored in large
/// collections on the client.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Op {
    /// Creates a node under a parent (or at the page root).
    InsertNode {
        /// Target page.
        page: PageId,
        /// Full node snapshot; children are rebuilt from ordering ops.
        node: Node,
        /// Parent container (`None` for a page root).
        parent: Option<NodeId>,
        /// Sibling to insert after (`None` for the first position).
        after: Option<NodeId>,
        /// Author stamp.
        stamp: Hlc,
    },
    /// Tombstones a node (all later field edits are preserved but invisible
    /// until an explicit insert re-creates the id).
    DeleteNode {
        /// Target page.
        page: PageId,
        /// Node to delete.
        node: NodeId,
        /// Author stamp.
        stamp: Hlc,
    },
    /// Last-writer-wins property update.
    SetField {
        /// Target page.
        page: PageId,
        /// Target node.
        node: NodeId,
        /// Property.
        field: NodeField,
        /// New value.
        value: Value,
        /// Author stamp.
        stamp: Hlc,
    },
    /// Moves a node under a new parent / sibling position.
    Reparent {
        /// Target page.
        page: PageId,
        /// Node to move.
        node: NodeId,
        /// New parent (`None` for page root).
        parent: Option<NodeId>,
        /// New previous sibling (`None` for first position).
        after: Option<NodeId>,
        /// Author stamp.
        stamp: Hlc,
    },
    /// Adds a page.
    AddPage {
        /// Page snapshot (its node list is expected empty).
        page: Page,
        /// Author stamp.
        stamp: Hlc,
    },
    /// Renames a page.
    RenamePage {
        /// Page to rename.
        page: PageId,
        /// New name.
        name: String,
        /// Author stamp.
        stamp: Hlc,
    },
    /// Renames the document.
    RenameDocument {
        /// New name.
        name: String,
        /// Author stamp.
        stamp: Hlc,
    },
}

impl Op {
    /// Author stamp.
    pub fn stamp(&self) -> Hlc {
        match self {
            Op::InsertNode { stamp, .. }
            | Op::DeleteNode { stamp, .. }
            | Op::SetField { stamp, .. }
            | Op::Reparent { stamp, .. }
            | Op::AddPage { stamp, .. }
            | Op::RenamePage { stamp, .. }
            | Op::RenameDocument { stamp, .. } => *stamp,
        }
    }

    /// Page the op targets (document-level ops return `None`).
    pub fn page(&self) -> Option<PageId> {
        match self {
            Op::InsertNode { page, .. }
            | Op::DeleteNode { page, .. }
            | Op::SetField { page, .. }
            | Op::Reparent { page, .. }
            | Op::RenamePage { page, .. } => Some(*page),
            Op::AddPage { page, .. } => Some(page.id),
            Op::RenameDocument { .. } => None,
        }
    }

    /// Actor that authored the op.
    pub fn actor(&self) -> ActorId {
        self.stamp().actor
    }
}

/// Group of ops emitted by one client in one batch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Envelope {
    /// Wire protocol version.
    pub protocol: u32,
    /// Client-chosen batch id (idempotency key on the server).
    pub batch_id: Uuid,
    /// Authoring actor.
    pub actor: ActorId,
    /// Per-actor monotonically increasing sequence number.
    pub seq: u64,
    /// Operations.
    pub ops: Vec<Op>,
}

impl Envelope {
    /// Creates an envelope and validates it.
    pub fn new(actor: ActorId, seq: u64, ops: Vec<Op>) -> Result<Self, OpError> {
        let envelope = Self {
            protocol: PROTOCOL_VERSION,
            batch_id: Uuid::new_v4(),
            actor,
            seq,
            ops,
        };
        envelope.validate()?;
        Ok(envelope)
    }

    /// Validates protocol version, non-emptiness and actor consistency.
    pub fn validate(&self) -> Result<(), OpError> {
        if self.protocol != PROTOCOL_VERSION {
            return Err(OpError::UnsupportedProtocol {
                found: self.protocol,
                supported: PROTOCOL_VERSION,
            });
        }
        if self.ops.is_empty() {
            return Err(OpError::EmptyBatch);
        }
        if self.ops.iter().any(|op| op.actor() != self.actor) {
            return Err(OpError::MixedActors);
        }
        Ok(())
    }

    /// Highest stamp in the batch, for clock merging.
    pub fn max_stamp(&self) -> Option<Hlc> {
        self.ops.iter().map(Op::stamp).max()
    }
}

/// Ephemeral presence update (never persisted, not replicated).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Presence {
    /// Actor behind the cursor.
    pub actor: ActorId,
    /// Display name.
    pub name: String,
    /// UI colour (hex).
    pub color: String,
    /// Page currently viewed.
    pub page: PageId,
    /// Selected nodes.
    pub selection: Vec<NodeId>,
    /// Pointer position in world space.
    pub cursor: Option<dp_geom::Point>,
    /// Client wall-clock time in milliseconds.
    pub last_seen_ms: u64,
}

/// Structural conflict reported back to the UI.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Conflict {
    /// Human-readable description.
    pub message: String,
    /// Node involved, when applicable.
    pub node: Option<NodeId>,
    /// Field involved, when applicable.
    pub field: Option<NodeField>,
}

/// Convenience map type for typed props decoding (kept public for callers).
pub type PropsMap = IndexMap<String, Value>;
