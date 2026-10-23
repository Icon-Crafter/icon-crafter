//! Design-system authoring operations.
//!
//! Components, shared styles and variables are document-level registries plus a
//! node role. Every mutation here is expressed as a [`Transaction`] so it is
//! undoable, serializable and observable by collaboration exactly like node
//! edits.
//!
//! Honest limitations of this module:
//! * Component **instances are stored structurally** (role + registry link) but
//!   the renderer does not expand master contents yet, so an instance draws as
//!   its own frame box. Instance expansion is tracked in the feature checklist.
//! * Deleting a style/binding does not walk the whole document to strip broken
//!   references; validation reports them explicitly instead.

use dp_geom::{Mat2D, Point, Rect};
use indexmap::IndexMap;
use uuid::Uuid;

use crate::document::{
    ComponentMeta, ComponentRegistry, Document, DocumentSettings,
};
use crate::geometry::TextStyle;
use crate::ids::{ComponentId, NodeId, PageId, StyleId, VariableId};
use crate::node::{ComponentRole, Node, NodeKind, Transform};
use crate::page::Page;
use crate::paint::{Effect, Fill, Paint, StrokeAlign};
use crate::style::{
    SharedEffectStyle, SharedPaintStyle, SharedTextStyle, StyleRegistry,
};
use crate::transactions::{Edit, Transaction};
use crate::variables::{Variable, VariableCollection, VariableRegistry, VariableType, VariableValue};
use crate::ModelError;

/// Returns selected ids without descendants of other selected ids, in input order.
fn top_level_ids(page: &Page, ids: &[NodeId]) -> Vec<NodeId> {
    ids.iter()
        .copied()
        .filter(|id| page.nodes.contains_key(id))
        .filter(|id| {
            let mut parent = page.nodes.get(id).and_then(|node| node.parent);
            let mut guard = 0;
            while let Some(parent_id) = parent {
                guard += 1;
                if guard > 256 {
                    return true;
                }
                if ids.contains(&parent_id) {
                    return false;
                }
                parent = page.nodes.get(&parent_id).and_then(|node| node.parent);
            }
            true
        })
        .collect()
}

/// World matrix mapping the node's local space to page coordinates.
fn world_matrix(page: &Page, id: NodeId) -> Option<Mat2D> {
    let node = page.nodes.get(&id)?;
    let mut matrix = node.transform.matrix();
    let mut parent = node.parent;
    let mut guard = 0;
    while let Some(parent_id) = parent {
        guard += 1;
        if guard > 256 {
            return None;
        }
        let parent_node = page.nodes.get(&parent_id)?;
        matrix = parent_node.transform.matrix().mul(matrix);
        parent = parent_node.parent;
    }
    Some(matrix)
}

/// World-space axis-aligned bounds of a node.
fn world_bounds(page: &Page, id: NodeId) -> Option<Rect> {
    let node = page.nodes.get(&id)?;
    let matrix = world_matrix(page, id)?;
    let local = Rect::new(0.0, 0.0, node.transform.w, node.transform.h);
    Some(matrix.apply_rect(local))
}

fn set_styles(document: &Document, after: StyleRegistry) -> Edit {
    Edit::SetStyles {
        before: document.styles.clone(),
        after,
    }
}

fn set_variables(document: &Document, after: VariableRegistry) -> Edit {
    Edit::SetVariables {
        before: document.variables.clone(),
        after,
    }
}

