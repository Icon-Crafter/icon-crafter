//! Structural document diff between two versions.
//!
//! The diff is computed from canonical dp-model document JSON: pages and nodes
//! are keyed by their document ids, and a changed node reports only the
//! top-level field names that differ. Full blobs are never returned.

use std::collections::BTreeSet;

use dp_model::Document;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{ApiError, AppResult};

/// One node whose top-level fields changed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NodeChange {
    /// Node id (UUID string).
    pub node_id: String,
    /// Changed top-level node field names, sorted.
    pub changed_fields: Vec<String>,
}

/// Diff for a page that exists in both versions.
#[derive(Debug, Clone, Serialize)]
pub struct PageDiff {
    /// Page id (UUID string).
    pub page_id: String,
    /// Page name in the newer version (or older version when removed).
    pub name: String,
    /// `added`, `removed` or `changed`.
    pub status: String,
    /// Node ids present only in the newer version.
    pub added_nodes: Vec<String>,
    /// Node ids present only in the older version.
    pub removed_nodes: Vec<String>,
    /// Nodes present in both but changed.
    pub changed_nodes: Vec<NodeChange>,
}

/// Aggregate counters for a document diff.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct DiffSummary {
    /// Pages only in the newer version.
    pub pages_added: usize,
    /// Pages only in the older version.
    pub pages_removed: usize,
    /// Pages present in both with at least one change.
    pub pages_changed: usize,
    /// Nodes only in the newer version.
    pub nodes_added: usize,
    /// Nodes only in the older version.
    pub nodes_removed: usize,
    /// Nodes present in both but changed.
    pub nodes_changed: usize,
}

/// Full diff result.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentDiff {
    /// Aggregate counters.
    pub summary: DiffSummary,
    /// Per-page entries, sorted by page id.
    pub pages: Vec<PageDiff>,
}

impl DocumentDiff {
    /// Whether the two documents are identical in the compared structure.
    pub fn is_empty(&self) -> bool {
        self.summary.pages_added == 0
            && self.summary.pages_removed == 0
            && self.summary.pages_changed == 0
            && self.summary.nodes_added == 0
            && self.summary.nodes_removed == 0
            && self.summary.nodes_changed == 0
    }
}

struct PageSnapshot {
    name: String,
    nodes: Map<String, Value>,
    meta: Value,
}

/// Diff two documents.
pub fn diff_documents(old: &Document, new: &Document) -> AppResult<DocumentDiff> {
    let old_value = old
        .to_json_value()
        .map_err(|err| ApiError::internal_context("serializing stored version", err))?;
    let new_value = new
        .to_json_value()
        .map_err(|err| ApiError::internal_context("serializing stored version", err))?;
    Ok(diff_document_json(&old_value, &new_value))
}

/// Diff two canonical document JSON values.
pub fn diff_document_json(old: &Value, new: &Value) -> DocumentDiff {
    let old_pages = page_snapshots(old);
    let new_pages = page_snapshots(new);
    let mut summary = DiffSummary::default();
    let mut pages = Vec::new();

    let all_ids: BTreeSet<&String> = old_pages.keys().chain(new_pages.keys()).collect();
    for page_id in all_ids {
        match (old_pages.get(page_id), new_pages.get(page_id)) {
            (Some(old_page), Some(new_page)) => {
                let mut entry = PageDiff {
                    page_id: page_id.clone(),
                    name: new_page.name.clone(),
                    status: "changed".to_owned(),
                    added_nodes: Vec::new(),
                    removed_nodes: Vec::new(),
                    changed_nodes: Vec::new(),
                };
                let node_ids: BTreeSet<&String> =
                    old_page.nodes.keys().chain(new_page.nodes.keys()).collect();
                for node_id in node_ids {
                    match (old_page.nodes.get(node_id), new_page.nodes.get(node_id)) {
                        (None, Some(_)) => entry.added_nodes.push(node_id.clone()),
                        (Some(_), None) => entry.removed_nodes.push(node_id.clone()),
                        (Some(old_node), Some(new_node)) if old_node != new_node => {
                            entry.changed_nodes.push(NodeChange {
                                node_id: node_id.clone(),
                                changed_fields: changed_top_level_fields(old_node, new_node),
                            });
                        }
                        _ => {}
                    }
                }
                let page_meta_changed = old_page.meta != new_page.meta;
                let changed = !entry.added_nodes.is_empty()
                    || !entry.removed_nodes.is_empty()
                    || !entry.changed_nodes.is_empty()
                    || page_meta_changed;
                if changed {
                    summary.pages_changed += 1;
                    summary.nodes_added += entry.added_nodes.len();
                    summary.nodes_removed += entry.removed_nodes.len();
                    summary.nodes_changed += entry.changed_nodes.len();
                    pages.push(entry);
                }
            }
            (None, Some(page)) => {
                summary.pages_added += 1;
                summary.nodes_added += page.nodes.len();
                pages.push(PageDiff {
                    page_id: page_id.clone(),
                    name: page.name.clone(),
                    status: "added".to_owned(),
                    added_nodes: page.nodes.keys().cloned().collect(),
                    removed_nodes: Vec::new(),
                    changed_nodes: Vec::new(),
                });
            }
            (Some(page), None) => {
                summary.pages_removed += 1;
                summary.nodes_removed += page.nodes.len();
                pages.push(PageDiff {
                    page_id: page_id.clone(),
                    name: page.name.clone(),
                    status: "removed".to_owned(),
                    added_nodes: Vec::new(),
                    removed_nodes: page.nodes.keys().cloned().collect(),
                    changed_nodes: Vec::new(),
                });
            }
            (None, None) => {}
        }
    }

    for page in &mut pages {
        page.added_nodes.sort();
        page.removed_nodes.sort();
        page.changed_nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    }

    DocumentDiff { summary, pages }
}

