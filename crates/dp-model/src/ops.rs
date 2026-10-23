//! Document operations: transactions for the commands the editor exposes.
//!
//! These functions never mutate the document themselves; they produce a
//! [`Transaction`] which [`apply_transaction`] applies and records in the
//! [`History`]. That keeps the command layer, undo, collaboration and the
//! prototype runtime on the same edit path.

use std::collections::HashMap;

use dp_geom::{Mat2D, Rect};
use serde::{Deserialize, Serialize};

use crate::ModelError;
use crate::document::Document;
use crate::geometry::NodeGeometry;
use crate::ids::{NodeId, PageId};
use crate::node::{Node, NodeKind, Transform};
use crate::page::Page;
use crate::transactions::{Edit, History, NodePatch, Transaction};

/// Alignment target for the align commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    /// Align left edges.
    Left,
    /// Align horizontal centres.
    HCenter,
    /// Align right edges.
    Right,
    /// Align top edges.
    Top,
    /// Align vertical centres.
    VCenter,
    /// Align bottom edges.
    Bottom,
}

/// Axis for the distribute command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributeAxis {
    /// Distribute horizontally.
    Horizontal,
    /// Distribute vertically.
    Vertical,
}

/// Applies a transaction and commits it to the undo history.
///
/// If an edit fails midway the already applied edits are reverted before the
/// error is returned, so a failed command never leaves a half-applied document.
pub fn apply_transaction(
    document: &mut Document,
    history: &mut History,
    transaction: Transaction,
) -> Result<Transaction, ModelError> {
    let mut applied = 0usize;
    for edit in &transaction.edits {
        match edit.apply(document) {
            Ok(()) => applied += 1,
            Err(error) => {
                for rollback in transaction.edits[..applied].iter().rev() {
                    let _ = rollback.revert(document);
                }
                return Err(error);
            }
        }
    }
    document.touch();
    history.push(transaction.clone());
    Ok(transaction)
}

/// Creates a transaction inserting nodes into a page.
pub fn insert_nodes(
    document: &Document,
    page: PageId,
    inserts: Vec<(Node, Option<NodeId>, Option<usize>)>,
    label: impl Into<String>,
) -> Transaction {
    let mut transaction = Transaction::new(label);
    if document.page(page).is_none() {
        return transaction;
    }
    for (node, parent, index) in inserts {
        let parent_exists = match parent {
            Some(id) => document
                .page(page)
                .and_then(|page| page.get(id))
                .is_some_and(|node| node.kind.is_container()),
            None => true,
        };
        if !parent_exists {
            continue;
        }
        let index = index.unwrap_or_else(|| {
            document
                .page(page)
                .map(|page| match parent {
                    Some(id) => page.get(id).map(|node| node.children.len()).unwrap_or(0),
                    None => page.roots().len(),
                })
                .unwrap_or(0)
        });
        transaction.push(Edit::InsertNode {
            page,
            node,
            parent,
            index,
        });
    }
    transaction
}

/// Produces a transaction patching one or more nodes.
pub fn patch_nodes(
    document: &Document,
    ids: &[NodeId],
    patch: NodePatch,
    label: impl Into<String>,
    coalesce_key: Option<&str>,
) -> Result<Transaction, ModelError> {
    if patch.is_empty() {
        return Err(ModelError::InvalidDocument("empty patch".to_string()));
    }
    let mut transaction = Transaction::new(label);
    if let Some(key) = coalesce_key {
        transaction.coalesce_key = Some(key.to_string());
    }
    for id in ids {
        let Some((page, node)) = document.locate_node(*id) else {
            return Err(ModelError::NodeNotFound(*id));
        };
        let before = node.clone();
        let mut after = before.clone();
        patch.apply_to(&mut after);
        after.rev = after.rev.saturating_add(1);
        transaction.push(Edit::SetNode {
            page: page.id,
            before,
            after,
        });
    }
    Ok(transaction)
}

