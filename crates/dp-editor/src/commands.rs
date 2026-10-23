//! Command dispatcher and document commands shared by the UI, keyboard
//! shortcuts and the future AI/plugin operation layer.
//!
//! Every command mutates the document through validated transactions, so all
//! changes are undoable and observable. Unknown or not-yet-implemented commands
//! return an explicit `unsupported` outcome instead of silently doing nothing.

use std::collections::{HashMap, HashSet};

use dp_geom::BooleanOp;
use dp_model::ops::{Align, DistributeAxis};
use dp_model::{
    CollectionId, Color, ComponentId, Edit, ModeId, ModelError, Node, NodeId, NodeKind, NodePatch,
    Page, PageId, StyleId, Transaction, Transform, VariableId, VariableValue,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ClipboardNode, EditorError, EditorState, ToolId};

/// Result of dispatching one command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CommandOutcome {
    /// Command ran.
    Ok {
        /// Whether the document changed.
        changed: bool,
    },
    /// Command name is unknown or not supported by this build.
    Unsupported {
        /// Human readable reason.
        message: String,
    },
    /// Command failed validation or hit an internal error.
    Error {
        /// Machine-readable error code: validation, io, unsupported, internal.
        code: String,
        /// Human readable message.
        message: String,
    },
}

impl EditorState {
    /// Dispatches a command by name with JSON arguments.
    pub fn command(&mut self, name: &str, args: &Value) -> CommandOutcome {
        match self.execute_command(name, args) {
            Ok(changed) => CommandOutcome::Ok { changed },
            Err(EditorError::Unsupported(message)) => CommandOutcome::Unsupported { message },
            Err(EditorError::InvalidArguments(message)) => CommandOutcome::Error {
                code: "validation".to_owned(),
                message,
            },
            Err(other) => CommandOutcome::Error {
                code: "internal".to_owned(),
                message: other.to_string(),
            },
        }
    }

