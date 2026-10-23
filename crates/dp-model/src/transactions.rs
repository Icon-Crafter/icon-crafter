//! Undoable edits: patches, structural edits, transactions and history.
//!
//! Every user-visible change is expressed as a [`Transaction`] containing
//! reversible [`Edit`] values. Edits are applied forwards and reverted in
//! reverse order, which keeps structural edits (insert/remove/reparent)
//! consistent. The engine owns the `History` and the current selection, so
//! transactions carry the selection before/after for undo fidelity.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::ModelError;
use crate::document::Document;
use crate::geometry::{AutoLayout, Constraints, NodeGeometry, TextContent};
use crate::ids::{NodeId, PageId};
use crate::node::{ComponentRole, Node, PrototypeNode, Transform};
use crate::paint::BlendMode;
use crate::style::NodeStyle;

/// A partial update to a node's properties.
///
/// `None` means "leave unchanged". Nested `Option`s (`Some(None)`) clear an
/// optional property. Patches replace whole values, which makes undo and
/// collaboration rebasing deterministic.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodePatch {
    /// New name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New visibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    /// New lock state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// New opacity (0..=1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// New blend mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<BlendMode>,
    /// New transform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    /// New style (fills, strokes, effects, corners).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<NodeStyle>,
    /// New geometry; `Some(None)` removes shape geometry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Option<NodeGeometry>>,
    /// New text content; `Some(None)` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Option<TextContent>>,
    /// New auto layout; `Some(None)` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Option<AutoLayout>>,
    /// New constraints; `Some(None)` removes them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Option<Constraints>>,
    /// New component role; `Some(None)` detaches it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<Option<ComponentRole>>,
    /// New prototype data; `Some(None)` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prototype: Option<Option<PrototypeNode>>,
    /// Replaces the free-form props map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub props: Option<IndexMap<String, serde_json::Value>>,
    /// Replaces the plugin private data map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_data: Option<IndexMap<String, String>>,
}

impl NodePatch {
    /// Empty patch.
    pub fn new() -> NodePatch {
        NodePatch::default()
    }

    /// Whether the patch changes nothing.
    pub fn is_empty(&self) -> bool {
        self == &NodePatch::default()
    }

    /// Convenience: opacity patch.
    pub fn opacity(value: f64) -> NodePatch {
        NodePatch {
            opacity: Some(value.clamp(0.0, 1.0)),
            ..NodePatch::default()
        }
    }

    /// Convenience: name patch.
    pub fn name(value: impl Into<String>) -> NodePatch {
        NodePatch {
            name: Some(value.into()),
            ..NodePatch::default()
        }
    }

    /// Convenience: visibility patch.
    pub fn visible(value: bool) -> NodePatch {
        NodePatch {
            visible: Some(value),
            ..NodePatch::default()
        }
    }

    /// Convenience: transform patch.
    pub fn transform(value: Transform) -> NodePatch {
        NodePatch {
            transform: Some(value),
            ..NodePatch::default()
        }
    }

    /// Applies the patch to a node in place.
    pub fn apply_to(&self, node: &mut Node) {
        if let Some(value) = &self.name {
            node.name = value.clone();
        }
        if let Some(value) = self.visible {
            node.visible = value;
        }
        if let Some(value) = self.locked {
            node.locked = value;
        }
        if let Some(value) = self.opacity {
            node.opacity = value.clamp(0.0, 1.0);
        }
        if let Some(value) = self.blend {
            node.blend = value;
        }
        if let Some(value) = &self.transform {
            node.transform = *value;
        }
        if let Some(value) = &self.style {
            node.style = value.clone();
        }
        if let Some(value) = &self.geometry {
            node.geometry = value.clone();
        }
        if let Some(value) = &self.text {
            node.text = value.clone();
        }
        if let Some(value) = &self.layout {
            node.layout = value.clone();
        }
        if let Some(value) = &self.constraints {
            node.constraints = *value;
        }
        if let Some(value) = &self.component {
            node.component = value.clone();
        }
        if let Some(value) = &self.prototype {
            node.prototype = value.clone();
        }
        if let Some(value) = &self.props {
            node.props = value.clone();
        }
        if let Some(value) = &self.plugin_data {
            node.plugin_data = value.clone();
        }
    }