/// Produces a transaction nudge (arrow-key move), coalescing repeated nudges.
pub fn nudge(
    document: &Document,
    ids: &[NodeId],
    dx: f64,
    dy: f64,
    label: impl Into<String>,
    coalesce_key: Option<&str>,
) -> Result<Transaction, ModelError> {
    let mut transaction = Transaction::new(label);
    transaction.coalesce_key = coalesce_key.map(str::to_string);
    for id in ids {
        let Some((page, node)) = document.locate_node(*id) else {
            return Err(ModelError::NodeNotFound(*id));
        };
        let before = node.clone();
        let mut after = before.clone();
        after.transform.x += dx;
        after.transform.y += dy;
        after.rev = after.rev.saturating_add(1);
        transaction.push(Edit::SetNode {
            page: page.id,
            before,
            after,
        });
    }
    Ok(transaction)
}

/// Produces a transaction removing nodes (and their subtrees).
pub fn delete_nodes(
    document: &Document,
    ids: &[NodeId],
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let mut transaction = Transaction::new(label);
    let roots = top_level_selection(document, ids);
    for root in roots {
        let Some(page_id) = document.page_containing(root) else {
            continue;
        };
        let page = document.page(page_id).expect("page exists");
        let subtree = page.subtree_ids(root);
        // Children first so parents are leaf nodes when they are removed.
        for node_id in subtree.iter().rev() {
            let Some(node) = page.get(*node_id) else {
                continue;
            };
            let (parent, index) = page.index_in_parent(*node_id).unwrap_or((node.parent, 0));
            transaction.push(Edit::RemoveNode {
                page: page_id,
                node: node.clone(),
                parent,
                index,
            });
        }
    }
    Ok(transaction)
}

/// Produces a transaction duplicating subtrees (offset by 10 units).
pub fn duplicate_nodes(
    document: &Document,
    ids: &[NodeId],
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let mut transaction = Transaction::new(label);
    let roots = top_level_selection(document, ids);
    // Process by descending original position so insertion indices stay valid.
    let mut ordered: Vec<(PageId, Option<NodeId>, usize, NodeId)> = Vec::new();
    for root in roots {
        let Some(page_id) = document.page_containing(root) else {
            continue;
        };
        let page = document.page(page_id).expect("page exists");
        if let Some((parent, index)) = page.index_in_parent(root) {
            ordered.push((page_id, parent, index, root));
        }
    }
    ordered.sort_by(|a, b| {
        (a.0.to_string(), format!("{:?}", a.1), a.2)
            .cmp(&(b.0.to_string(), format!("{:?}", b.1), b.2))
            .reverse()
    });
    for (page_id, _parent, _index, root) in ordered {
        let page = document.page(page_id).expect("page exists");
        let Some(root_node) = page.get(root) else {
            continue;
        };
        let subtree = page.subtree_ids(root);
        let mut map: HashMap<NodeId, NodeId> = HashMap::new();
        for id in &subtree {
            map.insert(*id, NodeId::new_v4());
        }
        let root_parent = root_node.parent;
        let root_index = page
            .index_in_parent(root)
            .map(|(_, index)| index)
            .unwrap_or(0);
        let mut new_root_id = None;
        for id in &subtree {
            let Some(original) = page.get(*id) else {
                continue;
            };
            let mut copy = original.clone();
            copy.id = map[id];
            copy.rev = 1;
            // Children are rebuilt by Page::insert_node as the subtree is
            // inserted in pre-order; pre-populating them would duplicate the
            // links.
            copy.children.clear();
            if *id == root {
                copy.parent = root_parent;
                copy.transform.x += 10.0;
                copy.transform.y += 10.0;
                new_root_id = Some(copy.id);
                transaction.push(Edit::InsertNode {
                    page: page_id,
                    node: copy,
                    parent: root_parent,
                    index: root_index + 1,
                });
            } else {
                copy.parent = original.parent.and_then(|parent| map.get(&parent).copied());
                let parent = copy.parent;
                transaction.push(Edit::InsertNode {
                    page: page_id,
                    node: copy,
                    parent,
                    index: usize::MAX,
                });
            }
        }
        if let Some(new_root) = new_root_id {
            transaction.selection_after.push(new_root);
        }
    }
    if transaction.selection_after.is_empty() {
        return Err(ModelError::InvalidDocument(
            "nothing to duplicate".to_string(),
        ));
    }
    Ok(transaction)
}

