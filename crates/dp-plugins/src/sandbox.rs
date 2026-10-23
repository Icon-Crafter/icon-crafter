//! Op-scoped plugin sandbox.
//!
//! Plugins do not run arbitrary JavaScript/WASM and have no direct access to the
//! editor's document, filesystem or network. A plugin's host-provided handler
//! may only issue typed [`SandboxRequest`]s; the sandbox enforces the manifest's
//! declared permissions per request, validates arguments against the live
//! document and turns mutations into *proposals* ([`SandboxProposal`]) that the
//! editor applies as normal undoable transactions after confirmation.

use dp_model::{Document, Node, NodeId, NodePatch, PageId, VariableId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::manifest::PluginManifest;
use crate::permissions::Grants;

/// Maximum nodes returned by one request.
pub const MAX_NODES_PER_REQUEST: usize = 1000;
/// Maximum nodes affected by one proposal.
pub const MAX_NODES_PER_PROPOSAL: usize = 100;
/// Maximum string length accepted in proposal payloads.
pub const MAX_STRING_BYTES: usize = 4096;

/// What a plugin can ask the host to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SandboxRequest {
    /// Read-only document summary (pages, counts, active page).
    GetDocumentSummary,
    /// Read the current selection ids.
    GetSelection,
    /// Read one node's full data.
    GetNode {
        /// Node to read.
        node: NodeId,
    },
    /// Read the children of a page.
    GetPageNodes {
        /// Page to read.
        page: PageId,
        /// Maximum nodes (clamped to [`MAX_NODES_PER_REQUEST`]).
        #[serde(default = "default_limit")]
        limit: usize,
    },
    /// Read a variable definition.
    GetVariable {
        /// Variable to read.
        variable: VariableId,
    },
    /// Propose creating a node.
    ProposeInsert {
        /// Page to insert into.
        page: PageId,
        /// Node data (ids of descendants are re-mapped by the host).
        node: Node,
        /// Parent to insert under (None = page root).
        parent: Option<NodeId>,
    },
    /// Propose patching a node.
    ProposePatch {
        /// Node to patch.
        node: NodeId,
        /// Patch payload.
        patch: NodePatch,
    },
    /// Propose deleting nodes.
    ProposeDelete {
        /// Nodes to delete (at most [`MAX_NODES_PER_PROPOSAL`]).
        nodes: Vec<NodeId>,
    },
    /// Propose changing the selection.
    ProposeSelection {
        /// New selection.
        nodes: Vec<NodeId>,
    },
}

fn default_limit() -> usize {
    200
}

/// Context the host supplies when executing a request.
#[derive(Debug, Clone)]
pub struct SandboxContext<'a> {
    /// Current document.
    pub document: &'a Document,
    /// Current selection.
    pub selection: &'a [NodeId],
}

/// A validated mutation proposal for the editor to apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SandboxProposal {
    /// Insert a node under a parent.
    Insert {
        /// Target page.
        page: PageId,
        /// Node payload.
        node: Node,
        /// Parent (None = root).
        parent: Option<NodeId>,
    },
    /// Patch a node.
    Patch {
        /// Target page.
        page: PageId,
        /// Target node.
        node: NodeId,
        /// Patch payload.
        patch: NodePatch,
    },
    /// Delete nodes (top-level ids already filtered of descendants).
    Delete {
        /// Target page.
        page: PageId,
        /// Nodes to delete.
        nodes: Vec<NodeId>,
    },
    /// Change selection.
    Selection {
        /// New selection ids.
        nodes: Vec<NodeId>,
    },
}

/// Responses to read requests.
///
/// Nodes dominate the size, but responses are short-lived and moved into the
/// host, so the enum is not boxed (clippy allows the size difference here).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SandboxResponse {
    /// Document summary.
    Summary {
        /// Active page id.
        active_page: PageId,
        /// Page count.
        pages: usize,
        /// Node count of the active page.
        nodes: usize,
        /// Selection count.
        selection: usize,
    },
    /// Selection ids.
    Selection {
        /// Selected ids.
        nodes: Vec<NodeId>,
    },
    /// One node.
    Node {
        /// The node.
        node: Box<Node>,
    },
    /// A chunk of page nodes.
    Nodes {
        /// Nodes in paint order.
        nodes: Vec<Node>,
        /// Whether more nodes exist beyond `limit`.
        truncated: bool,
    },
    /// A variable.
    Variable {
        /// Variable id.
        id: VariableId,
        /// Collection id.
        collection_id: dp_model::CollectionId,
        /// Name.
        name: String,
        /// Resolved type.
        kind: String,
    },
    /// A validated mutation proposal.
    Proposed {
        /// Proposal payload.
        proposal: SandboxProposal,
    },
}