    /// Creates a patch that turns `before` into `after`.
    pub fn diff(before: &Node, after: &Node) -> NodePatch {
        let mut patch = NodePatch::default();
        if before.name != after.name {
            patch.name = Some(after.name.clone());
        }
        if before.visible != after.visible {
            patch.visible = Some(after.visible);
        }
        if before.locked != after.locked {
            patch.locked = Some(after.locked);
        }
        if before.opacity != after.opacity {
            patch.opacity = Some(after.opacity);
        }
        if before.blend != after.blend {
            patch.blend = Some(after.blend);
        }
        if before.transform != after.transform {
            patch.transform = Some(after.transform);
        }
        if before.style != after.style {
            patch.style = Some(after.style.clone());
        }
        if before.geometry != after.geometry {
            patch.geometry = Some(after.geometry.clone());
        }
        if before.text != after.text {
            patch.text = Some(after.text.clone());
        }
        if before.layout != after.layout {
            patch.layout = Some(after.layout.clone());
        }
        if before.constraints != after.constraints {
            patch.constraints = Some(after.constraints);
        }
        if before.component != after.component {
            patch.component = Some(after.component.clone());
        }
        if before.prototype != after.prototype {
            patch.prototype = Some(after.prototype.clone());
        }
        if before.props != after.props {
            patch.props = Some(after.props.clone());
        }
        if before.plugin_data != after.plugin_data {
            patch.plugin_data = Some(after.plugin_data.clone());
        }
        patch
    }
}

/// A single reversible structural or property edit within one page.
///
/// Variants embed whole [`Node`] snapshots so undo/redo and the collaboration
/// journal are self-contained and cheap to apply; boxing would add an
/// allocation per edit and complicate the serialized format, so the enum is
/// intentionally large (see the `Edit` size note in section 5 of the document
/// format).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Edit {
    /// Replace a node's properties with `after` (undo restores `before`).
    SetNode {
        /// Page the node lives on.
        page: PageId,
        /// Node state before the edit.
        before: Node,
        /// Node state after the edit.
        after: Node,
    },
    /// Insert a node (with its children list) under `parent` at `index`.
    InsertNode {
        /// Page the node lives on.
        page: PageId,
        /// Node to insert.
        node: Node,
        /// Parent node (or page root when `None`).
        parent: Option<NodeId>,
        /// Insertion index.
        index: usize,
    },
    /// Remove a single node (children must be removed separately).
    RemoveNode {
        /// Page the node lived on.
        page: PageId,
        /// Removed node snapshot.
        node: Node,
        /// Former parent (or page root when `None`).
        parent: Option<NodeId>,
        /// Former index.
        index: usize,
    },
    /// Move a node to a different parent/index.
    Reparent {
        /// Page the node lives on.
        page: PageId,
        /// Moved node.
        node_id: NodeId,
        /// Parent before the move.
        old_parent: Option<NodeId>,
        /// Index before the move.
        old_index: usize,
        /// Parent after the move.
        new_parent: Option<NodeId>,
        /// Index after the move.
        new_index: usize,
    },
}