/// Produces a transaction moving a node to a new parent/index.
pub fn reorder(
    document: &Document,
    id: NodeId,
    new_parent: Option<NodeId>,
    new_index: Option<usize>,
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let (page, _node) = document
        .locate_node(id)
        .ok_or(ModelError::NodeNotFound(id))?;
    let page_id = page.id;
    let (old_parent, old_index) = page
        .index_in_parent(id)
        .ok_or(ModelError::NodeNotFound(id))?;
    if new_parent == Some(id) {
        return Err(ModelError::InvalidDocument(
            "a node cannot be its own parent".to_string(),
        ));
    }
    let mut transaction = Transaction::new(label);
    let new_index = new_index.unwrap_or_else(|| match new_parent {
        Some(parent) => page
            .get(parent)
            .map(|node| node.children.len())
            .unwrap_or(0),
        None => page.roots().len(),
    });
    transaction.push(Edit::Reparent {
        page: page_id,
        node_id: id,
        old_parent,
        old_index,
        new_parent,
        new_index,
    });
    transaction.selection_after = vec![id];
    Ok(transaction)
}

/// Produces a transaction grouping nodes into a new group node.
pub fn group_nodes(
    document: &Document,
    ids: &[NodeId],
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let roots = top_level_selection(document, ids);
    if roots.len() < 2 {
        return Err(ModelError::InvalidDocument(
            "group needs at least two nodes".to_string(),
        ));
    }
    let page_id = document
        .page_containing(roots[0])
        .ok_or(ModelError::NodeNotFound(roots[0]))?;
    for id in &roots {
        if document.page_containing(*id) != Some(page_id) {
            return Err(ModelError::InvalidDocument(
                "cannot group nodes across pages".to_string(),
            ));
        }
    }
    let page = document.page(page_id).expect("page exists");
    let parents: Vec<Option<NodeId>> = roots
        .iter()
        .filter_map(|id| page.get(*id).map(|node| node.parent))
        .collect();
    let shared_parent = if parents.windows(2).all(|pair| pair[0] == pair[1]) {
        parents.first().copied().flatten()
    } else {
        None
    };
    let bounds = bounds_in_space(page, shared_parent, &roots)
        .ok_or_else(|| ModelError::InvalidDocument("cannot compute group bounds".to_string()))?;
    if !bounds.is_finite() || bounds.w <= 0.0 || bounds.h <= 0.0 {
        return Err(ModelError::InvalidDocument(
            "group bounds are degenerate".to_string(),
        ));
    }
    let mut group = Node::new(
        NodeKind::Group,
        "Group",
        Transform::new(bounds.x, bounds.y, bounds.w, bounds.h),
    );
    group.children = Vec::new();
    let group_id = group.id;
    let insert_index = match shared_parent {
        Some(parent) => {
            let parent_node = page.get(parent).expect("parent exists");
            roots
                .iter()
                .filter_map(|id| parent_node.children.iter().position(|child| child == id))
                .min()
                .unwrap_or(parent_node.children.len())
        }
        None => roots
            .iter()
            .filter_map(|id| page.roots().iter().position(|root| root == id))
            .min()
            .unwrap_or(page.roots().len()),
    };
    let mut transaction = Transaction::new(label);
    transaction.push(Edit::InsertNode {
        page: page_id,
        node: group,
        parent: shared_parent,
        index: insert_index,
    });
    transaction.selection_after = vec![group_id];

    let parent_world = match shared_parent {
        Some(parent) => page
            .absolute_matrix(parent)
            .ok_or_else(|| ModelError::InvalidDocument("missing parent matrix".to_string()))?,
        None => Mat2D::IDENTITY,
    };
    let group_world =
        parent_world.mul(Transform::new(bounds.x, bounds.y, bounds.w, bounds.h).matrix());
    let inverse = group_world.inverse().ok_or_else(|| {
        ModelError::InvalidDocument("group transform is not invertible".to_string())
    })?;
    let mut ordered: Vec<(usize, NodeId)> = Vec::new();
    for id in &roots {
        if let Some((_, index)) = page.index_in_parent(*id) {
            ordered.push((index, *id));
        }
    }
    ordered.sort_by_key(|(index, _)| *index);
    for (_, id) in ordered {
        let Some(node) = page.get(id) else {
            continue;
        };
        let (old_parent, old_index) = page
            .index_in_parent(id)
            .ok_or(ModelError::NodeNotFound(id))?;
        let world = page.absolute_matrix(id).ok_or(ModelError::InvalidDocument(
            "missing node matrix".to_string(),
        ))?;
        let local = inverse.mul(world);
        let mut after = node.clone();
        after.parent = Some(group_id);
        after.transform = transform_from_matrix(local, node.transform.w, node.transform.h);
        after.rev = after.rev.saturating_add(1);
        transaction.push(Edit::Reparent {
            page: page_id,
            node_id: id,
            old_parent,
            old_index,
            new_parent: Some(group_id),
            new_index: usize::MAX,
        });
        transaction.push(Edit::SetNode {
            page: page_id,
            before: node.clone(),
            after,
        });
    }
    Ok(transaction)
}