/// Turns the current selection into a component master.
///
/// The selected nodes must share one parent. The new component is inserted at
/// the position of the topmost selected node and the nodes are reparented into
/// it with their world positions preserved.
pub fn create_component(
    document: &Document,
    page_id: PageId,
    ids: &[NodeId],
    name: &str,
) -> Result<Transaction, ModelError> {
    let page = document
        .page(page_id)
        .ok_or(ModelError::PageNotFound(page_id))?;
    let roots = top_level_ids(page, ids);
    if roots.is_empty() {
        return Err(ModelError::InvalidDocument(
            "select at least one node to create a component".to_owned(),
        ));
    }
    let mut parents = Vec::with_capacity(roots.len());
    let mut indices = Vec::with_capacity(roots.len());
    for id in &roots {
        let (parent, index) = page
            .index_in_parent(*id)
            .ok_or(ModelError::NodeNotFound(*id))?;
        parents.push(parent);
        indices.push(index);
    }
    let parent = parents[0];
    if parents.iter().any(|candidate| *candidate != parent) {
        return Err(ModelError::InvalidDocument(
            "create a component from nodes that share the same parent".to_owned(),
        ));
    }
    let mut bounds: Option<Rect> = None;
    for id in &roots {
        let node_bounds = world_bounds(page, *id).ok_or(ModelError::NodeNotFound(*id))?;
        bounds = Some(match bounds {
            Some(current) => current.union(node_bounds),
            None => node_bounds,
        });
    }
    let bounds = bounds.expect("selection is not empty");
    let parent_world = match parent {
        Some(parent_id) => world_matrix(page, parent_id).ok_or(ModelError::NodeNotFound(parent_id))?,
        None => Mat2D::IDENTITY,
    };
    let inverse_parent = parent_world
        .inverse()
        .ok_or_else(|| ModelError::InvalidDocument("parent transform is not invertible".to_owned()))?;
    let origin = inverse_parent.apply_point(Point::new(bounds.x, bounds.y));

    let mut component = Node::new(
        NodeKind::Component,
        name.to_owned(),
        Transform::new(
            origin.x,
            origin.y,
            bounds.w.max(0.01),
            bounds.h.max(0.01),
        ),
    );
    // Masters start compact; contents render from their children.
    component.children = Vec::new();
    let component_id = component.id;
    let component_world = parent_world.mul(component.transform.matrix());
    let inverse_component = component_world.inverse().ok_or_else(|| {
        ModelError::InvalidDocument("component transform is not invertible".to_owned())
    })?;
    let insert_index = indices.iter().copied().min().unwrap_or(0);

    let mut transaction = Transaction::new("Create component");
    transaction.push(Edit::InsertNode {
        page: page_id,
        node: component,
        parent,
        index: insert_index,
    });
    for (position, id) in roots.iter().enumerate() {
        let node = page
            .nodes
            .get(id)
            .ok_or(ModelError::NodeNotFound(*id))?;
        let child_world = world_matrix(page, *id).ok_or(ModelError::NodeNotFound(*id))?;
        let local = inverse_component.mul(child_world);
        let mut after = node.clone();
        after.parent = Some(component_id);
        after.transform.x = local.tx;
        after.transform.y = local.ty;
        transaction.push(Edit::Reparent {
            page: page_id,
            node_id: *id,
            old_parent: parent,
            old_index: indices[position],
            new_parent: Some(component_id),
            new_index: position,
        });
        transaction.push(Edit::SetNode {
            page: page_id,
            before: node.clone(),
            after,
        });
    }

    let mut components: ComponentRegistry = document.components.clone();
    components.upsert_component(ComponentMeta {
        id: ComponentId::new_v4(),
        node_id: component_id,
        name: name.to_owned(),
        key: Uuid::new_v4().to_string(),
        description: "Created from selection".to_owned(),
        component_set_id: None,
        variant_properties: IndexMap::new(),
        published: false,
        library_id: None,
    });
    transaction.push(Edit::SetComponents {
        before: document.components.clone(),
        after: components,
    });
    transaction.selection_after = vec![component_id];
    Ok(transaction)
}

/// Converts a component instance back to a plain frame (contents are kept).
pub fn detach_instance(
    document: &Document,
    page_id: PageId,
    node_id: NodeId,
) -> Result<Transaction, ModelError> {
    let page = document
        .page(page_id)
        .ok_or(ModelError::PageNotFound(page_id))?;
    let node = page
        .nodes
        .get(&node_id)
        .ok_or(ModelError::NodeNotFound(node_id))?;
    let is_instance = node.kind == NodeKind::Instance
        || matches!(&node.component, Some(ComponentRole::Instance { .. }));
    if !is_instance {
        return Err(ModelError::InvalidDocument(
            "only component instances can be detached".to_owned(),
        ));
    }
    let mut after = node.clone();
    after.kind = NodeKind::Frame;
    after.component = None;
    let mut transaction = Transaction::new("Detach instance");
    transaction.push(Edit::SetNode {
        page: page_id,
        before: node.clone(),
        after,
    });
    transaction.selection_after = vec![node_id];
    Ok(transaction)
}

