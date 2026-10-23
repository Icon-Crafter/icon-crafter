//! Pages: the flat node store and tree operations.
//!
//! A page owns an `IndexMap<NodeId, Node>` plus an ordered list of root node
//! ids. Nodes store their children ids explicitly, so the tree order (and
//! therefore paint order, first = bottom) is stable and cheap to serialize.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::color::Color;
use crate::ids::{NodeId, PageId};
use crate::node::Node;
use dp_geom::{Mat2D, Rect};

/// Errors from page level tree operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PageError {
    /// A node with this id already exists in the page.
    #[error("node {0} already exists in the page")]
    DuplicateNode(NodeId),
    /// A referenced node does not exist in the page.
    #[error("node {0} does not exist in the page")]
    MissingNode(NodeId),
    /// A parent node does not exist in the page.
    #[error("parent node {0} does not exist in the page")]
    MissingParent(NodeId),
    /// The parent cannot contain children of this kind.
    #[error("node {0} is not a container and cannot have children")]
    NonContainerParent(NodeId),
    /// The operation would create a cycle in the node tree.
    #[error("the operation would create a cycle at node {0}")]
    Cycle(NodeId),
}

/// A single page of a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    /// Page id.
    pub id: PageId,
    /// Page name.
    pub name: String,
    /// Canvas background colour.
    #[serde(default = "default_background")]
    pub background: Color,
    /// All nodes on the page, keyed by id.
    #[serde(default)]
    pub nodes: IndexMap<NodeId, Node>,
    /// Root nodes in paint order (first = bottom).
    #[serde(default)]
    pub roots: Vec<NodeId>,
}

fn default_background() -> Color {
    Color::WHITE
}

impl Page {
    /// Creates an empty page.
    pub fn new(name: impl Into<String>) -> Page {
        Page {
            id: PageId::new_v4(),
            name: name.into(),
            background: Color::WHITE,
            nodes: IndexMap::new(),
            roots: Vec::new(),
        }
    }

    /// Creates an empty page with a specific id.
    pub fn with_id(id: PageId, name: impl Into<String>) -> Page {
        Page {
            id,
            name: name.into(),
            background: Color::WHITE,
            nodes: IndexMap::new(),
            roots: Vec::new(),
        }
    }

    /// Number of nodes on the page.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the page has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Root node ids.
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Looks up a node.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Mutably looks up a node.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Whether the page contains a node.
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    /// Inserts a node under `parent` at `index` (clamped), or as a root when
    /// `parent` is `None`.
    pub fn insert_node(
        &mut self,
        node: Node,
        parent: Option<NodeId>,
        index: Option<usize>,
    ) -> Result<(), PageError> {
        let id = node.id;
        if self.nodes.contains_key(&id) {
            return Err(PageError::DuplicateNode(id));
        }
        if let Some(parent_id) = parent {
            let parent_node = self
                .nodes
                .get(&parent_id)
                .ok_or(PageError::MissingParent(parent_id))?;
            if !parent_node.kind.is_container() {
                return Err(PageError::NonContainerParent(parent_id));
            }
        }
        let mut node = node;
        node.parent = parent;
        let index = index.unwrap_or(usize::MAX);
        self.nodes.insert(id, node);
        match parent {
            Some(parent_id) => {
                if let Some(parent_node) = self.nodes.get_mut(&parent_id) {
                    let at = index.min(parent_node.children.len());
                    parent_node.children.insert(at, id);
                }
            }
            None => {
                let at = index.min(self.roots.len());
                self.roots.insert(at, id);
            }
        }
        Ok(())
    }