/// Produces a transaction ungrouping a group node.
pub fn ungroup(
    document: &Document,
    group_id: NodeId,
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let (page, group) = document
        .locate_node(group_id)
        .ok_or(ModelError::NodeNotFound(group_id))?;
    if group.kind != NodeKind::Group {
        return Err(ModelError::InvalidDocument(
            "node is not a group".to_string(),
        ));
    }
    let page_id = page.id;
    let (new_parent, group_index) = page
        .index_in_parent(group_id)
        .ok_or(ModelError::NodeNotFound(group_id))?;
    let mut transaction = Transaction::new(label);
    let children = group.children.clone();
    for (offset, child_id) in children.iter().enumerate() {
        let Some(child) = page.get(*child_id) else {
            continue;
        };
        let combined = group.transform.matrix().mul(child.transform.matrix());
        let mut after = child.clone();
        after.parent = new_parent;
        after.transform = transform_from_matrix(combined, child.transform.w, child.transform.h);
        after.rev = after.rev.saturating_add(1);
        transaction.push(Edit::Reparent {
            page: page_id,
            node_id: *child_id,
            old_parent: Some(group_id),
            old_index: offset,
            new_parent,
            new_index: group_index + offset,
        });
        transaction.push(Edit::SetNode {
            page: page_id,
            before: child.clone(),
            after,
        });
        transaction.selection_after.push(*child_id);
    }
    let mut group_snapshot = group.clone();
    group_snapshot.children.clear();
    transaction.selection_before = vec![group_id];
    transaction.push(Edit::RemoveNode {
        page: page_id,
        node: group_snapshot,
        parent: new_parent,
        index: group_index,
    });
    Ok(transaction)
}

/// Produces a transaction wrapping selected nodes in a boolean node.
///
/// Operands stay as children of the boolean node (their outlines are evaluated
/// by the renderer), so the result remains editable until flattened.
pub fn boolean_nodes(
    document: &Document,
    ids: &[NodeId],
    operation: dp_geom::BooleanOp,
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let roots = top_level_selection(document, ids);
    if roots.len() < 2 {
        return Err(ModelError::InvalidDocument(
            "boolean operations need at least two nodes".to_string(),
        ));
    }
    let page_id = document
        .page_containing(roots[0])
        .ok_or(ModelError::NodeNotFound(roots[0]))?;
    let page = document.page(page_id).expect("page exists");
    let parents: Vec<Option<NodeId>> = roots
        .iter()
        .filter_map(|id| page.get(*id).map(|node| node.parent))
        .collect();
    if !parents.windows(2).all(|pair| pair[0] == pair[1]) {
        return Err(ModelError::InvalidDocument(
            "boolean operands must share a parent".to_string(),
        ));
    }
    let shared_parent = parents.first().copied().flatten();
    let bounds = bounds_in_space(page, shared_parent, &roots)
        .ok_or_else(|| ModelError::InvalidDocument("cannot compute bounds".to_string()))?;
    let mut node = Node::new(
        NodeKind::Boolean,
        "Boolean",
        Transform::new(bounds.x, bounds.y, bounds.w, bounds.h),
    );
    node.geometry = Some(NodeGeometry::Boolean {
        operation,
        fill_rule: crate::geometry::FillRule::NonZero,
    });
    let boolean_id = node.id;
    let insert_index = match shared_parent {
        Some(parent) => {
            let parent_node = page.get(parent).expect("parent exists");
            roots
                .iter()
                .filter_map(|id| parent_node.children.iter().position(|child| child == id))
                .min()
                .unwrap_or(parent_node.children.len())
        }
        None => roots
            .iter()
            .filter_map(|id| page.roots().iter().position(|root| root == id))
            .min()
            .unwrap_or(page.roots().len()),
    };
    let mut transaction = Transaction::new(label);
    transaction.push(Edit::InsertNode {
        page: page_id,
        node,
        parent: shared_parent,
        index: insert_index,
    });
    transaction.selection_after = vec![boolean_id];
    let parent_world = match shared_parent {
        Some(parent) => page
            .absolute_matrix(parent)
            .ok_or_else(|| ModelError::InvalidDocument("missing parent matrix".to_string()))?,
        None => Mat2D::IDENTITY,
    };
    let boolean_world =
        parent_world.mul(Transform::new(bounds.x, bounds.y, bounds.w, bounds.h).matrix());
    let inverse = boolean_world.inverse().ok_or_else(|| {
        ModelError::InvalidDocument("boolean transform is not invertible".to_string())
    })?;
    let mut ordered: Vec<(usize, NodeId)> = Vec::new();
    for id in &roots {
        if let Some((_, index)) = page.index_in_parent(*id) {
            ordered.push((index, *id));
        }
    }
    ordered.sort_by_key(|(index, _)| *index);
    for (_, id) in ordered {
        let Some(child) = page.get(id) else {
            continue;
        };
        let (old_parent, old_index) = page
            .index_in_parent(id)
            .ok_or(ModelError::NodeNotFound(id))?;
        let world = page.absolute_matrix(id).ok_or(ModelError::InvalidDocument(
            "missing node matrix".to_string(),
        ))?;
        let local = inverse.mul(world);
        let mut after = child.clone();
        after.parent = Some(boolean_id);
        after.transform = transform_from_matrix(local, child.transform.w, child.transform.h);
        after.rev = after.rev.saturating_add(1);
        transaction.push(Edit::Reparent {
            page: page_id,
            node_id: id,
            old_parent,
            old_index,
            new_parent: Some(boolean_id),
            new_index: usize::MAX,
        });
        transaction.push(Edit::SetNode {
            page: page_id,
            before: child.clone(),
            after,
        });
    }
    Ok(transaction)
}