/// Places a linked instance of a component on a page.
///
/// The instance records its component id and size; master contents are not
/// expanded by the renderer yet (documented limitation).
pub fn create_instance(
    document: &Document,
    page_id: PageId,
    component_id: ComponentId,
    parent: Option<NodeId>,
    x: f64,
    y: f64,
) -> Result<Transaction, ModelError> {
    let page = document
        .page(page_id)
        .ok_or(ModelError::PageNotFound(page_id))?;
    if let Some(parent_id) = parent {
        let parent_node = page
            .nodes
            .get(&parent_id)
            .ok_or(ModelError::NodeNotFound(parent_id))?;
        if !parent_node.kind.is_container() {
            return Err(ModelError::InvalidDocument(
                "instances can only be placed inside frames, groups or pages".to_owned(),
            ));
        }
    }
    let meta = document.components.component(component_id).ok_or_else(|| {
        ModelError::InvalidDocument(format!("component {component_id} is not registered"))
    })?;
    let (_master_page, master) = document
        .locate_node(meta.node_id)
        .ok_or(ModelError::NodeNotFound(meta.node_id))?;
    let mut node = Node::new(
        NodeKind::Instance,
        meta.name.clone(),
        Transform::new(x, y, master.transform.w, master.transform.h),
    );
    node.component = Some(ComponentRole::Instance {
        component_id,
        overrides: Vec::new(),
        swapped_component_id: None,
    });
    let node_id = node.id;
    let mut transaction = Transaction::new("Place instance");
    transaction.push(Edit::InsertNode {
        page: page_id,
        node,
        parent,
        index: usize::MAX,
    });
    transaction.selection_after = vec![node_id];
    Ok(transaction)
}

/// Creates a shared paint style.
pub fn create_paint_style(
    document: &Document,
    name: &str,
    paint: Paint,
) -> Transaction {
    let mut styles = document.styles.clone();
    styles.upsert_paint(SharedPaintStyle {
        id: StyleId::new_v4(),
        name: name.to_owned(),
        description: String::new(),
        paint,
    });
    let mut transaction = Transaction::new("Create paint style");
    transaction.push(set_styles(document, styles));
    transaction
}

/// Creates a shared text style.
pub fn create_text_style(
    document: &Document,
    name: &str,
    style: TextStyle,
) -> Transaction {
    let mut styles = document.styles.clone();
    styles.upsert_text(SharedTextStyle {
        id: StyleId::new_v4(),
        name: name.to_owned(),
        description: String::new(),
        style,
    });
    let mut transaction = Transaction::new("Create text style");
    transaction.push(set_styles(document, styles));
    transaction
}

/// Creates a shared effect style.
pub fn create_effect_style(
    document: &Document,
    name: &str,
    effects: Vec<Effect>,
) -> Transaction {
    let mut styles = document.styles.clone();
    styles.upsert_effect(SharedEffectStyle {
        id: StyleId::new_v4(),
        name: name.to_owned(),
        description: String::new(),
        effects,
    });
    let mut transaction = Transaction::new("Create effect style");
    transaction.push(set_styles(document, styles));
    transaction
}

/// Creates a style from a node's first relevant visual property.
pub fn create_style_from_node(
    document: &Document,
    node_id: NodeId,
    name: &str,
) -> Result<Transaction, ModelError> {
    let (_page, node) = document
        .locate_node(node_id)
        .ok_or(ModelError::NodeNotFound(node_id))?;
    if let Some(text) = &node.text {
        return Ok(create_text_style(document, name, text.style.clone()));
    }
    if let Some(fill) = node.style.fills.first() {
        return Ok(create_paint_style(document, name, fill.paint.clone()));
    }
    if !node.style.effects.is_empty() {
        return Ok(create_effect_style(
            document,
            name,
            node.style.effects.clone(),
        ));
    }
    if let Some(stroke) = node.style.strokes.first() {
        return Ok(create_paint_style(document, name, stroke.paint.clone()));
    }
    Err(ModelError::InvalidDocument(
        "the node has no fill, text or effect to turn into a style".to_owned(),
    ))
}

