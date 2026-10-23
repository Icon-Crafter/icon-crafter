//! Developer handoff workbench: implementation annotations, readiness tracking
//! and revision comparison.

use std::collections::HashMap;

use dp_model::{Document, NodeId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Kind of a developer annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    /// A free-form comment for engineering.
    Note,
    /// "Implement this with ..." guidance.
    Implementation,
    /// A measurement that must be honoured.
    Measurement,
}

/// A developer annotation attached to a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// Stable id.
    pub id: Uuid,
    /// Inspected node.
    pub node_id: NodeId,
    /// Node name at annotation time (kept for readable exports).
    pub node_name: String,
    /// Annotation text.
    pub body: String,
    /// Author display name.
    pub author: String,
    /// ISO-8601 timestamp.
    pub created_at: String,
    /// Annotation kind.
    pub kind: AnnotationKind,
    /// Optional link to code or documentation.
    pub link: Option<String>,
    /// Whether the annotation is resolved.
    pub resolved: bool,
}

/// Readiness of a selection/frame for development.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    /// Still changing.
    Draft,
    /// Awaiting design review.
    ReadyForReview,
    /// Contract for engineering.
    ReadyForDevelopment,
    /// Engineering has implemented it.
    Implemented,
}

impl HandoffStatus {
    /// UI label.
    pub fn label(self) -> &'static str {
        match self {
            HandoffStatus::Draft => "Draft",
            HandoffStatus::ReadyForReview => "Ready for review",
            HandoffStatus::ReadyForDevelopment => "Ready for development",
            HandoffStatus::Implemented => "Implemented",
        }
    }
}

/// Handoff state for one document revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Handoff {
    /// Revision the annotations were made against.
    pub revision: u64,
    /// Readiness.
    pub status: HandoffStatus,
    /// Annotations in creation order.
    pub annotations: Vec<Annotation>,
    /// Who marked it ready.
    pub marked_ready_by: Option<String>,
    /// When it was marked ready.
    pub marked_ready_at: Option<String>,
}

impl Handoff {
    /// Empty handoff for `revision`.
    pub fn new(revision: u64) -> Self {
        Self {
            revision,
            status: HandoffStatus::Draft,
            annotations: Vec::new(),
            marked_ready_by: None,
            marked_ready_at: None,
        }
    }

    /// Adds an annotation and returns its id.
    pub fn add_annotation(
        &mut self,
        node: NodeId,
        node_name: impl Into<String>,
        body: impl Into<String>,
        author: impl Into<String>,
        created_at: impl Into<String>,
        kind: AnnotationKind,
    ) -> Uuid {
        let annotation = Annotation {
            id: Uuid::new_v4(),
            node_id: node,
            node_name: node_name.into(),
            body: body.into(),
            author: author.into(),
            created_at: created_at.into(),
            kind,
            link: None,
            resolved: false,
        };
        let id = annotation.id;
        self.annotations.push(annotation);
        id
    }

    /// Attaches a link to an annotation; returns false when missing.
    pub fn link_annotation(&mut self, id: Uuid, link: impl Into<String>) -> bool {
        let link = link.into();
        if let Some(annotation) = self.annotations.iter_mut().find(|item| item.id == id) {
            annotation.link = Some(link);
            true
        } else {
            false
        }
    }

    /// Resolves an annotation; returns false when missing.
    pub fn resolve_annotation(&mut self, id: Uuid) -> bool {
        if let Some(annotation) = self.annotations.iter_mut().find(|item| item.id == id) {
            annotation.resolved = true;
            true
        } else {
            false
        }
    }

    /// Annotations attached to one node.
    pub fn annotations_for(&self, node: NodeId) -> Vec<&Annotation> {
        self.annotations
            .iter()
            .filter(|annotation| annotation.node_id == node)
            .collect()
    }

    /// Annotations still open.
    pub fn open_annotations(&self) -> Vec<&Annotation> {
        self.annotations
            .iter()
            .filter(|annotation| !annotation.resolved)
            .collect()
    }

    /// Count of annotations by kind.
    pub fn count(&self, kind: AnnotationKind) -> usize {
        self.annotations
            .iter()
            .filter(|annotation| annotation.kind == kind)
            .count()
    }