fn page_snapshots(document: &Value) -> std::collections::BTreeMap<String, PageSnapshot> {
    let mut pages = std::collections::BTreeMap::new();
    let Some(entries) = document.get("pages").and_then(Value::as_array) else {
        return pages;
    };
    for page in entries {
        let Some(id) = page.get("id").and_then(Value::as_str) else {
            continue;
        };
        let name = page
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let nodes = page
            .get("nodes")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let mut meta = page.clone();
        if let Some(object) = meta.as_object_mut() {
            object.remove("nodes");
            object.remove("roots");
            object.remove("id");
            object.remove("name");
        }
        pages.insert(id.to_owned(), PageSnapshot { name, nodes, meta });
    }
    pages
}

fn changed_top_level_fields(old_node: &Value, new_node: &Value) -> Vec<String> {
    let mut fields = BTreeSet::new();
    let empty = Map::new();
    let old_fields = old_node.as_object().unwrap_or(&empty);
    let new_fields = new_node.as_object().unwrap_or(&empty);
    for (key, value) in old_fields {
        match new_fields.get(key) {
            Some(new_value) if new_value == value => {}
            _ => {
                fields.insert(key.clone());
            }
        }
    }
    for key in new_fields.keys() {
        if !old_fields.contains_key(key) {
            fields.insert(key.clone());
        }
    }
    fields.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_model::{Node, NodeId, NodeKind, Page, Transform};

    fn document_with_nodes() -> (Document, PageIdAlias, NodeId) {
        let mut document = Document::new("Diff fixture");
        let page_id = document.pages[0].id;
        let node_id = NodeId::new_v4();
        let node = Node::with_id(
            node_id,
            NodeKind::Rectangle,
            "Rect",
            Transform::new(0.0, 0.0, 40.0, 20.0),
        );
        insert(&mut document.pages[0], node);
        (document, page_id, node_id)
    }

    type PageIdAlias = dp_model::PageId;

    fn insert(page: &mut Page, node: Node) {
        page.insert_node(node, None, None).expect("insert node");
    }

    #[test]
    fn identical_documents_have_empty_diff() {
        let (document, _, _) = document_with_nodes();
        let diff = diff_documents(&document, &document).expect("diff");
        assert!(diff.is_empty());
        assert!(diff.pages.is_empty());
    }

    #[test]
    fn added_removed_and_changed_nodes_are_reported() {
        let (old_document, _, kept_id) = document_with_nodes();
        let mut new_document = old_document.clone();

        // Rename the kept node.
        if let Some(node) = new_document.pages[0].get_mut(kept_id) {
            node.name = "Renamed".to_owned();
            node.opacity = 0.5;
        }
        // Add a node.
        let added_id = NodeId::new_v4();
        insert(
            &mut new_document.pages[0],
            Node::with_id(
                added_id,
                NodeKind::Ellipse,
                "Circle",
                Transform::new(10.0, 10.0, 8.0, 8.0),
            ),
        );

        let diff = diff_documents(&old_document, &new_document).expect("diff");
        assert_eq!(diff.summary.nodes_added, 1);
        assert_eq!(diff.summary.nodes_removed, 0);
        assert_eq!(diff.summary.nodes_changed, 1);
        assert_eq!(diff.pages.len(), 1);
        let page = &diff.pages[0];
        assert_eq!(page.status, "changed");
        assert_eq!(page.added_nodes, vec![added_id.to_string()]);
        assert_eq!(page.changed_nodes.len(), 1);
        assert_eq!(page.changed_nodes[0].node_id, kept_id.to_string());
        assert_eq!(
            page.changed_nodes[0].changed_fields,
            vec!["name".to_owned(), "opacity".to_owned()]
        );
    }

    #[test]
    fn removed_node_is_reported() {
        let (old_document, _, kept_id) = document_with_nodes();
        let mut new_document = old_document.clone();
        new_document.pages[0].nodes.shift_remove(&kept_id);
        new_document.pages[0].roots.clear();
        let diff = diff_documents(&old_document, &new_document).expect("diff");
        assert_eq!(diff.summary.nodes_removed, 1);
        assert_eq!(diff.pages[0].removed_nodes, vec![kept_id.to_string()]);
    }

    #[test]
    fn added_page_is_reported() {
        let old_document = Document::new("Base");
        let mut new_document = old_document.clone();
        new_document.add_page("Second");
        let diff = diff_documents(&old_document, &new_document).expect("diff");
        assert_eq!(diff.summary.pages_added, 1);
        assert_eq!(diff.pages.len(), 1);
        assert_eq!(diff.pages[0].status, "added");
    }

    #[test]
    fn changed_fields_are_sorted_and_complete() {
        let old = serde_json::json!({"name": "a", "opacity": 1.0});
        let new = serde_json::json!({"name": "b", "visible": true});
        let fields = changed_top_level_fields(&old, &new);
        assert_eq!(fields, vec!["name", "opacity", "visible"]);
    }

    #[test]
    fn unparseable_pages_are_skipped() {
        let old = serde_json::json!({"pages": [{"name": "no id"}]});
        let new = serde_json::json!({"pages": []});
        let diff = diff_document_json(&old, &new);
        assert!(diff.is_empty());
    }
}