/// Binds a shared style to nodes (paint → fills, text → typography, effect → effects).
pub fn apply_style(
    document: &Document,
    ids: &[NodeId],
    style_id: StyleId,
) -> Result<Transaction, ModelError> {
    let mut transaction = Transaction::new("Apply style");
    let mut changed = 0usize;
    for id in ids {
        let (page, node) = document
            .locate_node(*id)
            .ok_or(ModelError::NodeNotFound(*id))?;
        let page_id = page.id;
        let mut after = node.clone();
        if let Some(shared) = document.styles.paint(style_id) {
            let mut fill = Fill::solid(crate::Color::TRANSPARENT);
            fill.paint = shared.paint.clone();
            fill.style_id = Some(style_id);
            fill.variable_id = None;
            after.style.fills = vec![fill];
        } else if let Some(shared) = document.styles.text(style_id) {
            let Some(text) = after.text.as_mut() else {
                continue;
            };
            text.style = shared.style.clone();
        } else if let Some(shared) = document.styles.effect(style_id) {
            after.style.effects = shared.effects.clone();
        } else {
            return Err(ModelError::InvalidDocument(format!(
                "style {style_id} does not exist"
            )));
        }
        transaction.push(Edit::SetNode {
            page: page_id,
            before: node.clone(),
            after,
        });
        changed += 1;
    }
    if changed == 0 {
        return Err(ModelError::InvalidDocument(
            "the selected nodes cannot take this style".to_owned(),
        ));
    }
    transaction.selection_after = ids.to_vec();
    Ok(transaction)
}

/// Renames any shared style kind.
pub fn rename_style(
    document: &Document,
    style_id: StyleId,
    name: &str,
) -> Result<Transaction, ModelError> {
    let mut styles = document.styles.clone();
    if let Some(style) = styles.paints.get_mut(&style_id) {
        style.name = name.to_owned();
    } else if let Some(style) = styles.texts.get_mut(&style_id) {
        style.name = name.to_owned();
    } else if let Some(style) = styles.effects.get_mut(&style_id) {
        style.name = name.to_owned();
    } else {
        return Err(ModelError::InvalidDocument(format!(
            "style {style_id} does not exist"
        )));
    }
    let mut transaction = Transaction::new("Rename style");
    transaction.push(set_styles(document, styles));
    Ok(transaction)
}

/// Removes a shared style (references stay in the document and validate as broken).
pub fn delete_style(
    document: &Document,
    style_id: StyleId,
) -> Result<Transaction, ModelError> {
    let mut styles = document.styles.clone();
    let removed = styles.paints.shift_remove(&style_id).is_some()
        || styles.texts.shift_remove(&style_id).is_some()
        || styles.effects.shift_remove(&style_id).is_some();
    if !removed {
        return Err(ModelError::InvalidDocument(format!(
            "style {style_id} does not exist"
        )));
    }
    let mut transaction = Transaction::new("Delete style");
    transaction.push(set_styles(document, styles));
    Ok(transaction)
}

fn ensure_collection(
    document: &Document,
    collection_name: &str,
) -> Result<(VariableCollection, VariableRegistry), ModelError> {
    let mut registry = document.variables.clone();
    if let Some(found) = registry
        .collections
        .iter()
        .find(|collection| collection.name == collection_name)
    {
        return Ok((found.clone(), registry));
    }
    let collection = VariableCollection::new(collection_name);
    registry.collections.push(collection.clone());
    Ok((collection, registry))
}

fn ensure_variable_name_free(registry: &VariableRegistry, name: &str) -> Result<(), ModelError> {
    if registry.variables.values().any(|variable| variable.name == name) {
        return Err(ModelError::InvalidDocument(format!(
            "a variable named {name} already exists"
        )));
    }
    Ok(())
}

/// Creates a variable collection (fails when the name is already used).
pub fn create_variable_collection(
    document: &Document,
    name: &str,
) -> Result<Transaction, ModelError> {
    if document
        .variables
        .collections
        .iter()
        .any(|collection| collection.name == name)
    {
        return Err(ModelError::InvalidDocument(format!(
            "a variable collection named {name} already exists"
        )));
    }
    let mut registry = document.variables.clone();
    registry.collections.push(VariableCollection::new(name));
    let mut transaction = Transaction::new("Create variable collection");
    transaction.push(set_variables(document, registry));
    Ok(transaction)
}

