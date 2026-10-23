//! Convergent replica state.
//!
//! A [`Replica`] holds the replicated projection of one design document. Local
//! edits produce [`Op`]s (which are applied immediately, as in any optimistic
//! UI); remote ops are applied with the same rules, so every replica that has
//! seen the same op set materializes the same [`Document`].
//!
//! Convergence rules:
//!
//! * node properties are per-field last-writer-wins registers ordered by [`Hlc`];
//! * sibling order is an RGA list (see [`crate::rga`]) anchored on stamps;
//! * deletes are tombstones ordered by stamp; an insert with a newer stamp
//!   resurrects a node id;
//! * pages and the document name are registers;
//! * the materialized [`Document`] is rebuilt from this state, never patched in
//!   place by ops.

use std::collections::HashMap;

use dp_model::{Color, Document, Node, NodeId, Page, PageId};
use indexmap::IndexMap;
use serde_json::Value;
use uuid::Uuid;

use crate::clock::{ActorId, Hlc, HlcClock};
use crate::op::{Conflict, Envelope, NodeField, Op, OpError};
use crate::rga::RgaList;

/// Deterministic actor used for the stamps of the initial snapshot, so every
/// replica seeded from the same document agrees on the base order.
fn seed_actor() -> ActorId {
    ActorId::from_uuid(Uuid::from_bytes([0u8; 16]))
}

/// Last-writer-wins register.
#[derive(Clone, Debug)]
struct Register<T> {
    value: T,
    stamp: Hlc,
}

impl<T: Clone> Register<T> {
    fn new(value: T, stamp: Hlc) -> Self {
        Self { value, stamp }
    }

    /// Returns `true` when the write won.
    fn set(&mut self, value: T, stamp: Hlc) -> bool {
        if stamp > self.stamp {
            self.value = value;
            self.stamp = stamp;
            true
        } else {
            false
        }
    }
}

/// Replicated state of one page.
#[derive(Debug)]
struct PageState {
    name: Register<String>,
    background: Color,
    /// Node property snapshots (id -> node). Child lists live in `order`.
    nodes: IndexMap<NodeId, Node>,
    /// Sibling order per parent (`None` = page root).
    order: HashMap<Option<NodeId>, RgaList<NodeId>>,
    /// Delete tombstones.
    deleted: HashMap<NodeId, Hlc>,
    /// Last-writer-wins stamp per property.
    field_stamps: HashMap<(NodeId, NodeField), Hlc>,
    /// Stamp of the latest insert of each node id.
    inserted: HashMap<NodeId, Hlc>,
    /// Parent register per node.
    locations: HashMap<NodeId, Register<Option<NodeId>>>,
}

impl PageState {
    fn new(name: String, background: Color, stamp: Hlc) -> Self {
        Self {
            name: Register::new(name, stamp),
            background,
            nodes: IndexMap::new(),
            order: HashMap::from([(None, RgaList::new())]),
            deleted: HashMap::new(),
            field_stamps: HashMap::new(),
            inserted: HashMap::new(),
            locations: HashMap::new(),
        }
    }

    fn is_alive(&self, id: NodeId) -> bool {
        match self.deleted.get(&id) {
            None => true,
            Some(delete) => self.inserted.get(&id).is_some_and(|insert| insert > delete),
        }
    }