    fn execute_command(&mut self, name: &str, args: &Value) -> Result<bool, EditorError> {
        match name {
            "undo" => self.undo(),
            "redo" => self.redo(),
            "delete" => delete_selection(self),
            "duplicate" => duplicate_selection(self),
            "copy" => {
                copy_selection(self);
                Ok(false)
            }
            "cut" => {
                copy_selection(self);
                delete_selection(self)
            }
            "paste" => paste_clipboard(self),
            "select_all" => {
                select_all(self);
                Ok(false)
            }
            "select_none" => {
                self.clear_selection();
                Ok(false)
            }
            "select_inverse" => {
                select_inverse(self);
                Ok(false)
            }
            "select_nodes" => {
                let ids = pid_list(args, "ids")?.ok_or_else(|| {
                    EditorError::InvalidArguments("select_nodes requires ids".to_owned())
                })?;
                let additive = args
                    .get("additive")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if additive {
                    for id in ids {
                        self.select(id, true);
                    }
                } else {
                    self.select_many(&ids);
                }
                Ok(false)
            }
            "set_tool" => {
                let tool_value = args.get("tool").cloned().ok_or_else(|| {
                    EditorError::InvalidArguments("set_tool requires tool".to_owned())
                })?;
                let tool: ToolId = serde_json::from_value(tool_value)
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?;
                self.set_tool(tool);
                Ok(false)
            }
            "set_viewport" => {
                let width = number(args, "width")?;
                let height = number(args, "height")?;
                let dpr = args.get("dpr").and_then(Value::as_f64).unwrap_or(1.0);
                self.set_viewport_size(width, height, dpr);
                Ok(false)
            }
            "zoom_in" => {
                self.zoom_in();
                Ok(false)
            }
            "zoom_out" => {
                self.zoom_out();
                Ok(false)
            }
            "zoom_100" => {
                self.zoom_100();
                Ok(false)
            }
            "zoom_fit" => {
                self.fit_to_content();
                Ok(false)
            }
            "zoom_selection" => {
                self.zoom_to_selection();
                Ok(false)
            }
            "zoom_to" => {
                self.zoom_to(number(args, "zoom")?);
                Ok(false)
            }
            "pan_by" => {
                let dx = number(args, "dx")?;
                let dy = number(args, "dy")?;
                self.viewport.x += dx;
                self.viewport.y += dy;
                Ok(false)
            }
            "rename_node" => {
                let id = args
                    .get("id")
                    .cloned()
                    .map(serde_json::from_value::<NodeId>)
                    .transpose()
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?
                    .ok_or_else(|| {
                        EditorError::InvalidArguments("rename_node requires id".to_owned())
                    })?;
                let name = text_arg(args, "name")?;
                rename_node(self, id, name)
            }
            "toggle_visible" => {
                let ids = resolve_ids(self, args)?;
                toggle_flag(
                    self,
                    &ids,
                    |node| !node.visible,
                    |patch, value| {
                        patch.visible = Some(value);
                    },
                    "Toggle visibility",
                )
            }
            "toggle_locked" => {
                let ids = resolve_ids(self, args)?;
                toggle_flag(
                    self,
                    &ids,
                    |node| !node.locked,
                    |patch, value| {
                        patch.locked = Some(value);
                    },
                    "Toggle lock",
                )
            }
            "set_opacity" => {
                let ids = resolve_ids(self, args)?;
                let opacity = number(args, "opacity")?.clamp(0.0, 1.0);
                set_opacity(self, &ids, opacity)
            }
            "nudge" => {
                let ids = resolve_ids(self, args)?;
                let dx = number(args, "dx")?;
                let dy = number(args, "dy")?;
                nudge_selection(self, dx, dy, &ids)
            }
            "align" => {
                let ids = resolve_ids(self, args)?;
                let align = parse_align(text_arg(args, "align")?)?;
                align_selection(self, &ids, align)
            }
            "distribute" => {
                let ids = resolve_ids(self, args)?;
                let axis = match text_arg(args, "axis")? {
                    "horizontal" => DistributeAxis::Horizontal,
                    "vertical" => DistributeAxis::Vertical,
                    other => {
                        return Err(EditorError::InvalidArguments(format!(
                            "unknown distribute axis {other}"
                        )));
                    }
                };
                let spacing = args.get("spacing").and_then(Value::as_f64).unwrap_or(0.0);
                distribute_selection(self, &ids, axis, spacing)
            }
            "boolean" => {
                let ids = resolve_ids(self, args)?;
                let op = match text_arg(args, "op")? {
                    "union" => BooleanOp::Union,
                    "subtract" => BooleanOp::Subtract,
                    "intersect" => BooleanOp::Intersect,
                    "exclude" => BooleanOp::Exclude,
                    other => {
                        return Err(EditorError::InvalidArguments(format!(
                            "unknown boolean op {other}"
                        )));
                    }
                };
                boolean_selection(self, &ids, op)
            }
            "group" => group_selection(self),
            "ungroup" => ungroup_selection(self),
            "bring_to_front" => reorder_selection(self, Reorder::Front),
            "bring_forward" => reorder_selection(self, Reorder::Forward),
            "send_backward" => reorder_selection(self, Reorder::Backward),
            "send_to_back" => reorder_selection(self, Reorder::Back),
            "move_to_parent" => {
                let ids = resolve_ids(self, args)?;
                let parent: Option<NodeId> = args
                    .get("parent")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?
                    .flatten();
                let mut changed = false;
                for id in ids {
                    changed |= move_node(self, id, parent)?;
                }
                Ok(changed)
            }
            "reorder" => {
                let id = args
                    .get("node_id")
                    .cloned()
                    .map(serde_json::from_value::<NodeId>)
                    .transpose()
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?
                    .ok_or_else(|| {
                        EditorError::InvalidArguments("reorder requires node_id".to_owned())
                    })?;
                let parent: Option<NodeId> = args
                    .get("parent")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?
                    .flatten();
                let index = args
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize);
                let transaction =
                    dp_model::ops::reorder(&self.document, id, parent, index, "Reorder")?;
                self.apply(transaction)?;
                Ok(true)
            }
            "set_active_page" => {
                let page = parse_page_id(args)?;
                if self.document.page(page).is_none() {
                    return Err(EditorError::InvalidArguments("unknown page id".to_owned()));
                }
                self.document.active_page = page;
                self.selection.clear();
                self.invalidate_layout();
                Ok(false)
            }
            "add_page" => {
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|value| value.to_owned())
                    .unwrap_or_else(|| format!("Page {}", self.document.pages.len() + 1));
                let page = Page::new(name);
                let id = page.id;
                self.document.pages.push(page);
                self.document.active_page = id;
                self.selection.clear();
                self.invalidate_layout();
                self.dirty = true;
                Ok(true)
            }
            "rename_page" => {
                let page_id = parse_page_id(args)?;
                let name = text_arg(args, "name")?.to_owned();
                let page = self
                    .document
                    .page_mut(page_id)
                    .ok_or_else(|| EditorError::InvalidArguments("unknown page id".to_owned()))?;
                page.name = name;
                self.dirty = true;
                Ok(true)
            }
            "delete_page" => {
                let page_id = parse_page_id(args)?;
                if self.document.pages.len() <= 1 {
                    return Err(EditorError::InvalidArguments(
                        "cannot delete the last remaining page".to_owned(),
                    ));
                }
                let before = self.document.pages.len();
                self.document.pages.retain(|page| page.id != page_id);
                if self.document.pages.len() == before {
                    return Err(EditorError::InvalidArguments("unknown page id".to_owned()));
                }
                if self.document.active_page == page_id {
                    self.document.active_page = self.document.pages[0].id;
                }
                self.selection.clear();
                self.invalidate_layout();
                self.dirty = true;
                Ok(true)
            }
            "text_edit_begin" => {
                let node_id = args
                    .get("node_id")
                    .cloned()
                    .map(serde_json::from_value::<NodeId>)
                    .transpose()
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?
                    .ok_or_else(|| {
                        EditorError::InvalidArguments("text_edit_begin requires node_id".to_owned())
                    })?;
                self.text_edit_begin(node_id)?;
                Ok(true)
            }
            "text_edit_input" => {
                let characters = text_arg(args, "characters")?.to_owned();
                let caret = args.get("caret").and_then(Value::as_u64).unwrap_or(0) as usize;
                let selection = parse_caret_selection(args)?;
                self.text_edit_input(&characters, caret, selection)?;
                Ok(true)
            }
            "text_edit_set_caret" => {
                let caret = args.get("caret").and_then(Value::as_u64).unwrap_or(0) as usize;
                let selection = parse_caret_selection(args)?;
                self.text_edit_set_caret(caret, selection)?;
                Ok(false)
            }
            "text_edit_commit" => self.text_edit_commit(),
            "text_edit_cancel" => Ok(self.text_edit_cancel()),
            "apply_patch" => {
                let ids = pid_list(args, "ids")?.unwrap_or_else(|| self.selection.clone());
                if ids.is_empty() {
                    return Err(EditorError::InvalidArguments(
                        "apply_patch requires selected nodes or ids".to_owned(),
                    ));
                }
                let patch_value = args.get("patch").cloned().ok_or_else(|| {
                    EditorError::InvalidArguments("apply_patch requires patch".to_owned())
                })?;
                let patch: NodePatch = serde_json::from_value(patch_value)
                    .map_err(|error| EditorError::InvalidArguments(error.to_string()))?;
                let label = args
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("Edit")
                    .to_owned();
                let coalesce = args.get("coalesce_key").and_then(Value::as_str);
                let transaction =
                    dp_model::ops::patch_nodes(&self.document, &ids, patch, label, coalesce)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "export_png" => {
                let path = text_arg(args, "path")?;
                let scale = args.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
                self.export_png(path, scale)?;
                Ok(false)
            }
            "export_svg" => {
                let path = text_arg(args, "path")?;
                self.export_svg(path)?;
                Ok(false)
            }
            "export_pdf" => {
                let path = text_arg(args, "path")?;
                self.export_pdf(path)?;
                Ok(false)
            }
            "save_document" => {
                match args.get("path").and_then(Value::as_str) {
                    Some(path) => {
                        self.save_as(path)?;
                    }
                    None => {
                        self.save()?;
                    }
                }
                Ok(false)
            }
            "create_component" => {
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Component");
                let ids = top_level_selection(self);
                if ids.is_empty() {
                    return Err(EditorError::InvalidArguments(
                        "select one or more layers to create a component".to_owned(),
                    ));
                }
                let page = self.document.active_page_id();
                let before: HashSet<NodeId> =
                    self.document.active_page().nodes.keys().copied().collect();
                let transaction = dp_model::create_component(&self.document, page, &ids, name)?;
                self.apply(transaction)?;
                self.selection = new_nodes(self, &before);
                self.set_status(format!("Created component {name}"));
                Ok(true)
            }
            "detach_instance" => {
                let ids = resolve_ids(self, args)?;
                let mut changed = false;
                for id in ids {
                    let is_instance = self
                        .document
                        .node(id)
                        .map(|node| node.kind == NodeKind::Instance)
                        .unwrap_or(false);
                    if !is_instance {
                        continue;
                    }
                    let page = self.document.active_page_id();
                    let transaction = dp_model::detach_instance(&self.document, page, id)?;
                    self.apply(transaction)?;
                    changed = true;
                }
                if !changed {
                    return Err(EditorError::InvalidArguments(
                        "select a component instance to detach".to_owned(),
                    ));
                }
                self.set_status("Detached instance".to_owned());
                Ok(true)
            }
            "place_instance" => {
                let component_id: ComponentId = json_field(args, "component_id")?;
                let parent = optional_id(args, "parent_id")?;
                let x = args.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = args.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                let page = self.document.active_page_id();
                let before: HashSet<NodeId> =
                    self.document.active_page().nodes.keys().copied().collect();
                let transaction =
                    dp_model::create_instance(&self.document, page, component_id, parent, x, y)?;
                self.apply(transaction)?;
                self.selection = new_nodes(self, &before);
                Ok(true)
            }
            "create_style_from_node" => {
                let ids = resolve_ids(self, args)?;
                let id = ids.first().copied().ok_or_else(|| {
                    EditorError::InvalidArguments(
                        "select a layer to create a style from".to_owned(),
                    )
                })?;
                let name = args.get("name").and_then(Value::as_str).unwrap_or("Style");
                let transaction = dp_model::create_style_from_node(&self.document, id, name)?;
                self.apply(transaction)?;
                self.set_status(format!("Created style {name}"));
                Ok(true)
            }
            "apply_style" => {
                let ids = resolve_ids(self, args)?;
                let style_id: StyleId = json_field(args, "style_id")?;
                let transaction = dp_model::apply_style(&self.document, &ids, style_id)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "rename_style" => {
                let style_id: StyleId = json_field(args, "style_id")?;
                let name = text_arg(args, "name")?;
                let transaction = dp_model::rename_style(&self.document, style_id, name)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "delete_style" => {
                let style_id: StyleId = json_field(args, "style_id")?;
                let transaction = dp_model::delete_style(&self.document, style_id)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "create_variable_collection" => {
                let name = text_arg(args, "name")?;
                let transaction = dp_model::create_variable_collection(&self.document, name)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "create_color_variable" => {
                let collection = args
                    .get("collection")
                    .and_then(Value::as_str)
                    .unwrap_or("Theme");
                let name = text_arg(args, "name")?;
                let hex = text_arg(args, "color")?;
                let color = Color::from_hex(hex).ok_or_else(|| {
                    EditorError::InvalidArguments(format!("invalid colour {hex}"))
                })?;
                let transaction =
                    dp_model::create_color_variable(&self.document, collection, name, color)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "create_float_variable" => {
                let collection = args
                    .get("collection")
                    .and_then(Value::as_str)
                    .unwrap_or("Theme");
                let name = text_arg(args, "name")?;
                let value = number(args, "value")?;
                let transaction =
                    dp_model::create_float_variable(&self.document, collection, name, value)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "set_variable_value" => {
                let variable_id: VariableId = json_field(args, "variable_id")?;
                let collection_id = self
                    .document
                    .variables
                    .variable(variable_id)
                    .map(|variable| variable.collection_id)
                    .ok_or_else(|| {
                        EditorError::InvalidArguments("unknown variable id".to_owned())
                    })?;
                let mode_id = match args.get("mode_id").filter(|value| !value.is_null()) {
                    Some(value) => serde_json::from_value(value.clone())
                        .map_err(|error| EditorError::InvalidArguments(error.to_string()))?,
                    None => {
                        let collection = self
                            .document
                            .variables
                            .collection(collection_id)
                            .ok_or_else(|| {
                                EditorError::InvalidArguments(
                                    "the variable collection is missing".to_owned(),
                                )
                            })?;
                        self.document
                            .settings
                            .active_modes
                            .get(&collection_id)
                            .copied()
                            .unwrap_or(collection.default_mode)
                    }
                };
                let value = parse_variable_value(args)?;
                let transaction =
                    dp_model::set_variable_value(&self.document, variable_id, mode_id, value)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "set_active_mode" => {
                let collection_id: CollectionId = json_field(args, "collection_id")?;
                let mode_id: ModeId = json_field(args, "mode_id")?;
                let transaction =
                    dp_model::set_active_mode(&self.document, collection_id, mode_id)?;
                self.apply(transaction)?;
                Ok(true)
            }
            "create_component_set" | "swap_instance" => Err(EditorError::Unsupported(format!(
                "{name} is not implemented in this build yet"
            ))),
            "mask_with_selection" | "flatten" | "outline_stroke" => Err(EditorError::Unsupported(
                format!("{name} is not implemented in this build yet"),
            )),
            "play" => {
                self.prototype_play(None)?;
                Ok(false)
            }
            "stop" => {
                self.prototype_stop();
                Ok(false)
            }
            "add_interaction" => {
                let node_id = optional_id(args, "node_id")?
                    .or_else(|| self.selection.first().copied())
                    .ok_or_else(|| {
                        EditorError::InvalidArguments(
                            "select a layer to attach the interaction to".to_owned(),
                        )
                    })?;
                let trigger_value = args.get("trigger").cloned().ok_or_else(|| {
                    EditorError::InvalidArguments("trigger is required".to_owned())
                })?;
                let trigger: dp_model::prototype::Trigger = serde_json::from_value(trigger_value)
                    .map_err(|error| {
                    EditorError::InvalidArguments(format!("invalid trigger: {error}"))
                })?;
                let actions_value = args
                    .get("actions")
                    .or_else(|| args.get("action"))
                    .cloned()
                    .ok_or_else(|| {
                        EditorError::InvalidArguments("actions are required".to_owned())
                    })?;
                let actions: Vec<dp_model::prototype::Action> = if actions_value.is_array() {
                    serde_json::from_value(actions_value).map_err(|error| {
                        EditorError::InvalidArguments(format!("invalid actions: {error}"))
                    })?
                } else {
                    vec![serde_json::from_value(actions_value).map_err(|error| {
                        EditorError::InvalidArguments(format!("invalid action: {error}"))
                    })?]
                };
                let interaction = dp_model::prototype::Interaction {
                    id: dp_model::InteractionId::new_v4(),
                    trigger,
                    actions,
                };
                self.add_interaction(node_id, interaction)?;
                Ok(true)
            }
            "remove_interaction" => {
                let node_id = optional_id(args, "node_id")?
                    .or_else(|| self.selection.first().copied())
                    .ok_or_else(|| {
                        EditorError::InvalidArguments(
                            "select a layer that owns the interaction".to_owned(),
                        )
                    })?;
                let interaction_id: dp_model::InteractionId = json_field(args, "interaction_id")?;
                Ok(self.remove_interaction(node_id, interaction_id)?)
            }
            "ai_generate_layout" => {
                let prompt: String = json_field(args, "prompt")?;
                let width = args
                    .get("width")
                    .and_then(Value::as_u64)
                    .unwrap_or(1440)
                    .min(10_000) as u32;
                let height = args
                    .get("height")
                    .and_then(Value::as_u64)
                    .unwrap_or(900)
                    .min(10_000) as u32;
                self.ai_generate_layout(&prompt, width, height)?;
                Ok(true)
            }
            "ai_apply_last" => {
                let confirmed = args
                    .get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.ai_apply_last_proposal(confirmed)?;
                Ok(true)
            }
            "ai_discard_last" => Ok(self.ai_discard_proposal()),
            "ai_rename_layers" => {
                let applied = self.ai_rename_layers()?;
                Ok(applied > 0)
            }
            "ai_text_task" => {
                let task: String = json_field(args, "task")?;
                let input: String = json_field(args, "input")?;
                self.ai_text_task(&task, &input)?;
                Ok(false)
            }
            "plugin_install" => {
                let manifest = args.get("manifest").cloned().ok_or_else(|| {
                    EditorError::InvalidArguments("missing 'manifest'".to_owned())
                })?;
                let text = match manifest {
                    Value::String(text) => text,
                    other => other.to_string(),
                };
                self.plugin_install_json(&text)?;
                Ok(true)
            }
            "plugin_install_example" => {
                self.plugin_install_example()?;
                Ok(true)
            }
            "plugin_remove" => {
                let plugin_id: String = json_field(args, "plugin_id")?;
                self.plugin_remove(&plugin_id)?;
                Ok(true)
            }
            "plugin_set_enabled" => {
                let plugin_id: String = json_field(args, "plugin_id")?;
                let enabled = args.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                self.plugin_set_enabled(&plugin_id, enabled)?;
                Ok(true)
            }
            "plugin_set_setting" => {
                let plugin_id: String = json_field(args, "plugin_id")?;
                let key: String = json_field(args, "key")?;
                let value = args
                    .get("value")
                    .cloned()
                    .ok_or_else(|| EditorError::InvalidArguments("missing 'value'".to_owned()))?;
                self.plugin_set_setting(&plugin_id, &key, value)?;
                Ok(true)
            }
            "dev_resolve_annotation" => {
                let annotation_id: uuid::Uuid = json_field(args, "annotation_id")?;
                Ok(self.dev_resolve_annotation(annotation_id))
            }
            "dev_mode_toggle" => {
                let enabled = args
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(!self.dev_mode);
                self.set_dev_mode(enabled);
                Ok(false)
            }
            "import_svg" | "import_image" => Err(EditorError::Unsupported(
                "file import is handled by the desktop shell in this build".to_owned(),
            )),
            other => Err(EditorError::Unsupported(other.to_owned())),
        }
    }
}

/// Duplicates the current selection.
pub fn duplicate_selection(state: &mut EditorState) -> Result<bool, EditorError> {
    let ids = top_level_selection(state);
    if ids.is_empty() {
        state.status = "Nothing to duplicate".to_owned();
        return Ok(false);
    }
    let transaction = dp_model::ops::duplicate_nodes(&state.document, &ids, "Duplicate")?;
    let applied = state.apply(transaction)?;
    if !applied.selection_after.is_empty() {
        state.selection = applied.selection_after.clone();
    }
    Ok(true)
}

/// Deletes the current selection.
pub fn delete_selection(state: &mut EditorState) -> Result<bool, EditorError> {
    let ids = top_level_selection(state);
    if ids.is_empty() {
        state.status = "Nothing to delete".to_owned();
        return Ok(false);
    }
    let transaction = dp_model::ops::delete_nodes(&state.document, &ids, "Delete")?;
    state.apply(transaction)?;
    state.selection.clear();
    Ok(true)
}

/// Selects every node on the active page.
pub fn select_all(state: &mut EditorState) {
    let page = state.document.active_page();
    let mut ids = Vec::new();
    for root in page.roots.clone() {
        collect_page_ids(page, root, &mut ids);
    }
    state.select_many(&ids);
}

fn select_inverse(state: &mut EditorState) {
    let page = state.document.active_page();
    let mut ids = Vec::new();
    for root in page.roots.clone() {
        collect_page_ids(page, root, &mut ids);
    }
    let selected: HashSet<NodeId> = state.selection.iter().copied().collect();
    let inverted: Vec<NodeId> = ids
        .into_iter()
        .filter(|id| !selected.contains(id))
        .collect();
    state.select_many(&inverted);
}

/// Copies the current selection into the internal clipboard.
pub fn copy_selection(state: &mut EditorState) {
    let ids = top_level_selection(state);
    let page = state.document.active_page();
    let mut entries = Vec::new();
    for id in &ids {
        if let Some(node) = page.get(*id) {
            entries.push(ClipboardNode {
                node: node.clone(),
                parent: node.parent,
            });
        }
    }
    let count = entries.len();
    state.clipboard = entries;
    state.status = format!("Copied {count} layer(s)");
}

/// Pastes clipboard contents into the active page.
pub fn paste_clipboard(state: &mut EditorState) -> Result<bool, EditorError> {
    if state.clipboard.is_empty() {
        state.status = "Clipboard is empty".to_owned();
        return Ok(false);
    }
    let page_id = state.document.active_page_id();
    let page = state.document.active_page();
    let mut new_ids: HashMap<NodeId, NodeId> = HashMap::new();
    let mut inserts: Vec<(Node, Option<NodeId>, Option<usize>)> = Vec::new();
    for entry in state.clipboard.clone() {
        let parent = entry
            .parent
            .filter(|parent_id| page.get(*parent_id).is_some());
        clone_subtree_into(page, &entry.node, parent, &mut new_ids, &mut inserts);
    }
    let Some(page_mut) = state.document.page_mut(page_id) else {
        return Err(EditorError::InvalidArguments(
            "active page missing".to_owned(),
        ));
    };
    let mut inserted_roots = Vec::new();
    for (node, parent, index) in inserts {
        let is_root = parent.is_none();
        let id = node.id;
        page_mut.insert_node(node, parent, index)?;
        if is_root {
            inserted_roots.push(id);
        }
    }
    state.invalidate_layout();
    state.dirty = true;
    state.selection = inserted_roots;
    state.status = format!("Pasted {} layer(s)", new_ids.len());
    Ok(true)
}

/// Nudges the selection, recording one transaction.
pub fn nudge_selection(
    state: &mut EditorState,
    dx: f64,
    dy: f64,
    ids: &[NodeId],
) -> Result<bool, EditorError> {
    let page = state.document.active_page_id();
    let mut transaction = Transaction::new("Nudge");
    for id in ids {
        let Some(before) = state.document.node(*id).cloned() else {
            continue;
        };
        if before.locked {
            continue;
        }
        let mut after = before.clone();
        after.transform.x += dx;
        after.transform.y += dy;
        transaction.push(Edit::SetNode {
            page,
            before,
            after,
        });
    }
    if transaction.is_empty() {
        return Ok(false);
    }
    state.apply(transaction)?;
    Ok(true)
}

/// Aligns the given nodes.
pub fn align_selection(
    state: &mut EditorState,
    ids: &[NodeId],
    align: Align,
) -> Result<bool, EditorError> {
    if ids.len() < 2 {
        return Err(EditorError::InvalidArguments(
            "align requires at least two layers".to_owned(),
        ));
    }
    let transaction = dp_model::ops::align_nodes(&state.document, ids, align, "Align")?;
    state.apply(transaction)?;
    Ok(true)
}

/// Distributes the given nodes.
pub fn distribute_selection(
    state: &mut EditorState,
    ids: &[NodeId],
    axis: DistributeAxis,
    spacing: f64,
) -> Result<bool, EditorError> {
    if ids.len() < 2 {
        return Err(EditorError::InvalidArguments(
            "distribute requires at least two layers".to_owned(),
        ));
    }
    let transaction =
        dp_model::ops::distribute_nodes(&state.document, ids, axis, spacing, "Distribute")?;
    state.apply(transaction)?;
    Ok(true)
}

/// Applies a boolean operation to the given nodes.
pub fn boolean_selection(
    state: &mut EditorState,
    ids: &[NodeId],
    op: BooleanOp,
) -> Result<bool, EditorError> {
    if ids.len() < 2 {
        return Err(EditorError::InvalidArguments(
            "boolean operations require at least two layers".to_owned(),
        ));
    }
    let before: HashSet<NodeId> = state.document.active_page().nodes.keys().copied().collect();
    let transaction = dp_model::ops::boolean_nodes(&state.document, ids, op, "Boolean")?;
    state.apply(transaction)?;
    state.selection = new_nodes(state, &before);
    Ok(true)
}

/// Groups the current selection.
pub fn group_selection(state: &mut EditorState) -> Result<bool, EditorError> {
    let ids = top_level_selection(state);
    if ids.len() < 2 {
        return Err(EditorError::InvalidArguments(
            "group requires at least two layers".to_owned(),
        ));
    }
    let before: HashSet<NodeId> = state.document.active_page().nodes.keys().copied().collect();
    let transaction = dp_model::ops::group_nodes(&state.document, &ids, "Group")?;
    state.apply(transaction)?;
    state.selection = new_nodes(state, &before);
    Ok(true)
}

/// Ungroups the selected group layers.
pub fn ungroup_selection(state: &mut EditorState) -> Result<bool, EditorError> {
    let groups: Vec<NodeId> = state
        .selection
        .iter()
        .copied()
        .filter(|id| {
            state
                .document
                .node(*id)
                .map(|node| node.kind == dp_model::NodeKind::Group)
                .unwrap_or(false)
        })
        .collect();
    if groups.is_empty() {
        return Err(EditorError::InvalidArguments(
            "select a group layer to ungroup".to_owned(),
        ));
    }
    for group in groups {
        let transaction = dp_model::ops::ungroup(&state.document, group, "Ungroup")?;
        state.apply(transaction)?;
    }
    Ok(true)
}

/// Renames one node.
pub fn rename_node(state: &mut EditorState, id: NodeId, name: &str) -> Result<bool, EditorError> {
    let patch = NodePatch {
        name: Some(name.to_owned()),
        ..NodePatch::default()
    };
    let transaction = dp_model::ops::patch_nodes(&state.document, &[id], patch, "Rename", None)?;
    state.apply(transaction)?;
    Ok(true)
}

/// Sets opacity on the given nodes.
pub fn set_opacity(
    state: &mut EditorState,
    ids: &[NodeId],
    opacity: f64,
) -> Result<bool, EditorError> {
    let patch = NodePatch {
        opacity: Some(opacity),
        ..NodePatch::default()
    };
    let transaction = dp_model::ops::patch_nodes(&state.document, ids, patch, "Set opacity", None)?;
    state.apply(transaction)?;
    Ok(true)
}

fn toggle_flag(
    state: &mut EditorState,
    ids: &[NodeId],
    flip: impl Fn(&Node) -> bool,
    mut set: impl FnMut(&mut NodePatch, bool),
    label: &str,
) -> Result<bool, EditorError> {
    let mut changed = false;
    for id in ids {
        let Some(node) = state.document.node(*id).cloned() else {
            continue;
        };
        let mut patch = NodePatch::default();
        set(&mut patch, flip(&node));
        let transaction = dp_model::ops::patch_nodes(&state.document, &[*id], patch, label, None)?;
        state.apply(transaction)?;
        changed = true;
    }
    Ok(changed)
}

enum Reorder {
    Front,
    Forward,
    Backward,
    Back,
}

fn reorder_selection(state: &mut EditorState, direction: Reorder) -> Result<bool, EditorError> {
    let ids = top_level_selection(state);
    if ids.is_empty() {
        return Ok(false);
    }
    for id in ids {
        let Some(node) = state.document.node(id).cloned() else {
            continue;
        };
        let parent = node.parent;
        let siblings: Vec<NodeId> = match parent {
            Some(parent_id) => state
                .document
                .node(parent_id)
                .map(|parent_node| parent_node.children.clone())
                .unwrap_or_default(),
            None => state.document.active_page().roots.clone(),
        };
        let Some(current) = siblings.iter().position(|candidate| *candidate == id) else {
            continue;
        };
        let last = siblings.len().saturating_sub(1);
        let index = match direction {
            Reorder::Front => last,
            Reorder::Forward => (current + 1).min(last),
            Reorder::Backward => current.saturating_sub(1),
            Reorder::Back => 0,
        };
        if index == current {
            continue;
        }
        let transaction =
            dp_model::ops::reorder(&state.document, id, parent, Some(index), "Reorder")?;
        state.apply(transaction)?;
    }
    Ok(true)
}

fn move_node(
    state: &mut EditorState,
    id: NodeId,
    parent: Option<NodeId>,
) -> Result<bool, EditorError> {
    if let Some(parent_id) = parent {
        let is_container = state
            .document
            .node(parent_id)
            .map(|node| node.kind.is_container())
            .unwrap_or(false);
        if !is_container {
            return Err(EditorError::InvalidArguments(
                "target parent is not a container".to_owned(),
            ));
        }
    }
    let transaction = dp_model::ops::reorder(&state.document, id, parent, None, "Move layer")?;
    state.apply(transaction)?;
    Ok(true)
}

fn new_nodes(state: &EditorState, before: &HashSet<NodeId>) -> Vec<NodeId> {
    let page = state.document.active_page();
    page.nodes
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect()
}

fn top_level_selection(state: &EditorState) -> Vec<NodeId> {
    let page = state.document.active_page();
    let selected: HashSet<NodeId> = state.selection.iter().copied().collect();
    state
        .selection
        .iter()
        .copied()
        .filter(|id| {
            if !page.nodes.contains_key(id) {
                return false;
            }
            let mut current = page.get(*id).and_then(|node| node.parent);
            while let Some(parent) = current {
                if selected.contains(&parent) {
                    return false;
                }
                current = page.get(parent).and_then(|node| node.parent);
            }
            true
        })
        .collect()
}

fn clone_subtree_into(
    page: &Page,
    node: &Node,
    parent: Option<NodeId>,
    map: &mut HashMap<NodeId, NodeId>,
    inserts: &mut Vec<(Node, Option<NodeId>, Option<usize>)>,
) {
    let mut copy = node.clone();
    let old_id = copy.id;
    copy.id = NodeId::new_v4();
    copy.parent = parent;
    copy.children.clear();
    map.insert(old_id, copy.id);
    let child_ids: Vec<NodeId> = node
        .children
        .iter()
        .copied()
        .filter(|child_id| page.nodes.contains_key(child_id))
        .collect();
    inserts.push((copy.clone(), parent, None));
    for child_id in child_ids {
        if let Some(child) = page.get(child_id) {
            clone_subtree_into(page, child, Some(copy.id), map, inserts);
        }
    }
}

fn collect_page_ids(page: &Page, id: NodeId, out: &mut Vec<NodeId>) {
    let Some(node) = page.get(id) else {
        return;
    };
    out.push(id);
    for child in node.children.clone() {
        collect_page_ids(page, child, out);
    }
}

fn resolve_ids(state: &EditorState, args: &Value) -> Result<Vec<NodeId>, EditorError> {
    match pid_list(args, "ids")? {
        Some(ids) if !ids.is_empty() => Ok(ids),
        _ => {
            if state.selection.is_empty() {
                Err(EditorError::InvalidArguments(
                    "command requires a selection or explicit ids".to_owned(),
                ))
            } else {
                Ok(state.selection.clone())
            }
        }
    }
}

fn pid_list(args: &Value, key: &str) -> Result<Option<Vec<NodeId>>, EditorError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| EditorError::InvalidArguments(error.to_string())),
    }
}

fn parse_page_id(args: &Value) -> Result<PageId, EditorError> {
    let value = args
        .get("page_id")
        .cloned()
        .ok_or_else(|| EditorError::InvalidArguments("page_id is required".to_owned()))?;
    serde_json::from_value(value).map_err(|error| EditorError::InvalidArguments(error.to_string()))
}

fn parse_caret_selection(args: &Value) -> Result<Option<(usize, usize)>, EditorError> {
    let Some(value) = args.get("selection").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let pair: [usize; 2] = serde_json::from_value(value.clone())
        .map_err(|error| EditorError::InvalidArguments(error.to_string()))?;
    Ok(Some((pair[0].min(pair[1]), pair[0].max(pair[1]))))
}

fn json_field<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T, EditorError> {
    let value = args
        .get(key)
        .cloned()
        .ok_or_else(|| EditorError::InvalidArguments(format!("{key} is required")))?;
    serde_json::from_value(value)
        .map_err(|error| EditorError::InvalidArguments(format!("{key}: {error}")))
}

fn optional_id(args: &Value, key: &str) -> Result<Option<NodeId>, EditorError> {
    match args.get(key).filter(|value| !value.is_null()) {
        Some(value) => serde_json::from_value(value.clone())
            .map(Some)
            .map_err(|error| EditorError::InvalidArguments(format!("{key}: {error}"))),
        None => Ok(None),
    }
}

fn parse_variable_value(args: &Value) -> Result<VariableValue, EditorError> {
    let raw = args
        .get("value")
        .cloned()
        .ok_or_else(|| EditorError::InvalidArguments("value is required".to_owned()))?;
    if let Some(number) = raw.as_f64() {
        return Ok(VariableValue::Float { value: number });
    }
    if let Some(text) = raw.as_str() {
        if let Some(color) = Color::from_hex(text) {
            return Ok(VariableValue::Color { color });
        }
        return Ok(VariableValue::String {
            value: text.to_owned(),
        });
    }
    let kind = raw.get("kind").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "color" => {
            let hex = raw.get("hex").and_then(Value::as_str).ok_or_else(|| {
                EditorError::InvalidArguments("color needs a hex string".to_owned())
            })?;
            let color = Color::from_hex(hex)
                .ok_or_else(|| EditorError::InvalidArguments(format!("invalid colour {hex}")))?;
            Ok(VariableValue::Color { color })
        }
        "float" => Ok(VariableValue::Float {
            value: raw.get("value").and_then(Value::as_f64).ok_or_else(|| {
                EditorError::InvalidArguments("float needs a numeric value".to_owned())
            })?,
        }),
        "string" => Ok(VariableValue::String {
            value: raw
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        }),
        "boolean" => Ok(VariableValue::Boolean {
            value: raw.get("value").and_then(Value::as_bool).unwrap_or(false),
        }),
        other => Err(EditorError::InvalidArguments(format!(
            "unsupported variable value kind {other}"
        ))),
    }
}

fn number(args: &Value, key: &str) -> Result<f64, EditorError> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| EditorError::InvalidArguments(format!("{key} must be a number")))
}

fn text_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, EditorError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| EditorError::InvalidArguments(format!("{key} must be a string")))
}

fn parse_align(value: &str) -> Result<Align, EditorError> {
    Ok(match value {
        "left" => Align::Left,
        "hcenter" => Align::HCenter,
        "right" => Align::Right,
        "top" => Align::Top,
        "vcenter" => Align::VCenter,
        "bottom" => Align::Bottom,
        other => {
            return Err(EditorError::InvalidArguments(format!(
                "unknown align value {other}"
            )));
        }
    })
}

/// Converts an absolute world space placement into a node transform helper.
#[allow(dead_code)]
pub(crate) fn world_rect_transform(world: dp_geom::Rect) -> Transform {
    Transform::new(world.x, world.y, world.w, world.h)
}

/// Ensures unused-import warnings do not hide real problems in debug builds.
#[allow(dead_code)]
fn _type_check(_: ModelError) {}