/// Produces a transaction aligning nodes against the selection bounds.
pub fn align_nodes(
    document: &Document,
    ids: &[NodeId],
    align: Align,
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let nodes: Vec<(PageId, NodeId, Rect, Mat2D)> = ids
        .iter()
        .filter_map(|id| {
            let page_id = document.page_containing(*id)?;
            let page = document.page(page_id)?;
            let world = page.world_bounds(*id)?;
            let parent_matrix = parent_world_matrix(page, *id)?;
            Some((page_id, *id, world, parent_matrix))
        })
        .collect();
    if nodes.len() < 2 {
        return Err(ModelError::InvalidDocument(
            "align needs at least two nodes".to_string(),
        ));
    }
    let mut selection = nodes[0].2;
    for (_, _, bounds, _) in &nodes[1..] {
        selection = union_rect(selection, *bounds);
    }
    let mut transaction = Transaction::new(label);
    for (page_id, id, world, parent_matrix) in &nodes {
        let target = match align {
            Align::Left => selection.x,
            Align::HCenter => selection.x + selection.w / 2.0,
            Align::Right => selection.right(),
            Align::Top => selection.y,
            Align::VCenter => selection.y + selection.h / 2.0,
            Align::Bottom => selection.bottom(),
        };
        let current = match align {
            Align::Left => world.x,
            Align::HCenter => world.x + world.w / 2.0,
            Align::Right => world.right(),
            Align::Top => world.y,
            Align::VCenter => world.y + world.h / 2.0,
            Align::Bottom => world.bottom(),
        };
        let delta_world = match align {
            Align::Left | Align::HCenter | Align::Right => {
                dp_geom::Point::new(target - current, 0.0)
            }
            Align::Top | Align::VCenter | Align::Bottom => {
                dp_geom::Point::new(0.0, target - current)
            }
        };
        let inverse = parent_matrix
            .inverse()
            .ok_or_else(|| ModelError::InvalidDocument("singular parent matrix".to_string()))?;
        let delta = inverse.apply_vector(delta_world);
        let Some(page) = document.page(*page_id) else {
            continue;
        };
        let Some(node) = page.get(*id) else {
            continue;
        };
        let before = node.clone();
        let mut after = before.clone();
        after.transform.x += delta.x;
        after.transform.y += delta.y;
        after.rev = after.rev.saturating_add(1);
        transaction.push(Edit::SetNode {
            page: *page_id,
            before,
            after,
        });
    }
    Ok(transaction)
}