    /// Marks the handoff ready (or implemented). Refuses to move backwards
    /// silently by overwriting the latest state.
    pub fn mark_ready(
        &mut self,
        status: HandoffStatus,
        by: impl Into<String>,
        at: impl Into<String>,
    ) {
        self.status = status;
        self.marked_ready_by = Some(by.into());
        self.marked_ready_at = Some(at.into());
    }

    /// Whether the handoff still has open annotations.
    pub fn is_blocked(&self) -> bool {
        !self.open_annotations().is_empty()
    }
}

/// One node reported by a diff.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeDiff {
    /// Node id.
    pub id: String,
    /// Node name.
    pub name: String,
    /// Node kind (serde tag).
    pub kind: String,
}

/// One changed node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeChange {
    /// Node id.
    pub id: String,
    /// Current name.
    pub name: String,
    /// Changed field names.
    pub fields: Vec<String>,
}

/// Counts for a revision diff.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffCounts {
    /// Nodes added in the newer revision.
    pub added: usize,
    /// Nodes removed.
    pub removed: usize,
    /// Nodes modified.
    pub changed: usize,
}

/// Structural diff between two document revisions.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionDiff {
    /// Nodes only in the newer revision.
    pub added: Vec<NodeDiff>,
    /// Nodes only in the older revision.
    pub removed: Vec<NodeDiff>,
    /// Nodes present in both but modified.
    pub changed: Vec<NodeChange>,
    /// Totals.
    pub counts: DiffCounts,
}

impl RevisionDiff {
    /// Whether anything changed.
    pub fn is_empty(&self) -> bool {
        self.counts.added == 0 && self.counts.removed == 0 && self.counts.changed == 0
    }
}

/// Fields compared per node.
const COMPARED_FIELDS: [&str; 13] = [
    "name",
    "kind",
    "visible",
    "locked",
    "opacity",
    "blend",
    "transform",
    "style",
    "geometry",
    "text",
    "layout",
    "constraints",
    "component",
];

fn node_field_value(node: &dp_model::Node, field: &str) -> serde_json::Value {
    match field {
        "name" => serde_json::json!(node.name),
        "kind" => serde_json::to_value(node.kind).unwrap_or(serde_json::Value::Null),
        "visible" => serde_json::json!(node.visible),
        "locked" => serde_json::json!(node.locked),
        "opacity" => serde_json::json!(node.opacity),
        "blend" => serde_json::to_value(node.blend).unwrap_or(serde_json::Value::Null),
        "transform" => serde_json::to_value(node.transform).unwrap_or(serde_json::Value::Null),
        "style" => serde_json::to_value(&node.style).unwrap_or(serde_json::Value::Null),
        "geometry" => serde_json::to_value(&node.geometry).unwrap_or(serde_json::Value::Null),
        "text" => serde_json::to_value(&node.text).unwrap_or(serde_json::Value::Null),
        "layout" => serde_json::to_value(&node.layout).unwrap_or(serde_json::Value::Null),
        "constraints" => serde_json::to_value(node.constraints).unwrap_or(serde_json::Value::Null),
        "component" => serde_json::to_value(&node.component).unwrap_or(serde_json::Value::Null),
        _ => serde_json::Value::Null,
    }
}

fn collect_nodes(document: &Document) -> HashMap<NodeId, (String, &dp_model::Node)> {
    let mut nodes = HashMap::new();
    for page in &document.pages {
        for (id, node) in &page.nodes {
            nodes.insert(*id, (kind_key(node), node));
        }
    }
    nodes
}