    /// Alive children of `parent` in convergent order.
    ///
    /// Used by the replica tests today and by the upcoming dev-mode inspection
    /// and presence layers; kept on the type instead of duplicating the filter.
    #[allow(dead_code)]
    fn children_of(&self, parent: Option<NodeId>) -> Vec<NodeId> {
        self.order
            .get(&parent)
            .map(|list| {
                list.values()
                    .copied()
                    .filter(|id| self.is_alive(*id) && self.nodes.contains_key(id))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// One applied op and whether it changed anything.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyReport {
    /// Whether the replica state changed.
    pub changed: bool,
    /// Whether the op was ignored because a newer write already won.
    pub stale: bool,
    /// Conflicts worth surfacing (resolved structurally, never blocking).
    pub conflicts: Vec<Conflict>,
}

impl ApplyReport {
    fn changed() -> Self {
        Self {
            changed: true,
            stale: false,
            conflicts: Vec::new(),
        }
    }

    fn stale(conflict: Option<Conflict>) -> Self {
        Self {
            changed: false,
            stale: true,
            conflicts: conflict.into_iter().collect(),
        }
    }
}

/// Convergent replica of one document.
#[derive(Debug)]
pub struct Replica {
    actor: ActorId,
    clock: HlcClock,
    base: Document,
    doc_name: Register<String>,
    pages: IndexMap<PageId, PageState>,
    seq: u64,
    pending: Vec<Op>,
}

impl Replica {
    /// Seeds a replica from a snapshot.
    ///
    /// Every replica seeded from the same document produces identical state, so
    /// concurrently created replicas converge once they exchange ops.
    pub fn seed(document: &Document, actor: ActorId) -> Self {
        let seed = seed_actor();
        let mut counter = 1u64;
        let doc_name = Register::new(document.name.clone(), Hlc::new(1, seed));
        let mut pages = IndexMap::new();
        for page in &document.pages {
            counter += 1;
            let stamp = Hlc::new(counter, seed);
            let mut state = PageState::new(page.name.clone(), page.background, stamp);
            seed_subtree(&mut state, page, None, &page.roots, &mut counter);
            pages.insert(page.id, state);
        }
        let mut clock = HlcClock::new(actor);
        clock.merge(counter);
        Self {
            actor,
            clock,
            base: document.clone(),
            doc_name,
            pages,
            seq: 0,
            pending: Vec::new(),
        }
    }

    /// Actor that authors local ops.
    pub fn actor(&self) -> ActorId {
        self.actor
    }

    /// Current Lamport component.
    pub fn lamport(&self) -> u64 {
        self.clock.last()
    }

    /// Document name as currently known.
    pub fn document_name(&self) -> &str {
        &self.doc_name.value
    }

    /// Number of pages tracked.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Pending (not yet flushed) local ops.
    pub fn pending_ops(&self) -> &[Op] {
        &self.pending
    }

    /// Creates a pending local op for a property write and applies it.
    pub fn local_set(
        &mut self,
        page: PageId,
        node: NodeId,
        field: NodeField,
        value: Value,
    ) -> Result<Op, OpError> {
        let stamp = self.clock.tick();
        let op = Op::SetField {
            page,
            node,
            field,
            value,
            stamp,
        };
        self.apply(&op)?;
        self.pending.push(op.clone());
        Ok(op)
    }

    /// Creates a pending local insert op and applies it.
    pub fn local_insert(
        &mut self,
        page: PageId,
        node: Node,
        parent: Option<NodeId>,
        after: Option<NodeId>,
    ) -> Result<Op, OpError> {
        let stamp = self.clock.tick();
        let op = Op::InsertNode {
            page,
            node,
            parent,
            after,
            stamp,
        };
        self.apply(&op)?;
        self.pending.push(op.clone());
        Ok(op)
    }

    /// Creates a pending local delete op and applies it.
    pub fn local_delete(&mut self, page: PageId, node: NodeId) -> Result<Op, OpError> {
        let stamp = self.clock.tick();
        let op = Op::DeleteNode { page, node, stamp };
        self.apply(&op)?;
        self.pending.push(op.clone());
        Ok(op)
    }

    /// Creates a pending local reparent op and applies it.
    pub fn local_reparent(
        &mut self,
        page: PageId,
        node: NodeId,
        parent: Option<NodeId>,
        after: Option<NodeId>,
    ) -> Result<Op, OpError> {
        let stamp = self.clock.tick();
        let op = Op::Reparent {
            page,
            node,
            parent,
            after,
            stamp,
        };
        self.apply(&op)?;
        self.pending.push(op.clone());
        Ok(op)
    }

    /// Creates a pending local page insertion and applies it.
    pub fn local_add_page(&mut self, page: Page) -> Result<Op, OpError> {
        let stamp = self.clock.tick();
        let op = Op::AddPage { page, stamp };
        self.apply(&op)?;
        self.pending.push(op.clone());
        Ok(op)
    }

    /// Creates a pending local page rename and applies it.
    pub fn local_rename_page(&mut self, page: PageId, name: String) -> Result<Op, OpError> {
        let stamp = self.clock.tick();
        let op = Op::RenamePage { page, name, stamp };
        self.apply(&op)?;
        self.pending.push(op.clone());
        Ok(op)
    }

    /// Creates a pending local document rename and applies it.
    pub fn local_rename_document(&mut self, name: String) -> Result<Op, OpError> {
        let stamp = self.clock.tick();
        let op = Op::RenameDocument { name, stamp };
        self.apply(&op)?;
        self.pending.push(op.clone());
        Ok(op)
    }

    /// Drains pending ops into an envelope (returns `None` when nothing is
    /// pending).
    pub fn flush(&mut self) -> Result<Option<Envelope>, OpError> {
        if self.pending.is_empty() {
            return Ok(None);
        }
        let seq = self.seq;
        self.seq += 1;
        let ops = std::mem::take(&mut self.pending);
        let envelope = Envelope::new(self.actor, seq, ops)?;
        Ok(Some(envelope))
    }

    /// Applies one op (local or remote) with the convergence rules.
    pub fn apply(&mut self, op: &Op) -> Result<ApplyReport, OpError> {
        self.clock.observe(op.stamp());
        match op {
            Op::InsertNode {
                page,
                node,
                parent,
                after,
                stamp,
            } => self.apply_insert(*page, node, *parent, *after, *stamp),
            Op::DeleteNode { page, node, stamp } => self.apply_delete(*page, *node, *stamp),
            Op::SetField {
                page,
                node,
                field,
                value,
                stamp,
            } => self.apply_set_field(*page, *node, *field, value, *stamp),
            Op::Reparent {
                page,
                node,
                parent,
                after,
                stamp,
            } => self.apply_reparent(*page, *node, *parent, *after, *stamp),
            Op::AddPage { page, stamp } => self.apply_add_page(page, *stamp),
            Op::RenamePage { page, name, stamp } => self.apply_rename_page(*page, name, *stamp),
            Op::RenameDocument { name, stamp } => {
                let changed = self.doc_name.set(name.clone(), *stamp);
                Ok(if changed {
                    ApplyReport::changed()
                } else {
                    ApplyReport::stale(Some(Conflict {
                        message: "document rename superseded by a newer write".to_owned(),
                        node: None,
                        field: None,
                    }))
                })
            }
        }
    }

    /// Validates and applies every op of an envelope.
    pub fn apply_envelope(&mut self, envelope: &Envelope) -> Result<Vec<ApplyReport>, OpError> {
        envelope.validate()?;
        envelope.ops.iter().map(|op| self.apply(op)).collect()
    }

    fn page_mut(&mut self, page: PageId) -> Result<&mut PageState, OpError> {
        self.pages.get_mut(&page).ok_or(OpError::UnknownPage(page))
    }

    fn apply_insert(
        &mut self,
        page: PageId,
        node: &Node,
        parent: Option<NodeId>,
        after: Option<NodeId>,
        stamp: Hlc,
    ) -> Result<ApplyReport, OpError> {
        let state = self.page_mut(page)?;
        if state
            .inserted
            .get(&node.id)
            .is_some_and(|existing| *existing >= stamp)
        {
            return Ok(ApplyReport::stale(Some(Conflict {
                message: format!("node {} was already inserted", node.id),
                node: Some(node.id),
                field: None,
            })));
        }
        if let Some(parent_id) = parent
            && !state.nodes.contains_key(&parent_id)
        {
            return Err(OpError::UnknownNode(parent_id));
        }
        let mut stored = node.clone();
        stored.parent = parent;
        stored.children.clear();
        state.nodes.insert(node.id, stored);
        state.inserted.insert(node.id, stamp);
        state
            .locations
            .insert(node.id, Register::new(parent, stamp));
        for field in NodeField::ALL {
            let entry = state.field_stamps.entry((node.id, field)).or_insert(stamp);
            if *entry < stamp {
                *entry = stamp;
            }
        }
        let list = state.order.entry(parent).or_default();
        let anchor = after.filter(|anchor| list.values().any(|value| value == anchor));
        list.insert_after(anchor.as_ref(), node.id, stamp)
            .map_err(|_| OpError::InvalidDocument("duplicate node id in ordering".to_owned()))?;
        Ok(ApplyReport::changed())
    }

    fn apply_delete(
        &mut self,
        page: PageId,
        node: NodeId,
        stamp: Hlc,
    ) -> Result<ApplyReport, OpError> {
        let state = self.page_mut(page)?;
        if state
            .deleted
            .get(&node)
            .is_some_and(|delete| *delete >= stamp)
        {
            return Ok(ApplyReport::stale(Some(Conflict {
                message: format!("node {node} was already deleted by a newer op"),
                node: Some(node),
                field: None,
            })));
        }
        state.deleted.insert(node, stamp);
        if let Some(location) = state.locations.get(&node) {
            let parent = location.value;
            if let Some(list) = state.order.get_mut(&parent) {
                list.remove(&node);
            }
        }
        Ok(ApplyReport::changed())
    }

    fn apply_set_field(
        &mut self,
        page: PageId,
        node: NodeId,
        field: NodeField,
        value: &Value,
        stamp: Hlc,
    ) -> Result<ApplyReport, OpError> {
        let state = self.page_mut(page)?;
        if state
            .field_stamps
            .get(&(node, field))
            .is_some_and(|existing| *existing >= stamp)
        {
            return Ok(ApplyReport::stale(Some(Conflict {
                message: format!(
                    "{} on {node} superseded by a newer write",
                    field_name(field)
                ),
                node: Some(node),
                field: Some(field),
            })));
        }
        let node_ref = state
            .nodes
            .get_mut(&node)
            .ok_or(OpError::UnknownNode(node))?;
        field.apply(node_ref, value.clone())?;
        state.field_stamps.insert((node, field), stamp);
        Ok(ApplyReport::changed())
    }

    fn apply_reparent(
        &mut self,
        page: PageId,
        node: NodeId,
        parent: Option<NodeId>,
        after: Option<NodeId>,
        stamp: Hlc,
    ) -> Result<ApplyReport, OpError> {
        let state = self.page_mut(page)?;
        if !state.nodes.contains_key(&node) {
            return Err(OpError::UnknownNode(node));
        }
        if let Some(parent_id) = parent {
            if !state.nodes.contains_key(&parent_id) {
                return Err(OpError::UnknownNode(parent_id));
            }
            if parent_id == node || is_ancestor(state, parent_id, node) {
                return Err(OpError::InvalidDocument(
                    "reparent would create a cycle in the page tree".to_owned(),
                ));
            }
        }
        let old_parent = state.locations.get(&node).map(|register| register.value);
        let winning = state
            .locations
            .get(&node)
            .is_none_or(|register| register.stamp < stamp);
        if !winning {
            return Ok(ApplyReport::stale(Some(Conflict {
                message: format!("move of {node} superseded by a newer move"),
                node: Some(node),
                field: None,
            })));
        }
        if let Some(list) = old_parent.and_then(|parent_id| state.order.get_mut(&parent_id)) {
            list.remove(&node);
        }
        state.locations.insert(node, Register::new(parent, stamp));
        let list = state.order.entry(parent).or_default();
        let anchor = after.filter(|anchor| list.values().any(|value| value == anchor));
        list.insert_after(anchor.as_ref(), node, stamp)
            .map_err(|_| OpError::InvalidDocument("duplicate node id in ordering".to_owned()))?;
        Ok(ApplyReport::changed())
    }

    fn apply_add_page(&mut self, page: &Page, stamp: Hlc) -> Result<ApplyReport, OpError> {
        if self.pages.contains_key(&page.id) {
            return Ok(ApplyReport::stale(Some(Conflict {
                message: format!("page {} already exists", page.id),
                node: None,
                field: None,
            })));
        }
        let mut counter = stamp.lamport;
        let mut state = PageState::new(page.name.clone(), page.background, stamp);
        seed_subtree(&mut state, page, None, &page.roots, &mut counter);
        self.clock.merge(counter);
        self.pages.insert(page.id, state);
        Ok(ApplyReport::changed())
    }

    fn apply_rename_page(
        &mut self,
        page: PageId,
        name: &str,
        stamp: Hlc,
    ) -> Result<ApplyReport, OpError> {
        let state = self.page_mut(page)?;
        let changed = state.name.set(name.to_owned(), stamp);
        Ok(if changed {
            ApplyReport::changed()
        } else {
            ApplyReport::stale(Some(Conflict {
                message: format!("rename of page {page} superseded by a newer write"),
                node: None,
                field: None,
            }))
        })
    }

    /// Rebuilds the converged document from the replicated state.
    pub fn to_document(&self) -> Result<Document, OpError> {
        let mut document = self.base.clone();
        document.name = self.doc_name.value.clone();
        document.revision = document.revision.max(self.clock.last());
        document.pages.clear();
        for (page_id, state) in &self.pages {
            let mut page = Page::new(state.name.value.clone());
            page.id = *page_id;
            page.background = state.background;
            for (node_id, node) in &state.nodes {
                if !state.is_alive(*node_id) {
                    continue;
                }
                let mut stored = node.clone();
                stored.children.clear();
                page.nodes.insert(*node_id, stored);
            }
            for (parent, list) in &state.order {
                let children: Vec<NodeId> = list
                    .values()
                    .copied()
                    .filter(|id| page.nodes.contains_key(id))
                    .collect();
                for child in &children {
                    if let Some(node) = page.nodes.get_mut(child) {
                        node.parent = *parent;
                    }
                }
                match parent {
                    Some(parent_id) => {
                        if let Some(node) = page.nodes.get_mut(parent_id) {
                            node.children = children;
                        }
                    }
                    None => page.roots = children,
                }
            }
            document.pages.push(page);
        }
        if document.pages.is_empty() {
            return Err(OpError::InvalidDocument(
                "replica has no pages after materialization".to_owned(),
            ));
        }
        if !document
            .pages
            .iter()
            .any(|page| page.id == document.active_page)
        {
            document.active_page = document.pages[0].id;
        }
        Ok(document)
    }
}

fn field_name(field: NodeField) -> &'static str {
    match field {
        NodeField::Name => "name",
        NodeField::Visible => "visible",
        NodeField::Locked => "locked",
        NodeField::Opacity => "opacity",
        NodeField::Blend => "blend",
        NodeField::Transform => "transform",
        NodeField::Style => "style",
        NodeField::Geometry => "geometry",
        NodeField::Text => "text",
        NodeField::Layout => "layout",
        NodeField::Constraints => "constraints",
        NodeField::Component => "component",
        NodeField::Prototype => "prototype",
        NodeField::Props => "props",
        NodeField::PluginData => "plugin_data",
    }
}

fn seed_subtree(
    state: &mut PageState,
    page: &Page,
    parent: Option<NodeId>,
    children: &[NodeId],
    counter: &mut u64,
) {
    let seed = seed_actor();
    state.order.entry(parent).or_default();
    for child in children {
        let Some(node) = page.nodes.get(child) else {
            continue;
        };
        *counter += 1;
        let stamp = Hlc::new(*counter, seed);
        let mut stored = node.clone();
        stored.parent = parent;
        stored.children.clear();
        state.nodes.insert(*child, stored);
        state.inserted.insert(*child, stamp);
        state.locations.insert(*child, Register::new(parent, stamp));
        for field in NodeField::ALL {
            state.field_stamps.insert((*child, field), stamp);
        }
        let previous = state
            .order
            .get(&parent)
            .and_then(|list| list.values().last().cloned());
        {
            let list = state.order.entry(parent).or_default();
            if list.insert_after(previous.as_ref(), *child, stamp).is_err() {
                tracing::warn!(node = %child, "seed order insert rejected duplicate id");
            }
        }
        let grandchildren = node.children.clone();
        seed_subtree(state, page, Some(*child), &grandchildren, counter);
    }
}

fn is_ancestor(state: &PageState, candidate: NodeId, of: NodeId) -> bool {
    let mut current = Some(candidate);
    let mut guard = 0;
    while let Some(id) = current {
        if id == of {
            return true;
        }
        guard += 1;
        if guard > 512 {
            return true;
        }
        current = state.locations.get(&id).and_then(|register| register.value);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_model::{Node, Transform};

    fn document() -> Document {
        let mut document = Document::new("Collaborative File");
        let page_id = document.active_page_id();
        let page = document
            .page_mut(page_id)
            .expect("new document has an active page");
        let frame = Node::frame("Frame", 0.0, 0.0, 400.0, 300.0);
        let frame_id = frame.id;
        page.insert_node(frame, None, None).unwrap();
        let first = Node::rectangle("First", 10.0, 10.0, 100.0, 40.0);
        page.insert_node(first, Some(frame_id), None).unwrap();
        let second = Node::rectangle("Second", 10.0, 60.0, 100.0, 40.0);
        page.insert_node(second, Some(frame_id), None).unwrap();
        document
    }

    fn actor(n: u8) -> ActorId {
        ActorId::from_uuid(uuid::Uuid::from_bytes([n; 16]))
    }

    #[test]
    fn seed_materializes_identical_document() {
        let document = document();
        let replica = Replica::seed(&document, actor(1));
        let materialized = replica.to_document().unwrap();
        assert_eq!(materialized.name, document.name);
        assert_eq!(materialized.pages.len(), document.pages.len());
        assert_eq!(
            materialized.pages[0].roots, document.pages[0].roots,
            "seed root order must be stable"
        );
        assert_eq!(
            materialized.pages[0].nodes.len(),
            document.pages[0].nodes.len()
        );
    }

    #[test]
    fn concurrent_property_writes_converge() {
        let document = document();
        let page = document.active_page_id();
        let node = document.pages[0].roots[0];
        let mut a = Replica::seed(&document, actor(1));
        let mut b = Replica::seed(&document, actor(2));

        let op_a = a
            .local_set(
                page,
                node,
                NodeField::Name,
                serde_json::json!("Renamed by A"),
            )
            .unwrap();
        let op_b = b
            .local_set(
                page,
                node,
                NodeField::Name,
                serde_json::json!("Renamed by B"),
            )
            .unwrap();

        a.apply(&op_b).unwrap();
        b.apply(&op_a).unwrap();

        let name_a = a.to_document().unwrap().pages[0].nodes[&node].name.clone();
        let name_b = b.to_document().unwrap().pages[0].nodes[&node].name.clone();
        assert_eq!(name_a, name_b, "both replicas must pick the same winner");
    }

    #[test]
    fn concurrent_sibling_inserts_converge() {
        let document = document();
        let page = document.active_page_id();
        let frame = document.pages[0].roots[0];
        let mut a = Replica::seed(&document, actor(1));
        let mut b = Replica::seed(&document, actor(2));

        let node_a = Node::rectangle("A-insert", 0.0, 0.0, 20.0, 20.0);
        let node_b = Node::rectangle("B-insert", 0.0, 30.0, 20.0, 20.0);
        let op_a = a.local_insert(page, node_a, Some(frame), None).unwrap();
        let op_b = b.local_insert(page, node_b, Some(frame), None).unwrap();

        a.apply(&op_b).unwrap();
        b.apply(&op_a).unwrap();

        let children_a = a.to_document().unwrap().pages[0].nodes[&frame]
            .children
            .clone();
        let children_b = b.to_document().unwrap().pages[0].nodes[&frame]
            .children
            .clone();
        assert_eq!(children_a, children_b);
        assert_eq!(children_a.len(), 4);
    }

    #[test]
    fn delete_tombstone_hides_node_and_newer_insert_resurrects() {
        let document = document();
        let page = document.active_page_id();
        let frame = document.pages[0].roots[0];
        let mut a = Replica::seed(&document, actor(1));
        let target = a.pages[&page].children_of(Some(frame))[0];

        a.local_delete(page, target).unwrap();
        assert!(!a.pages[&page].is_alive(target));
        let doc = a.to_document().unwrap();
        assert!(!doc.pages[0].nodes.contains_key(&target));

        let mut b = Replica::seed(&document, actor(2));
        b.local_set(page, target, NodeField::Name, serde_json::json!("Edited"))
            .unwrap();
        let patched = b.to_document().unwrap();
        assert!(patched.pages[0].nodes.contains_key(&target));
    }

    #[test]
    fn stale_writes_are_rejected_and_reported() {
        let document = document();
        let page = document.active_page_id();
        let frame = document.pages[0].roots[0];
        let mut a = Replica::seed(&document, actor(1));
        let first = a
            .local_set(page, frame, NodeField::Opacity, serde_json::json!(0.5))
            .unwrap();
        a.local_set(page, frame, NodeField::Opacity, serde_json::json!(0.75))
            .unwrap();

        let report = a.apply(&first).unwrap();
        assert!(!report.changed);
        assert!(report.stale);
        assert_eq!(
            a.to_document().unwrap().pages[0].nodes[&frame].opacity,
            0.75
        );
    }

    #[test]
    fn flush_drains_pending_ops_into_valid_envelope() {
        let document = document();
        let page = document.active_page_id();
        let frame = document.pages[0].roots[0];
        let mut a = Replica::seed(&document, actor(1));
        assert!(a.flush().unwrap().is_none());
        a.local_set(page, frame, NodeField::Name, serde_json::json!("x"))
            .unwrap();
        let envelope = a.flush().unwrap().unwrap();
        assert_eq!(envelope.ops.len(), 1);
        assert_eq!(envelope.actor, a.actor());
        assert_eq!(envelope.seq, 0);
        assert!(a.flush().unwrap().is_none());
    }

    #[test]
    fn reparent_reorders_and_keeps_node_alive() {
        let document = document();
        let page = document.active_page_id();
        let frame = document.pages[0].roots[0];
        let mut a = Replica::seed(&document, actor(1));
        let moving = a.pages[&page].children_of(Some(frame))[0];
        a.local_reparent(page, moving, None, None).unwrap();
        let doc = a.to_document().unwrap();
        assert_eq!(doc.pages[0].nodes[&moving].parent, None);
        assert!(doc.pages[0].roots.contains(&moving));
    }

    #[test]
    fn reparent_into_descendant_is_rejected() {
        let document = document();
        let page = document.active_page_id();
        let frame = document.pages[0].roots[0];
        let mut a = Replica::seed(&document, actor(1));
        let child = a.pages[&page].children_of(Some(frame))[0];
        assert!(a.local_reparent(page, frame, Some(child), None).is_err());
    }

    #[test]
    fn transform_registers_roundtrip() {
        let document = document();
        let page = document.active_page_id();
        let node = document.pages[0].roots[0];
        let mut replica = Replica::seed(&document, actor(1));
        replica
            .local_set(
                page,
                node,
                NodeField::Transform,
                serde_json::to_value(Transform::new(5.0, 6.0, 70.0, 80.0)).unwrap(),
            )
            .unwrap();
        let materialized = replica.to_document().unwrap();
        assert_eq!(materialized.pages[0].nodes[&node].transform.w, 70.0);
    }
}