/// Creates a colour variable in a named collection (creating the collection when absent).
pub fn create_color_variable(
    document: &Document,
    collection_name: &str,
    variable_name: &str,
    color: crate::Color,
) -> Result<Transaction, ModelError> {
    let (collection, mut registry) = ensure_collection(document, collection_name)?;
    ensure_variable_name_free(&registry, variable_name)?;
    let variable = Variable::color(
        collection.id,
        variable_name,
        collection.default_mode,
        color,
    );
    let variable_id = variable.id;
    registry.variables.insert(variable_id, variable);
    let mut transaction = Transaction::new("Create colour variable");
    transaction.push(set_variables(document, registry));
    Ok(transaction)
}

/// Creates a numeric variable in a named collection.
pub fn create_float_variable(
    document: &Document,
    collection_name: &str,
    variable_name: &str,
    value: f64,
) -> Result<Transaction, ModelError> {
    let (collection, mut registry) = ensure_collection(document, collection_name)?;
    ensure_variable_name_free(&registry, variable_name)?;
    let mut values = IndexMap::new();
    values.insert(collection.default_mode, VariableValue::Float { value });
    let variable = Variable {
        id: VariableId::new_v4(),
        collection_id: collection.id,
        name: variable_name.to_owned(),
        description: String::new(),
        resolved_type: VariableType::Float,
        values,
    };
    let variable_id = variable.id;
    registry.variables.insert(variable_id, variable);
    let mut transaction = Transaction::new("Create numeric variable");
    transaction.push(set_variables(document, registry));
    Ok(transaction)
}

/// Sets one mode value of an existing variable.
pub fn set_variable_value(
    document: &Document,
    variable_id: VariableId,
    mode_id: crate::ids::ModeId,
    value: VariableValue,
) -> Result<Transaction, ModelError> {
    let mut registry = document.variables.clone();
    let variable = registry
        .variables
        .get(&variable_id)
        .ok_or_else(|| ModelError::InvalidDocument(format!("variable {variable_id} does not exist")))?;
    let collection = registry
        .collections
        .iter()
        .find(|collection| collection.id == variable.collection_id)
        .ok_or_else(|| {
            ModelError::InvalidDocument(format!(
                "variable {variable_id} belongs to a missing collection"
            ))
        })?;
    if !collection.has_mode(mode_id) {
        return Err(ModelError::InvalidDocument(format!(
            "mode {mode_id} does not exist in collection {}",
            collection.name
        )));
    }
    if let Some(variable) = registry.variables.get_mut(&variable_id) {
        variable.values.insert(mode_id, value);
    }
    let mut transaction = Transaction::new("Set variable value");
    transaction.push(set_variables(document, registry));
    Ok(transaction)
}

/// Selects the active mode for a collection (persisted theme selection).
pub fn set_active_mode(
    document: &Document,
    collection_id: crate::ids::CollectionId,
    mode_id: crate::ids::ModeId,
) -> Result<Transaction, ModelError> {
    let collection = document
        .variables
        .collections
        .iter()
        .find(|collection| collection.id == collection_id)
        .ok_or_else(|| {
            ModelError::InvalidDocument(format!("variable collection {collection_id} does not exist"))
        })?;
    if !collection.has_mode(mode_id) {
        return Err(ModelError::InvalidDocument(format!(
            "mode {mode_id} does not exist in collection {}",
            collection.name
        )));
    }
    let mut settings: DocumentSettings = document.settings.clone();
    settings.active_modes.insert(collection_id, mode_id);
    let mut transaction = Transaction::new("Switch variable mode");
    transaction.push(Edit::SetSettings {
        before: document.settings.clone(),
        after: settings,
    });
    Ok(transaction)
}

/// Convenience: total number of registered design-system entries.
pub fn design_system_counts(document: &Document) -> (usize, usize, usize) {
    (
        document.styles.len(),
        document.components.len(),
        document.variables.variables.len(),
    )
}