/// Produces a transaction distributing nodes along an axis.
pub fn distribute_nodes(
    document: &Document,
    ids: &[NodeId],
    axis: DistributeAxis,
    spacing: f64,
    label: impl Into<String>,
) -> Result<Transaction, ModelError> {
    let mut nodes: Vec<(PageId, NodeId, Rect, Mat2D, f64)> = Vec::new();
    for id in ids {
        let Some(page_id) = document.page_containing(*id) else {
            continue;
        };
        let Some(page) = document.page(page_id) else {
            continue;
        };
        let Some(world) = page.world_bounds(*id) else {
            continue;
        };
        let Some(parent_matrix) = parent_world_matrix(page, *id) else {
            continue;
        };
        let start = match axis {
            DistributeAxis::Horizontal => world.x,
            DistributeAxis::Vertical => world.y,
        };
        nodes.push((page_id, *id, world, parent_matrix, start));
    }
    if nodes.len() < 3 {
        return Err(ModelError::InvalidDocument(
            "distribute needs at least three nodes".to_string(),
        ));
    }
    nodes.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(std::cmp::Ordering::Equal));
    let first = nodes.first().unwrap();
    let last = nodes.last().unwrap();
    let span = match axis {
        DistributeAxis::Horizontal => last.2.right() - first.2.x,
        DistributeAxis::Vertical => last.2.bottom() - first.2.y,
    };
    let sizes: f64 = nodes
        .iter()
        .map(|node| match axis {
            DistributeAxis::Horizontal => node.2.w,
            DistributeAxis::Vertical => node.2.h,
        })
        .sum();
    let gap = if spacing > 0.0 {
        spacing
    } else {
        (span - sizes) / (nodes.len() as f64 - 1.0)
    };
    let mut transaction = Transaction::new(label);
    let mut cursor = match axis {
        DistributeAxis::Horizontal => first.2.x,
        DistributeAxis::Vertical => first.2.y,
    };
    for (page_id, id, world, parent_matrix, _) in &nodes {
        let current = match axis {
            DistributeAxis::Horizontal => world.x,
            DistributeAxis::Vertical => world.y,
        };
        let delta_world = match axis {
            DistributeAxis::Horizontal => dp_geom::Point::new(cursor - current, 0.0),
            DistributeAxis::Vertical => dp_geom::Point::new(0.0, cursor - current),
        };
        let inverse = parent_matrix
            .inverse()
            .ok_or_else(|| ModelError::InvalidDocument("singular parent matrix".to_string()))?;
        let delta = inverse.apply_vector(delta_world);
        if let Some(page) = document.page(*page_id)
            && let Some(node) = page.get(*id)
        {
            let before = node.clone();
            let mut after = before.clone();
            after.transform.x += delta.x;
            after.transform.y += delta.y;
            after.rev = after.rev.saturating_add(1);
            transaction.push(Edit::SetNode {
                page: *page_id,
                before,
                after,
            });
        }
        cursor += match axis {
            DistributeAxis::Horizontal => world.w,
            DistributeAxis::Vertical => world.h,
        } + gap;
    }
    Ok(transaction)
}

/// Nodes from `ids` that are not descendants of another selected node.
fn top_level_selection(document: &Document, ids: &[NodeId]) -> Vec<NodeId> {
    let mut roots = Vec::new();
    for id in ids {
        if document.node(*id).is_none() {
            continue;
        }
        let mut cursor = document.node(*id).and_then(|node| node.parent);
        let mut nested = false;
        while let Some(parent) = cursor {
            if ids.contains(&parent) {
                nested = true;
                break;
            }
            cursor = document.node(parent).and_then(|node| node.parent);
        }
        if !nested && !roots.contains(id) {
            roots.push(*id);
        }
    }
    roots
}

/// World matrix of a node's parent (identity for page roots).
fn parent_world_matrix(page: &Page, id: NodeId) -> Option<Mat2D> {
    match page.get(id)?.parent {
        Some(parent) => page.absolute_matrix(parent),
        None => Some(Mat2D::IDENTITY),
    }
}