impl Edit {
    /// Human readable label used in the history panel / status bar.
    pub fn label(&self) -> &'static str {
        match self {
            Edit::SetNode { .. } => "edit",
            Edit::InsertNode { .. } => "insert",
            Edit::RemoveNode { .. } => "delete",
            Edit::Reparent { .. } => "move",
        }
    }

    /// Applies the edit forward.
    pub fn apply(&self, document: &mut Document) -> Result<(), ModelError> {
        match self {
            Edit::SetNode { after, .. } => {
                let node = document
                    .node_mut(after.id)
                    .ok_or(ModelError::NodeNotFound(after.id))?;
                *node = after.clone();
                Ok(())
            }
            Edit::InsertNode {
                page,
                node,
                parent,
                index,
            } => {
                let page = document
                    .page_mut(*page)
                    .ok_or(ModelError::PageNotFound(*page))?;
                page.insert_node(node.clone(), *parent, Some(*index))?;
                Ok(())
            }
            Edit::RemoveNode { page, node, .. } => {
                let page = document
                    .page_mut(*page)
                    .ok_or(ModelError::PageNotFound(*page))?;
                page.remove_node(node.id)
                    .map(|_| ())
                    .ok_or(ModelError::NodeNotFound(node.id))
            }
            Edit::Reparent {
                page,
                node_id,
                new_parent,
                new_index,
                ..
            } => {
                let page = document
                    .page_mut(*page)
                    .ok_or(ModelError::PageNotFound(*page))?;
                page.reorder(*node_id, *new_parent, Some(*new_index))?;
                Ok(())
            }
        }
    }

    /// Reverts the edit.
    pub fn revert(&self, document: &mut Document) -> Result<(), ModelError> {
        match self {
            Edit::SetNode { before, .. } => {
                let node = document
                    .node_mut(before.id)
                    .ok_or(ModelError::NodeNotFound(before.id))?;
                *node = before.clone();
                Ok(())
            }
            Edit::InsertNode { page, node, .. } => {
                let page = document
                    .page_mut(*page)
                    .ok_or(ModelError::PageNotFound(*page))?;
                page.remove_node(node.id)
                    .map(|_| ())
                    .ok_or(ModelError::NodeNotFound(node.id))
            }
            Edit::RemoveNode {
                page,
                node,
                parent,
                index,
            } => {
                let page = document
                    .page_mut(*page)
                    .ok_or(ModelError::PageNotFound(*page))?;
                page.insert_node(node.clone(), *parent, Some(*index))?;
                Ok(())
            }
            Edit::Reparent {
                page,
                node_id,
                old_parent,
                old_index,
                ..
            } => {
                let page = document
                    .page_mut(*page)
                    .ok_or(ModelError::PageNotFound(*page))?;
                page.reorder(*node_id, *old_parent, Some(*old_index))?;
                Ok(())
            }
        }
    }
}

/// A coherent group of edits that undo/redo as a unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    /// Edits in application order.
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// Label shown to the user ("Move", "Resize", ...).
    pub label: String,
    /// Optional key used to merge consecutive transactions (e.g. dragging).
    #[serde(default)]
    pub coalesce_key: Option<String>,
    /// Selection before the transaction.
    #[serde(default)]
    pub selection_before: Vec<NodeId>,
    /// Selection after the transaction.
    #[serde(default)]
    pub selection_after: Vec<NodeId>,
}

impl Transaction {
    /// Creates an empty labelled transaction.
    pub fn new(label: impl Into<String>) -> Transaction {
        Transaction {
            edits: Vec::new(),
            label: label.into(),
            coalesce_key: None,
            selection_before: Vec::new(),
            selection_after: Vec::new(),
        }
    }

    /// Sets the coalesce key (used to merge a drag into one undo step).
    pub fn coalesce(mut self, key: impl Into<String>) -> Transaction {
        self.coalesce_key = Some(key.into());
        self
    }

    /// Sets the selection before/after.
    pub fn with_selection(mut self, before: Vec<NodeId>, after: Vec<NodeId>) -> Transaction {
        self.selection_before = before;
        self.selection_after = after;
        self
    }

    /// Number of edits.
    pub fn len(&self) -> usize {
        self.edits.len()
    }

    /// Whether the transaction contains no edits.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Appends an edit.
    pub fn push(&mut self, edit: Edit) {
        self.edits.push(edit);
    }

    /// Whether this transaction can be merged with `next`.
    pub fn can_coalesce_with(&self, next: &Transaction) -> bool {
        match (&self.coalesce_key, &next.coalesce_key) {
            (Some(left), Some(right)) => left == right && !self.edits.is_empty(),
            _ => false,
        }
    }
}

/// Undo/redo stacks of committed transactions.
#[derive(Debug, Clone)]
pub struct History {
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        History::new()
    }
}

impl History {
    /// Default history depth.
    pub const DEFAULT_LIMIT: usize = 200;

    /// Creates a history with the default depth.
    pub fn new() -> History {
        History::with_limit(History::DEFAULT_LIMIT)
    }

    /// Creates a history with a custom depth (minimum 1).
    pub fn with_limit(limit: usize) -> History {
        History {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Committed transactions available for undo.
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Transactions available for redo.
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Whether undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Label of the next undo step.
    pub fn next_undo_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|tx| tx.label.as_str())
    }

    /// Label of the next redo step.
    pub fn next_redo_label(&self) -> Option<&str> {
        self.redo_stack.last().map(|tx| tx.label.as_str())
    }