fn kind_key(node: &dp_model::Node) -> String {
    serde_json::to_value(node.kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Compares two document revisions structurally.
pub fn compare_documents(before: &Document, after: &Document) -> RevisionDiff {
    let old = collect_nodes(before);
    let new = collect_nodes(after);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut all_ids: Vec<NodeId> = old.keys().chain(new.keys()).copied().collect();
    all_ids.sort_by_key(|id| id.to_string());
    all_ids.dedup();
    for id in all_ids {
        match (old.get(&id), new.get(&id)) {
            (None, Some((kind, node))) => added.push(NodeDiff {
                id: id.to_string(),
                name: node.name.clone(),
                kind: kind.clone(),
            }),
            (Some((kind, node)), None) => removed.push(NodeDiff {
                id: id.to_string(),
                name: node.name.clone(),
                kind: kind.clone(),
            }),
            (Some((_, old_node)), Some((_kind, new_node))) => {
                let mut fields = Vec::new();
                for field in COMPARED_FIELDS {
                    if node_field_value(old_node, field) != node_field_value(new_node, field) {
                        fields.push(field.to_owned());
                    }
                }
                if !fields.is_empty() {
                    changed.push(NodeChange {
                        id: id.to_string(),
                        name: new_node.name.clone(),
                        fields,
                    });
                }
            }
            (None, None) => {}
        }
    }
    let counts = DiffCounts {
        added: added.len(),
        removed: removed.len(),
        changed: changed.len(),
    };
    RevisionDiff {
        added,
        removed,
        changed,
        counts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_model::{Color, Node};

    fn document() -> Document {
        let mut document = Document::new("Handoff");
        let page_id = document.active_page_id();
        let page = document.page_mut(page_id).expect("page");
        let node = Node::rectangle("Button", 0.0, 0.0, 120.0, 40.0);
        page.insert_node(node, None, None).unwrap();
        document
    }

    #[test]
    fn annotations_attach_resolve_and_link() {
        let mut handoff = Handoff::new(1);
        let document = document();
        let node_id = document.active_page().roots[0];
        let id = handoff.add_annotation(
            node_id,
            "Button",
            "Use a real <button>",
            "designer",
            "2026-01-01T00:00:00Z",
            AnnotationKind::Implementation,
        );
        assert_eq!(handoff.annotations_for(node_id).len(), 1);
        assert_eq!(handoff.count(AnnotationKind::Implementation), 1);
        assert!(handoff.is_blocked());
        assert!(handoff.link_annotation(id, "src/Button.tsx"));
        assert!(handoff.resolve_annotation(id));
        assert!(!handoff.is_blocked());
        assert!(!handoff.resolve_annotation(Uuid::new_v4()));
    }

    #[test]
    fn marking_ready_tracks_actor_and_time() {
        let mut handoff = Handoff::new(3);
        handoff.mark_ready(
            HandoffStatus::ReadyForDevelopment,
            "designer",
            "2026-01-02T00:00:00Z",
        );
        assert_eq!(handoff.status, HandoffStatus::ReadyForDevelopment);
        assert_eq!(handoff.marked_ready_by.as_deref(), Some("designer"));
        assert_eq!(
            handoff.marked_ready_at.as_deref(),
            Some("2026-01-02T00:00:00Z")
        );
        assert_eq!(HandoffStatus::Implemented.label(), "Implemented");
    }

    #[test]
    fn revision_diff_reports_added_changed_removed() {
        let before = document();
        let mut after = before.clone();
        // change name and opacity of the existing node
        let page_id = after.active_page_id();
        {
            let page = after.page_mut(page_id).expect("page");
            let id = page.roots[0];
            let node = page.nodes.get_mut(&id).unwrap();
            node.name = "Primary Button".to_owned();
            node.opacity = 0.5;
            node.style.fills = vec![dp_model::Fill::solid(Color::WHITE)];
        }
        // add a second node
        {
            let page = after.page_mut(page_id).expect("page");
            page.insert_node(
                Node::rectangle("Spacer", 200.0, 0.0, 10.0, 40.0),
                None,
                None,
            )
            .unwrap();
        }
        let diff = compare_documents(&before, &after);
        assert_eq!(diff.counts.added, 1);
        assert_eq!(diff.counts.removed, 0);
        assert_eq!(diff.counts.changed, 1);
        assert!(!diff.is_empty());
        assert_eq!(diff.added[0].name, "Spacer");
        let change = &diff.changed[0];
        assert!(change.fields.contains(&"name".to_owned()));
        assert!(change.fields.contains(&"opacity".to_owned()));
        assert!(change.fields.contains(&"style".to_owned()));

        // reversing swaps added/removed
        let reverse = compare_documents(&after, &before);
        assert_eq!(reverse.counts.added, 0);
        assert_eq!(reverse.counts.removed, 1);
        assert_eq!(reverse.counts.changed, 1);
    }

    #[test]
    fn identical_documents_have_empty_diff() {
        let document = document();
        let clone = document.clone();
        let diff = compare_documents(&document, &clone);
        assert!(diff.is_empty());
        assert_eq!(
            diff.counts.added + diff.counts.removed + diff.counts.changed,
            0
        );
    }
}