fn union_rect(left: Rect, right: Rect) -> Rect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = left.right().max(right.right());
    let bottom_edge = left.bottom().max(right.bottom());
    Rect::new(x, y, right_edge - x, bottom_edge - y)
}

fn bounds_in_space(page: &Page, parent: Option<NodeId>, ids: &[NodeId]) -> Option<Rect> {
    let inverse = match parent {
        Some(parent) => page.absolute_matrix(parent)?.inverse()?,
        None => Mat2D::IDENTITY,
    };
    let mut bounds: Option<Rect> = None;
    for id in ids {
        let world = page.world_bounds(*id)?;
        let local = inverse.apply_rect(world);
        bounds = Some(match bounds {
            Some(current) => union_rect(current, local),
            None => local,
        });
    }
    bounds
}

/// Extracts a node transform from a local matrix (rotation/flip aware).
fn transform_from_matrix(matrix: Mat2D, w: f64, h: f64) -> Transform {
    let rotation = matrix.m12.atan2(matrix.m11).to_degrees();
    let determinant = matrix.m11 * matrix.m22 - matrix.m12 * matrix.m21;
    Transform {
        x: matrix.tx,
        y: matrix.ty,
        w,
        h,
        rotation,
        flip_x: determinant < 0.0,
        flip_y: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document_with_shapes() -> (Document, Vec<NodeId>) {
        let mut document = Document::new("Ops");
        let page_id = document.active_page_id();
        let page = document.page_mut(page_id).unwrap();
        let mut ids = Vec::new();
        for index in 0..3 {
            let node = Node::rectangle(
                format!("Rect {index}"),
                index as f64 * 50.0,
                10.0,
                40.0,
                40.0,
            );
            ids.push(node.id);
            page.insert_node(node, None, None).unwrap();
        }
        (document, ids)
    }

    #[test]
    fn delete_subtree_and_undo() {
        let mut document = Document::new("Delete");
        let page_id = document.active_page_id();
        let frame = Node::frame("Frame", 0.0, 0.0, 200.0, 200.0);
        let child = Node::rectangle("Child", 10.0, 10.0, 20.0, 20.0);
        let child_id = child.id;
        let frame_id = frame.id;
        let page = document.page_mut(page_id).unwrap();
        page.insert_node(frame, None, None).unwrap();
        page.insert_node(child, Some(frame_id), None).unwrap();
        let transaction = delete_nodes(&document, &[frame_id], "Delete").unwrap();
        assert_eq!(transaction.len(), 2);
        let mut history = History::new();
        apply_transaction(&mut document, &mut history, transaction).unwrap();
        assert!(document.node(frame_id).is_none());
        assert!(document.node(child_id).is_none());
        history.undo(&mut document).unwrap().unwrap();
        assert!(document.node(frame_id).is_some());
        assert!(document.node(child_id).is_some());
        assert_eq!(document.node(child_id).unwrap().parent, Some(frame_id));
    }

    #[test]
    fn duplicate_offsets_and_keeps_subtree() {
        let mut document = Document::new("Duplicate");
        let page_id = document.active_page_id();
        let frame = Node::frame("Frame", 0.0, 0.0, 200.0, 200.0);
        let child = Node::rectangle("Child", 10.0, 10.0, 20.0, 20.0);
        let child_id = child.id;
        let frame_id = frame.id;
        let page = document.page_mut(page_id).unwrap();
        page.insert_node(frame, None, None).unwrap();
        page.insert_node(child, Some(frame_id), None).unwrap();
        let transaction = duplicate_nodes(&document, &[frame_id], "Duplicate").unwrap();
        let new_root = transaction.selection_after[0];
        let mut history = History::new();
        apply_transaction(&mut document, &mut history, transaction).unwrap();
        assert_eq!(document.active_page().len(), 4);
        let copy = document.node(new_root).unwrap();
        assert_eq!(copy.transform.x, 10.0);
        assert_eq!(copy.children.len(), 1);
        assert_ne!(copy.children[0], child_id);
        assert_eq!(document.node(copy.children[0]).unwrap().name, "Child");
        history.undo(&mut document).unwrap().unwrap();
        assert_eq!(document.active_page().len(), 2);
    }

    #[test]
    fn group_and_ungroup_preserve_world_positions() {
        let (mut document, ids) = document_with_shapes();
        let page_id = document.active_page_id();
        let before: Vec<Rect> = ids
            .iter()
            .map(|id| document.page(page_id).unwrap().world_bounds(*id).unwrap())
            .collect();
        let transaction = group_nodes(&document, &ids, "Group").unwrap();
        let group_id = transaction.selection_after[0];
        let mut history = History::new();
        apply_transaction(&mut document, &mut history, transaction).unwrap();
        let page = document.page(page_id).unwrap();
        assert!(page.get(group_id).unwrap().kind == NodeKind::Group);
        for (index, id) in ids.iter().enumerate() {
            let world = page.world_bounds(*id).unwrap();
            assert!((world.x - before[index].x).abs() < 1e-6, "x {}", index);
            assert!((world.y - before[index].y).abs() < 1e-6, "y {}", index);
            assert_eq!(page.get(*id).unwrap().parent, Some(group_id));
        }
        let transaction = ungroup(&document, group_id, "Ungroup").unwrap();
        apply_transaction(&mut document, &mut history, transaction).unwrap();
        assert!(document.node(group_id).is_none());
        let page = document.page(page_id).unwrap();
        for (index, id) in ids.iter().enumerate() {
            let world = page.world_bounds(*id).unwrap();
            assert!((world.x - before[index].x).abs() < 1e-6);
            assert!((world.y - before[index].y).abs() < 1e-6);
        }
        // Undo ungroup restores the group and its children.
        history.undo(&mut document).unwrap().unwrap();
        assert!(document.node(group_id).is_some());
        let page = document.page(page_id).unwrap();
        assert_eq!(page.get(group_id).unwrap().children.len(), 3);
        // And redo works.
        history.redo(&mut document).unwrap().unwrap();
        assert!(document.node(group_id).is_none());
    }

    #[test]
    fn align_and_distribute_update_positions() {
        let (mut document, ids) = document_with_shapes();
        let transaction = align_nodes(&document, &ids, Align::Left, "Align").unwrap();
        let mut history = History::new();
        apply_transaction(&mut document, &mut history, transaction).unwrap();
        let page = document.page(document.active_page_id()).unwrap();
        let lefts: Vec<f64> = ids
            .iter()
            .map(|id| page.world_bounds(*id).unwrap().x)
            .collect();
        assert!(
            lefts
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 1e-9)
        );

        let transaction = distribute_nodes(
            &document,
            &ids,
            DistributeAxis::Horizontal,
            10.0,
            "Distribute",
        )
        .unwrap();
        apply_transaction(&mut document, &mut history, transaction).unwrap();
        let page = document.page(document.active_page_id()).unwrap();
        let mut rects: Vec<Rect> = ids
            .iter()
            .map(|id| page.world_bounds(*id).unwrap())
            .collect();
        rects.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        for window in rects.windows(2) {
            assert!((window[1].x - window[0].right() - 10.0).abs() < 1e-6);
        }
    }

    #[test]
    fn boolean_creates_editable_operand_node() {
        let (mut document, ids) = document_with_shapes();
        let transaction =
            boolean_nodes(&document, &ids, dp_geom::BooleanOp::Union, "Boolean").unwrap();
        let boolean_id = transaction.selection_after[0];
        let mut history = History::new();
        apply_transaction(&mut document, &mut history, transaction).unwrap();
        let page = document.page(document.active_page_id()).unwrap();
        let boolean = page.get(boolean_id).unwrap();
        assert_eq!(boolean.kind, NodeKind::Boolean);
        assert_eq!(boolean.children.len(), 3);
        assert!(matches!(
            boolean.geometry,
            Some(NodeGeometry::Boolean {
                operation: dp_geom::BooleanOp::Union,
                ..
            })
        ));
    }

    #[test]
    fn failed_transaction_rolls_back() {
        let (mut document, ids) = document_with_shapes();
        let mut history = History::new();
        let missing = NodeId::new_v4();
        let mut transaction = Transaction::new("Bad");
        transaction.push(Edit::Reparent {
            page: document.active_page_id(),
            node_id: ids[0],
            old_parent: None,
            old_index: 0,
            new_parent: Some(missing),
            new_index: 0,
        });
        assert!(apply_transaction(&mut document, &mut history, transaction).is_err());
        assert!(!history.can_undo());
        assert_eq!(document.node(ids[0]).unwrap().parent, None);
    }
}