/// Sandbox errors.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SandboxError {
    /// The plugin's grants do not include the required permission.
    #[error("plugin is not granted '{0}'")]
    Denied(&'static str),
    /// Requested entity does not exist.
    #[error("unknown {kind}: {id}")]
    Unknown {
        /// Entity kind.
        kind: &'static str,
        /// Entity id.
        id: String,
    },
    /// Request payload invalid or above limits.
    #[error("invalid request: {0}")]
    Invalid(String),
}

/// Executes one request with authorization and validation.
pub fn execute(
    manifest: &PluginManifest,
    grants: &Grants,
    context: &SandboxContext<'_>,
    request: &SandboxRequest,
) -> Result<SandboxResponse, SandboxError> {
    match request {
        SandboxRequest::GetDocumentSummary | SandboxRequest::GetNode { .. } | SandboxRequest::GetPageNodes { .. } => {
            authorize(grants, "read_document")?;
        }
        SandboxRequest::GetSelection => authorize(grants, "read_selection")?,
        SandboxRequest::GetVariable { .. } => authorize(grants, "read_variables")?,
        SandboxRequest::ProposeInsert { .. }
        | SandboxRequest::ProposePatch { .. }
        | SandboxRequest::ProposeDelete { .. } => authorize(grants, "write_document")?,
        SandboxRequest::ProposeSelection { .. } => authorize(grants, "write_selection")?,
    }
    // A manifest must actually declare what it was granted, so an injected grant
    // cannot silently expand a plugin beyond its declared surface.
    let declared = crate::permissions::Grants::from_permissions(&manifest.permissions);
    let required = match request {
        SandboxRequest::GetDocumentSummary
        | SandboxRequest::GetNode { .. }
        | SandboxRequest::GetPageNodes { .. } => "read_document",
        SandboxRequest::GetSelection => "read_selection",
        SandboxRequest::GetVariable { .. } => "read_variables",
        SandboxRequest::ProposeInsert { .. }
        | SandboxRequest::ProposePatch { .. }
        | SandboxRequest::ProposeDelete { .. } => "write_document",
        SandboxRequest::ProposeSelection { .. } => "write_selection",
    };
    if !declared.allows(required) {
        return Err(SandboxError::Denied("undeclared permission"));
    }

    match request {
        SandboxRequest::GetDocumentSummary => Ok(SandboxResponse::Summary {
            active_page: context.document.active_page_id(),
            pages: context.document.pages.len(),
            nodes: context.document.active_page().nodes.len(),
            selection: context.selection.len(),
        }),
        SandboxRequest::GetSelection => Ok(SandboxResponse::Selection {
            nodes: context.selection.to_vec(),
        }),
        SandboxRequest::GetNode { node } => {
            let found = context
                .document
                .page_containing(*node)
                .and_then(|page| context.document.page(page))
                .and_then(|page| page.nodes.get(node))
                .cloned()
                .ok_or_else(|| SandboxError::Unknown {
                    kind: "node",
                    id: node.to_string(),
                })?;
            Ok(SandboxResponse::Node {
                node: Box::new(found),
            })
        }
        SandboxRequest::GetPageNodes { page, limit } => {
            let page_data = context.document.page(*page).ok_or_else(|| SandboxError::Unknown {
                kind: "page",
                id: page.to_string(),
            })?;
            let limit = (*limit).clamp(1, MAX_NODES_PER_REQUEST);
            let mut nodes = Vec::new();
            let mut truncated = false;
            for id in page_data.nodes.keys() {
                if nodes.len() >= limit {
                    truncated = true;
                    break;
                }
                if let Some(node) = page_data.nodes.get(id) {
                    nodes.push(node.clone());
                }
            }
            Ok(SandboxResponse::Nodes { nodes, truncated })
        }
        SandboxRequest::GetVariable { variable } => {
            let found = context
                .document
                .variables
                .variables
                .get(variable)
                .ok_or_else(|| SandboxError::Unknown {
                    kind: "variable",
                    id: variable.to_string(),
                })?;
            Ok(SandboxResponse::Variable {
                id: found.id,
                collection_id: found.collection_id,
                name: found.name.clone(),
                kind: format!("{:?}", found.resolved_type).to_lowercase(),
            })
        }
        SandboxRequest::ProposeInsert {
            page,
            node,
            parent,
        } => {
            let page_data = context.document.page(*page).ok_or_else(|| SandboxError::Unknown {
                kind: "page",
                id: page.to_string(),
            })?;
            if let Some(parent_id) = parent {
                let parent_node = page_data.nodes.get(parent_id).ok_or_else(|| SandboxError::Unknown {
                    kind: "node",
                    id: parent_id.to_string(),
                })?;
                if !parent_node.kind.is_container() {
                    return Err(SandboxError::Invalid(format!(
                        "node '{}' cannot contain children",
                        parent_node.name
                    )));
                }
            }
            check_strings(&node.name)?;
            Ok(SandboxResponse::Proposed {
                proposal: SandboxProposal::Insert {
                    page: *page,
                    node: node.clone(),
                    parent: *parent,
                },
            })
        }
        SandboxRequest::ProposePatch { node, patch } => {
            let page_id = context.document.page_containing(*node).ok_or_else(|| {
                SandboxError::Unknown {
                    kind: "node",
                    id: node.to_string(),
                }
            })?;
            if let Some(name) = &patch.name {
                check_strings(name)?;
            }
            Ok(SandboxResponse::Proposed {
                proposal: SandboxProposal::Patch {
                    page: page_id,
                    node: *node,
                    patch: patch.clone(),
                },
            })
        }
        SandboxRequest::ProposeDelete { nodes } => {
            if nodes.len() > MAX_NODES_PER_PROPOSAL {
                return Err(SandboxError::Invalid(format!(
                    "cannot delete more than {MAX_NODES_PER_PROPOSAL} nodes at once"
                )));
            }
            let mut page_id = None;
            for node in nodes {
                let found = context.document.page_containing(*node).ok_or_else(|| {
                    SandboxError::Unknown {
                        kind: "node",
                        id: node.to_string(),
                    }
                })?;
                match page_id {
                    None => page_id = Some(found),
                    Some(existing) if existing != found => {
                        return Err(SandboxError::Invalid(
                            "all deleted nodes must live on one page".to_owned(),
                        ))
                    }
                    Some(_) => {}
                }
            }
            let page = page_id.ok_or_else(|| SandboxError::Invalid("empty delete list".to_owned()))?;
            Ok(SandboxResponse::Proposed {
                proposal: SandboxProposal::Delete {
                    page,
                    nodes: nodes.clone(),
                },
            })
        }
        SandboxRequest::ProposeSelection { nodes } => {
            if nodes.len() > MAX_NODES_PER_PROPOSAL {
                return Err(SandboxError::Invalid(format!(
                    "cannot select more than {MAX_NODES_PER_PROPOSAL} nodes at once"
                )));
            }
            for node in nodes {
                if context.document.node(*node).is_none() {
                    return Err(SandboxError::Unknown {
                        kind: "node",
                        id: node.to_string(),
                    });
                }
            }
            Ok(SandboxResponse::Proposed {
                proposal: SandboxProposal::Selection { nodes: nodes.clone() },
            })
        }
    }
}