/// Stroke alignment used when converting a paint style to a stroke (helper).
pub fn stroke_from_paint(paint: Paint, weight: f64) -> crate::paint::Stroke {
    let mut stroke = crate::paint::Stroke::solid(crate::Color::BLACK);
    stroke.paint = paint;
    stroke.weight = weight.max(0.01);
    stroke.align = StrokeAlign::Center;
    stroke
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::apply_transaction;
    use crate::transactions::History;
    use crate::{Color, Document};

    fn document_with_rects() -> (Document, PageId, Vec<NodeId>) {
        let mut document = Document::new("Design system");
        let page_id = document.active_page_id();
        let mut ids = Vec::new();
        {
            let page = document.page_mut(page_id).expect("page");
            for (index, x) in [0.0_f64, 100.0].into_iter().enumerate() {
                let mut rect = Node::rectangle("Rect", x, 0.0, 80.0, 40.0);
                rect.style.fills = vec![Fill::solid(Color::from_hex("#ff6b4a").unwrap())];
                ids.push(rect.id);
                let _ = index;
                page.insert_node(rect, None, None).unwrap();
            }
        }
        (document, page_id, ids)
    }

    fn apply(document: &mut Document, history: &mut History, transaction: Transaction) {
        apply_transaction(document, history, transaction).expect("transaction applies");
    }

    #[test]
    fn create_component_reparents_and_registers() {
        let (mut document, page_id, ids) = document_with_rects();
        let mut history = History::new();
        let transaction = create_component(&document, page_id, &ids, "Button").unwrap();
        let component_id = transaction.selection_after[0];
        apply(&mut document, &mut history, transaction);
        let page = document.page(page_id).expect("page");
        let component = page.nodes.get(&component_id).expect("component node");
        assert_eq!(component.kind, NodeKind::Component);
        assert_eq!(component.children.len(), 2);
        assert_eq!(component.transform.w, 180.0);
        assert_eq!(document.components.len(), 1);
        assert_eq!(document.components.components.values().next().unwrap().node_id, component_id);
        history.undo(&mut document).expect("undo works");
        let page = document.page(page_id).expect("page");
        assert_eq!(page.nodes.get(&component_id), None);
        assert_eq!(page.roots.len(), 2);
        assert!(document.components.is_empty());
    }

    #[test]
    fn instances_are_linked_and_can_be_detached() {
        let (mut document, page_id, ids) = document_with_rects();
        let mut history = History::new();
        let create = create_component(&document, page_id, &ids, "Card").unwrap();
        let master_id = create.selection_after[0];
        apply(&mut document, &mut history, create);
        let component_id = document.components.components.keys().copied().next().unwrap();
        let place = create_instance(&document, page_id, component_id, None, 300.0, 40.0).unwrap();
        let instance_id = place.selection_after[0];
        apply(&mut document, &mut history, place);
        let instance = document.node(instance_id).expect("instance");
        assert_eq!(instance.kind, NodeKind::Instance);
        assert!(matches!(
            instance.component,
            Some(ComponentRole::Instance { component_id: linked, .. }) if linked == component_id
        ));
        assert_eq!(instance.transform.w, document.node(master_id).unwrap().transform.w);
        let detach = detach_instance(&document, page_id, instance_id).unwrap();
        apply(&mut document, &mut history, detach);
        let detached = document.node(instance_id).expect("detached");
        assert_eq!(detached.kind, NodeKind::Frame);
        assert!(detached.component.is_none());
    }

    #[test]
    fn styles_are_created_from_nodes_and_applied() {
        let (mut document, page_id, ids) = document_with_rects();
        let mut history = History::new();
        let transaction = create_style_from_node(&document, ids[0], "Brand/Coral").unwrap();
        apply(&mut document, &mut history, transaction);
        let style_id = *document.styles.paints.keys().next().unwrap();
        assert_eq!(document.styles.paints[&style_id].name, "Brand/Coral");
        // Apply to the second rectangle.
        let apply_tx = apply_style(&document, &[ids[1]], style_id).unwrap();
        apply(&mut document, &mut history, apply_tx);
        let page = document.page(page_id).expect("page");
        let fill = &page.nodes[&ids[1]].style.fills[0];
        assert_eq!(fill.style_id, Some(style_id));
        history.undo(&mut document).expect("undo");
        let page = document.page(page_id).expect("page");
        assert_eq!(page.nodes[&ids[1]].style.fills[0].style_id, None);
    }

    #[test]
    fn text_styles_apply_to_text_nodes() {
        let mut document = Document::new("Text styles");
        let page_id = document.active_page_id();
        let text_id;
        {
            let page = document.page_mut(page_id).expect("page");
            let text = Node::text("Label", "Hello", 0.0, 0.0);
            text_id = text.id;
            page.insert_node(text, None, None).unwrap();
        }
        let style_tx = create_style_from_node(&document, text_id, "Typography/Body").unwrap();
        let mut history = History::new();
        apply(&mut document, &mut history, style_tx);
        let style_id = *document.styles.texts.keys().next().unwrap();
        let mut other = Node::text("Other", "World", 0.0, 100.0);
        let other_id = other.id;
        other.text.as_mut().unwrap().style.font_size = 40.0;
        {
            let page = document.page_mut(page_id).expect("page");
            page.insert_node(other, None, None).unwrap();
        }
        let apply_tx = apply_style(&document, &[other_id], style_id).unwrap();
        apply(&mut document, &mut history, apply_tx);
        let node = document.node(other_id).unwrap();
        assert_eq!(node.text.as_ref().unwrap().style.font_size, 14.0);
        let rename = rename_style(&document, style_id, "Typography/Body Large").unwrap();
        apply(&mut document, &mut history, rename);
        assert_eq!(document.styles.texts[&style_id].name, "Typography/Body Large");
        let delete = delete_style(&document, style_id).unwrap();
        apply(&mut document, &mut history, delete);
        assert!(document.styles.texts.is_empty());
    }

    #[test]
    fn variables_and_modes_are_undoable() {
        let (mut document, _, _) = document_with_rects();
        let mut history = History::new();
        let create = create_color_variable(
            &document,
            "Theme",
            "color/background",
            Color::from_hex("#0b0d11").unwrap(),
        )
        .unwrap();
        apply(&mut document, &mut history, create);
        let collection = document.variables.collections[0].clone();
        let variable_id = *document.variables.variables.keys().next().unwrap();
        let light = collection.default_mode;
        assert_eq!(
            document.variables.resolve_color(variable_id, light).unwrap().to_hex(),
            "#0b0d11"
        );
        let set_value = set_variable_value(
            &document,
            variable_id,
            light,
            VariableValue::Color {
                color: Color::from_hex("#ffffff").unwrap(),
            },
        )
        .unwrap();
        apply(&mut document, &mut history, set_value);
        assert_eq!(
            document.variables.resolve_color(variable_id, light).unwrap().to_hex(),
            "#ffffff"
        );
        let mode = set_active_mode(&document, collection.id, light).unwrap();
        apply(&mut document, &mut history, mode);
        assert_eq!(document.settings.active_modes.get(&collection.id), Some(&light));
        history.undo(&mut document).expect("undo mode");
        assert!(document.settings.active_modes.is_empty());
        let nested = create_variable_collection(&document, "Theme").unwrap_err();
        assert!(matches!(nested, ModelError::InvalidDocument(_)));
    }

    #[test]
    fn float_variables_and_collection_creation() {
        let (mut document, _, _) = document_with_rects();
        let mut history = History::new();
        let create = create_float_variable(&document, "Spacing", "space/2", 8.0).unwrap();
        apply(&mut document, &mut history, create);
        let collection = document.variables.collections[0].clone();
        let variable_id = *document.variables.variables.keys().next().unwrap();
        assert_eq!(
            document
                .variables
                .resolve_float(variable_id, collection.default_mode)
                .unwrap(),
            8.0
        );
        assert_eq!(
            design_system_counts(&document),
            (0, 0, 1)
        );
    }

    #[test]
    fn apply_style_with_unknown_style_is_an_error() {
        let (document, _, ids) = document_with_rects();
        let result = apply_style(&document, &ids, StyleId::new_v4());
        assert!(matches!(result, Err(ModelError::InvalidDocument(_))));
    }
}