    /// Removes a single node (its children stay in the map but become orphaned;
    /// use [`Page::remove_subtree`] for tree-safe deletion).
    pub fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        let node = self.nodes.shift_remove(&id)?;
        match node.parent {
            Some(parent_id) => {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|child| *child != id);
                }
            }
            None => {
                self.roots.retain(|root| *root != id);
            }
        }
        Some(node)
    }

    /// Removes a node and all of its descendants. Returns the removed nodes in
    /// pre-order (parent before children).
    pub fn remove_subtree(&mut self, id: NodeId) -> Vec<Node> {
        let ids = self.subtree_ids(id);
        if ids.is_empty() {
            return Vec::new();
        }
        match self.nodes.get(&id).and_then(|node| node.parent) {
            Some(parent_id) => {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|child| *child != id);
                }
            }
            None => self.roots.retain(|root| *root != id),
        }
        let mut removed = Vec::with_capacity(ids.len());
        for node_id in ids {
            if let Some(node) = self.nodes.shift_remove(&node_id) {
                removed.push(node);
            }
        }
        removed
    }

    /// Ids of a subtree in pre-order (root first). Empty when `id` is missing.
    pub fn subtree_ids(&self, id: NodeId) -> Vec<NodeId> {
        if !self.nodes.contains_key(&id) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut stack = vec![id];
        let mut guard = 0usize;
        while let Some(current) = stack.pop() {
            guard += 1;
            if guard > 2_000_000 {
                // Corrupt tree guard: never hang the UI on a cycle.
                break;
            }
            if out.contains(&current) {
                continue;
            }
            out.push(current);
            if let Some(node) = self.nodes.get(&current) {
                for child in node.children.iter().rev() {
                    stack.push(*child);
                }
            }
        }
        out
    }

    /// Direct children of a node.
    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.nodes.get(&id).map(|node| node.children.as_slice())
    }

    /// Position of a node inside its parent (or the root list).
    pub fn index_in_parent(&self, id: NodeId) -> Option<(Option<NodeId>, usize)> {
        let node = self.nodes.get(&id)?;
        match node.parent {
            Some(parent_id) => {
                let parent = self.nodes.get(&parent_id)?;
                let index = parent.children.iter().position(|child| *child == id)?;
                Some((Some(parent_id), index))
            }
            None => {
                let index = self.roots.iter().position(|root| *root == id)?;
                Some((None, index))
            }
        }
    }

    /// Moves a node to a new parent/index. Prevents cycles.
    pub fn reorder(
        &mut self,
        id: NodeId,
        new_parent: Option<NodeId>,
        new_index: Option<usize>,
    ) -> Result<(), PageError> {
        if !self.nodes.contains_key(&id) {
            return Err(PageError::MissingNode(id));
        }
        if let Some(parent_id) = new_parent {
            if parent_id == id || self.subtree_ids(id).contains(&parent_id) {
                return Err(PageError::Cycle(id));
            }
            let parent = self
                .nodes
                .get(&parent_id)
                .ok_or(PageError::MissingParent(parent_id))?;
            if !parent.kind.is_container() {
                return Err(PageError::NonContainerParent(parent_id));
            }
        }
        // Detach from the current parent.
        let old_parent = self.nodes.get(&id).and_then(|node| node.parent);
        match old_parent {
            Some(parent_id) => {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    parent.children.retain(|child| *child != id);
                }
            }
            None => self.roots.retain(|root| *root != id),
        }
        // Attach at the new position.
        if let Some(node) = self.nodes.get_mut(&id) {
            node.parent = new_parent;
        }
        let index = new_index.unwrap_or(usize::MAX);
        match new_parent {
            Some(parent_id) => {
                if let Some(parent) = self.nodes.get_mut(&parent_id) {
                    let at = index.min(parent.children.len());
                    parent.children.insert(at, id);
                }
            }
            None => {
                let at = index.min(self.roots.len());
                self.roots.insert(at, id);
            }
        }
        Ok(())
    }

    /// Accumulated local-to-page matrix for a node.
    pub fn absolute_matrix(&self, id: NodeId) -> Option<Mat2D> {
        let mut chain: Vec<NodeId> = Vec::new();
        let mut current = Some(id);
        let mut guard = 0usize;
        while let Some(node_id) = current {
            guard += 1;
            if guard > 512 {
                return None;
            }
            let node = self.nodes.get(&node_id)?;
            chain.push(node_id);
            current = node.parent;
        }
        let mut matrix = Mat2D::IDENTITY;
        for node_id in chain.iter().rev() {
            let node = self.nodes.get(node_id)?;
            matrix = matrix.mul(node.transform.local_matrix());
        }
        Some(matrix)
    }

    /// Axis aligned page-space bounds of a node's (rotated) box.
    pub fn world_bounds(&self, id: NodeId) -> Option<Rect> {
        let node = self.nodes.get(&id)?;
        let matrix = self.absolute_matrix(id)?;
        Some(matrix.apply_rect(Rect::new(0.0, 0.0, node.transform.w, node.transform.h)))
    }

    /// Depth of a node (0 for roots). `None` when the parent chain is broken.
    pub fn depth(&self, id: NodeId) -> Option<usize> {
        let mut depth = 0usize;
        let mut current = self.nodes.get(&id)?.parent;
        while let Some(parent_id) = current {
            depth += 1;
            if depth > 512 {
                return None;
            }
            current = self.nodes.get(&parent_id)?.parent;
        }
        Some(depth)
    }

    /// Visits a subtree in pre-order.
    pub fn walk_preorder(&self, id: NodeId, visit: &mut impl FnMut(&Node)) {
        for node_id in self.subtree_ids(id) {
            if let Some(node) = self.nodes.get(&node_id) {
                visit(node);
            }
        }
    }

    /// Checks that parent/children links are consistent and acyclic.
    pub fn check_integrity(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let mut seen_roots = std::collections::HashSet::new();
        for root in &self.roots {
            if !seen_roots.insert(*root) {
                issues.push(format!("root {root} appears multiple times"));
            }
            match self.nodes.get(root) {
                Some(node) => {
                    if node.parent.is_some() {
                        issues.push(format!("root {root} has a parent"));
                    }
                }
                None => issues.push(format!("root {root} is missing from the node map")),
            }
        }
        for (id, node) in &self.nodes {
            for child in &node.children {
                match self.nodes.get(child) {
                    Some(child_node) => {
                        if child_node.parent != Some(*id) {
                            issues
                                .push(format!("child {child} does not point back to parent {id}"));
                        }
                    }
                    None => issues.push(format!("node {id} references missing child {child}")),
                }
            }
            match node.parent {
                Some(parent_id) => match self.nodes.get(&parent_id) {
                    Some(parent) => {
                        if !parent.children.contains(id) {
                            issues.push(format!("node {id} is not listed by parent {parent_id}"));
                        }
                    }
                    None => issues.push(format!("node {id} references missing parent {parent_id}")),
                },
                None => {
                    if !self.roots.contains(id) {
                        issues.push(format!("node {id} has no parent and is not a root"));
                    }
                }
            }
        }
        // Cycle detection.
        for id in self.nodes.keys() {
            let mut slow = Some(*id);
            let mut fast = Some(*id);
            loop {
                slow = slow
                    .and_then(|node| self.nodes.get(&node))
                    .and_then(|node| node.parent);
                fast = fast
                    .and_then(|node| self.nodes.get(&node))
                    .and_then(|node| node.parent)
                    .and_then(|node| self.nodes.get(&node))
                    .and_then(|node| node.parent);
                if slow.is_none() && fast.is_none() {
                    break;
                }
                if slow.is_some() && slow == fast {
                    issues.push(format!("cycle detected involving node {id}"));
                    break;
                }
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{NodeKind, Transform};

    fn page_with_nodes() -> (Page, NodeId, NodeId) {
        let mut page = Page::new("Page 1");
        let frame = Node::frame("Frame", 0.0, 0.0, 400.0, 300.0);
        let frame_id = frame.id;
        page.insert_node(frame, None, None).unwrap();
        let rect = Node::rectangle("Rect", 10.0, 10.0, 100.0, 50.0);
        let rect_id = rect.id;
        page.insert_node(rect, Some(frame_id), None).unwrap();
        (page, frame_id, rect_id)
    }

    #[test]
    fn insert_get_and_indexes() {
        let (page, frame_id, rect_id) = page_with_nodes();
        assert_eq!(page.len(), 2);
        assert_eq!(page.roots(), &[frame_id]);
        assert_eq!(page.get(rect_id).unwrap().parent, Some(frame_id));
        assert_eq!(page.index_in_parent(rect_id), Some((Some(frame_id), 0)));
        assert_eq!(page.children(frame_id).unwrap().len(), 1);
    }

    #[test]
    fn duplicate_ids_rejected() {
        let (mut page, frame_id, _) = page_with_nodes();
        let duplicate = Node::with_id(
            frame_id,
            NodeKind::Rectangle,
            "Dupe",
            Transform::new(0.0, 0.0, 1.0, 1.0),
        );
        assert_eq!(
            page.insert_node(duplicate, None, None),
            Err(PageError::DuplicateNode(frame_id))
        );
    }

    #[test]
    fn non_container_parent_rejected() {
        let (mut page, _, rect_id) = page_with_nodes();
        let child = Node::rectangle("Child", 0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            page.insert_node(child, Some(rect_id), None),
            Err(PageError::NonContainerParent(rect_id))
        );
    }

    #[test]
    fn subtree_removal_returns_preorder() {
        let (mut page, frame_id, rect_id) = page_with_nodes();
        let removed = page.remove_subtree(frame_id);
        assert_eq!(removed.len(), 2);
        assert_eq!(removed[0].id, frame_id);
        assert_eq!(removed[1].id, rect_id);
        assert!(page.is_empty());
        assert!(page.roots().is_empty());
    }

    #[test]
    fn reorder_moves_and_prevents_cycles() {
        let (mut page, frame_id, rect_id) = page_with_nodes();
        let second = Node::rectangle("Second", 200.0, 10.0, 50.0, 50.0);
        let second_id = second.id;
        page.insert_node(second, Some(frame_id), None).unwrap();
        page.reorder(rect_id, Some(frame_id), Some(1)).unwrap();
        assert_eq!(page.children(frame_id).unwrap(), &[second_id, rect_id]);
        assert_eq!(
            page.reorder(frame_id, Some(rect_id), None),
            Err(PageError::Cycle(frame_id))
        );
        page.reorder(rect_id, None, None).unwrap();
        assert_eq!(page.roots(), &[frame_id, rect_id]);
    }

    #[test]
    fn absolute_matrix_composes_parent_transforms() {
        let (page, frame_id, rect_id) = page_with_nodes();
        page.get(frame_id).unwrap();
        let matrix = page.absolute_matrix(rect_id).unwrap();
        let world = matrix.apply_point(dp_geom::Point::new(0.0, 0.0));
        assert!((world.x - 10.0).abs() < 1e-9);
        assert!((world.y - 10.0).abs() < 1e-9);
        let bounds = page.world_bounds(rect_id).unwrap();
        assert!((bounds.x - 10.0).abs() < 1e-9);
        assert!((bounds.w - 100.0).abs() < 1e-9);
    }

    #[test]
    fn integrity_check_flags_broken_links() {
        let (mut page, frame_id, rect_id) = page_with_nodes();
        assert!(page.check_integrity().is_empty());
        page.get_mut(frame_id).unwrap().children.clear();
        let issues = page.check_integrity();
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("not listed by parent"))
        );
        assert!(page.get(rect_id).is_some());
    }
}