fn authorize(grants: &Grants, key: &'static str) -> Result<(), SandboxError> {
    if grants.allows(key) {
        Ok(())
    } else {
        Err(SandboxError::Denied(key))
    }
}

fn check_strings(value: &str) -> Result<(), SandboxError> {
    if value.len() > MAX_STRING_BYTES {
        Err(SandboxError::Invalid(format!(
            "string exceeds {MAX_STRING_BYTES} bytes"
        )))
    } else {
        Ok(())
    }
}

/// Whether a JSON value would be accepted for a plugin setting.
pub fn validate_setting_value(manifest: &PluginManifest, key: &str, value: &Value) -> bool {
    manifest
        .setting(key)
        .is_some_and(|setting| setting.accepts(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::PluginManifest;
    use crate::permissions::Grants;

    fn manifest() -> PluginManifest {
        PluginManifest::from_json(
            r#"{
              "manifest_version": 1,
              "id": "sandbox-test",
              "name": "Sandbox Test",
              "version": "0.1.0",
              "api_version": 1,
              "permissions": ["read_document", "write_document", "write_selection"]
            }"#,
        )
        .unwrap()
    }

    fn document() -> Document {
        let mut document = Document::new("Sandbox");
        let page = document.active_page_id();
        let node = Node::rectangle("Card", 0.0, 0.0, 100.0, 50.0);
        document.page_mut(page).unwrap().insert_node(node, None, None).unwrap();
        document
    }

    #[test]
    fn declared_permissions_are_required() {
        let manifest = manifest();
        let document = document();
        let context = SandboxContext {
            document: &document,
            selection: &[],
        };
        let grants = Grants::from_permissions(&manifest.permissions);
        assert!(execute(&manifest, &grants, &context, &SandboxRequest::GetDocumentSummary).is_ok());
        // Revoking both the write and its implied read is denied.
        let mut revoked = grants.clone();
        revoked.revoke("read_document");
        revoked.revoke("write_document");
        assert!(matches!(
            execute(&manifest, &revoked, &context, &SandboxRequest::GetDocumentSummary),
            Err(SandboxError::Denied(_))
        ));
        // Grants outside the manifest are rejected.
        let mut injected = grants.clone();
        injected.insert("write_variables");
        assert!(matches!(
            execute(
                &manifest,
                &injected,
                &context,
                &SandboxRequest::GetVariable {
                    variable: VariableId::new_v4()
                }
            ),
            Err(SandboxError::Denied(_))
        ));
    }

    #[test]
    fn reads_validate_existence() {
        let manifest = manifest();
        let document = document();
        let node_id = *document.active_page().roots.first().unwrap();
        let context = SandboxContext {
            document: &document,
            selection: &[node_id],
        };
        let grants = Grants::from_permissions(&manifest.permissions);
        let response = execute(
            &manifest,
            &grants,
            &context,
            &SandboxRequest::GetNode { node: node_id },
        )
        .unwrap();
        match response {
            SandboxResponse::Node { node } => assert_eq!(node.name, "Card"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            execute(
                &manifest,
                &grants,
                &context,
                &SandboxRequest::GetNode {
                    node: NodeId::new_v4()
                }
            ),
            Err(SandboxError::Unknown { kind: "node", .. })
        ));
    }

    #[test]
    fn proposals_are_validated_not_applied() {
        let manifest = manifest();
        let document = document();
        let page = document.active_page_id();
        let context = SandboxContext {
            document: &document,
            selection: &[],
        };
        let grants = Grants::from_permissions(&manifest.permissions);
        let new_node = Node::rectangle("Proposed", 10.0, 10.0, 20.0, 20.0);
        let response = execute(
            &manifest,
            &grants,
            &context,
            &SandboxRequest::ProposeInsert {
                page,
                node: new_node.clone(),
                parent: None,
            },
        )
        .unwrap();
        match response {
            SandboxResponse::Proposed { proposal } => {
                assert!(matches!(proposal, SandboxProposal::Insert { .. }));
            }
            other => panic!("unexpected {other:?}"),
        }
        // The document did not change.
        assert_eq!(document.active_page().nodes.len(), 1);
    }

    #[test]
    fn invalid_parents_and_limits_are_rejected() {
        let manifest = manifest();
        let document = document();
        let page = document.active_page_id();
        let context = SandboxContext {
            document: &document,
            selection: &[],
        };
        let grants = Grants::from_permissions(&manifest.permissions);
        let node = Node::rectangle("Child", 0.0, 0.0, 10.0, 10.0);
        assert!(execute(
            &manifest,
            &grants,
            &context,
            &SandboxRequest::ProposeInsert {
                page,
                node,
                parent: Some(NodeId::new_v4()),
            }
        )
        .is_err());
        assert!(execute(
            &manifest,
            &grants,
            &context,
            &SandboxRequest::ProposeDelete {
                nodes: vec![NodeId::new_v4()]
            }
        )
        .is_err());
    }

    #[test]
    fn setting_values_are_type_checked() {
        let manifest = PluginManifest::from_json(
            r#"{
              "manifest_version": 1,
              "id": "settings-test",
              "name": "Settings Test",
              "version": "0.1.0",
              "api_version": 1,
              "settings": [{"type": "select", "id": "mode", "label": "Mode", "options": ["a", "b"], "default": "a"}]
            }"#,
        )
        .unwrap();
        assert!(validate_setting_value(&manifest, "mode", &Value::String("b".into())));
        assert!(!validate_setting_value(&manifest, "mode", &Value::String("c".into())));
        assert!(!validate_setting_value(&manifest, "missing", &Value::Null));
    }
}