    /// Commits a transaction (merging with the previous one when requested).
    pub fn push(&mut self, transaction: Transaction) {
        if transaction.is_empty() {
            return;
        }
        if let Some(last) = self.undo_stack.last_mut()
            && last.can_coalesce_with(&transaction)
        {
            last.edits.extend(transaction.edits);
            last.selection_after = transaction.selection_after;
            self.redo_stack.clear();
            return;
        }
        self.undo_stack.push(transaction);
        while self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Reverts the last transaction and returns it.
    pub fn undo(&mut self, document: &mut Document) -> Result<Option<Transaction>, ModelError> {
        let Some(transaction) = self.undo_stack.pop() else {
            return Ok(None);
        };
        for edit in transaction.edits.iter().rev() {
            if let Err(error) = edit.revert(document) {
                tracing::warn!(%error, label = %transaction.label, "undo failed");
                self.undo_stack.push(transaction);
                return Err(error);
            }
        }
        document.touch();
        self.redo_stack.push(transaction.clone());
        Ok(Some(transaction))
    }

    /// Reapplies the last undone transaction and returns it.
    pub fn redo(&mut self, document: &mut Document) -> Result<Option<Transaction>, ModelError> {
        let Some(transaction) = self.redo_stack.pop() else {
            return Ok(None);
        };
        for edit in transaction.edits.iter() {
            if let Err(error) = edit.apply(document) {
                tracing::warn!(%error, label = %transaction.label, "redo failed");
                self.redo_stack.push(transaction);
                return Err(error);
            }
        }
        document.touch();
        self.undo_stack.push(transaction.clone());
        Ok(Some(transaction))
    }

    /// Clears both stacks (used when loading a document).
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::geometry::TextContent;
    use crate::node::NodeKind;

    fn apply(
        document: &mut Document,
        history: &mut History,
        tx: Transaction,
    ) -> Result<(), ModelError> {
        for edit in &tx.edits {
            edit.apply(document)?;
        }
        document.touch();
        history.push(tx);
        Ok(())
    }

    fn document_with_rect() -> (Document, NodeId) {
        let mut document = Document::new("Transactions");
        let page_id = document.active_page_id();
        let rect = Node::rectangle("Rect", 10.0, 10.0, 100.0, 50.0);
        let rect_id = rect.id;
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(rect, None, None)
            .unwrap();
        (document, rect_id)
    }

    #[test]
    fn patch_roundtrip_and_undo() {
        let (mut document, rect_id) = document_with_rect();
        let before = document.node(rect_id).unwrap().clone();
        let mut after = before.clone();
        after.opacity = 0.25;
        after.name = "Patched".to_string();
        let mut tx =
            Transaction::new("Change opacity").with_selection(vec![rect_id], vec![rect_id]);
        tx.push(Edit::SetNode {
            page: document.active_page_id(),
            before,
            after,
        });
        let mut history = History::new();
        apply(&mut document, &mut history, tx).unwrap();
        assert_eq!(document.node(rect_id).unwrap().opacity, 0.25);
        assert_eq!(document.node(rect_id).unwrap().name, "Patched");
        assert_eq!(document.revision, 2);
        let undone = history.undo(&mut document).unwrap().unwrap();
        assert_eq!(undone.label, "Change opacity");
        assert_eq!(document.node(rect_id).unwrap().opacity, 1.0);
        assert_eq!(document.node(rect_id).unwrap().name, "Rect");
        assert!(history.can_redo());
        history.redo(&mut document).unwrap().unwrap();
        assert_eq!(document.node(rect_id).unwrap().opacity, 0.25);
    }

    #[test]
    fn insert_remove_roundtrip() {
        let (mut document, rect_id) = document_with_rect();
        let page_id = document.active_page_id();
        let node = document.node(rect_id).unwrap().clone();
        let index = document
            .page(page_id)
            .unwrap()
            .index_in_parent(rect_id)
            .unwrap()
            .1;
        let mut history = History::new();
        let mut tx = Transaction::new("Delete");
        tx.push(Edit::RemoveNode {
            page: page_id,
            node: node.clone(),
            parent: None,
            index,
        });
        apply(&mut document, &mut history, tx).unwrap();
        assert!(document.node(rect_id).is_none());
        history.undo(&mut document).unwrap().unwrap();
        assert!(document.node(rect_id).is_some());
        assert_eq!(
            document.page(page_id).unwrap().index_in_parent(rect_id),
            Some((None, 0))
        );
        history.redo(&mut document).unwrap().unwrap();
        assert!(document.node(rect_id).is_none());
    }

    #[test]
    fn reparent_undo_restores_order() {
        let (mut document, rect_id) = document_with_rect();
        let page_id = document.active_page_id();
        let group = Node::frame("Group", 0.0, 0.0, 200.0, 200.0);
        let group_id = group.id;
        document
            .page_mut(page_id)
            .unwrap()
            .insert_node(group, None, Some(0))
            .unwrap();
        let mut history = History::new();
        let mut tx = Transaction::new("Move into group");
        tx.push(Edit::Reparent {
            page: page_id,
            node_id: rect_id,
            old_parent: None,
            old_index: 1,
            new_parent: Some(group_id),
            new_index: 0,
        });
        apply(&mut document, &mut history, tx).unwrap();
        assert_eq!(document.node(rect_id).unwrap().parent, Some(group_id));
        history.undo(&mut document).unwrap().unwrap();
        assert_eq!(document.node(rect_id).unwrap().parent, None);
        assert_eq!(
            document.page(page_id).unwrap().roots(),
            &[group_id, rect_id]
        );
    }

    #[test]
    fn consecutive_transactions_coalesce() {
        let (mut document, rect_id) = document_with_rect();
        let page_id = document.active_page_id();
        let mut history = History::new();
        for _ in 0..5 {
            let before = document.node(rect_id).unwrap().clone();
            let mut after = before.clone();
            after.transform.x += 1.0;
            let mut tx = Transaction::new("Move")
                .coalesce("move-drag")
                .with_selection(vec![rect_id], vec![rect_id]);
            tx.push(Edit::SetNode {
                page: page_id,
                before,
                after,
            });
            apply(&mut document, &mut history, tx).unwrap();
        }
        assert_eq!(history.undo_len(), 1);
        history.undo(&mut document).unwrap().unwrap();
        assert!((document.node(rect_id).unwrap().transform.x - 10.0).abs() < 1e-9);
    }

    #[test]
    fn history_limit_drops_oldest() {
        let (mut document, rect_id) = document_with_rect();
        let page_id = document.active_page_id();
        let mut history = History::with_limit(2);
        for _ in 0..4 {
            let before = document.node(rect_id).unwrap().clone();
            let mut after = before.clone();
            after.transform.y += 1.0;
            let mut tx = Transaction::new("Move");
            tx.push(Edit::SetNode {
                page: page_id,
                before,
                after,
            });
            apply(&mut document, &mut history, tx).unwrap();
        }
        assert_eq!(history.undo_len(), 2);
    }

    #[test]
    fn node_patch_applies_all_fields() {
        let (document, rect_id) = document_with_rect();
        let page_id = document.active_page_id();
        let mut node = document.node(rect_id).unwrap().clone();
        let patch = NodePatch {
            name: Some("Renamed".into()),
            opacity: Some(0.5),
            visible: Some(false),
            locked: Some(true),
            blend: Some(crate::paint::BlendMode::Multiply),
            ..NodePatch::default()
        };
        patch.apply_to(&mut node);
        assert_eq!(node.name, "Renamed");
        assert_eq!(node.opacity, 0.5);
        assert!(!node.visible);
        assert!(node.locked);
        assert_eq!(node.blend, crate::paint::BlendMode::Multiply);
        assert!(!patch.is_empty());
        assert!(NodePatch::default().is_empty());

        let mut text_node = Node::text("Text", "Hello", 0.0, 0.0);
        let patch = NodePatch {
            text: Some(Some(TextContent::plain("World"))),
            ..NodePatch::default()
        };
        patch.apply_to(&mut text_node);
        assert_eq!(text_node.text.as_ref().unwrap().characters, "World");

        // Clearing an optional property with Some(None).
        let clear = NodePatch {
            layout: Some(None),
            ..NodePatch::default()
        };
        clear.apply_to(&mut text_node);
        assert!(text_node.layout.is_none());
        let _ = Color::WHITE;
        let _ = page_id;
        let _ = NodeKind::Text;
    }

    #[test]
    fn node_patch_diff_reproduces_change() {
        let (document, rect_id) = document_with_rect();
        let mut before = document.node(rect_id).unwrap().clone();
        let mut after = before.clone();
        after.transform.w = 250.0;
        after.style.fills.clear();
        let patch = NodePatch::diff(&before, &after);
        patch.apply_to(&mut before);
        assert_eq!(before.transform.w, 250.0);
        assert!(before.style.fills.is_empty());
    }
}
