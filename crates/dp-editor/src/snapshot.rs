//! Contract-shaped snapshots for the IPC layer.
//!
//! `docs/CONTRACT.md` and `apps/ui/src/engine/contract.ts` define the JSON
//! shapes the React shell consumes. The editor owns the source of truth; this
//! module converts internal state into exactly those shapes so the IPC layer
//! stays a thin, mechanical wrapper.
//!
//! Coordinates: everything sent here is CSS pixels for the overlay (the UI
//! draws it over the raster frame) except `SelectionBox.corners`,
//! `TreeNode.frame` and node details, which are world-space as documented in
//! the contract.

use dp_geom::{Point, Rect};
use dp_model::{CollectionId, Color, ModeId, NodeId, PageId, StyleId, TextAlign, TextStyle};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::overlay::{CssRect, HandleOverlay, Overlay, TextEditOverlay};
use crate::{EditorState, ToolId};

/// Document metadata for the title bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDocMeta {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub schema: u32,
    pub revision: u64,
}

/// Viewport in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SnapshotViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
}

/// Selected node rotated corners in world space (tl, tr, br, bl).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSelectionBox {
    pub id: NodeId,
    pub corners: [Point; 4],
}

/// Selection handle in world space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotHandle {
    pub kind: String,
    pub pos: Point,
    pub cursor: String,
}

/// Snap line in world space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSnapLine {
    pub axis: String,
    pub pos: f64,
    pub from: f64,
    pub to: f64,
}

/// Ruler guide in world space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotGuide {
    pub axis: String,
    pub pos: f64,
}

/// Remote collaborator cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotPeerCursor {
    pub actor: String,
    pub name: String,
    pub color: String,
    pub world: Point,
}

/// Font description used by the DOM textarea overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotFont {
    pub family: String,
    pub weight: u16,
    pub italic: bool,
    pub size: f64,
    pub line_height: f64,
    pub letter_spacing: f64,
    pub align: String,
}

/// Active text editing session (DOM textarea over the canvas).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotTextEdit {
    pub node_id: NodeId,
    pub screen_rect: Rect,
    pub characters: String,
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
    pub font: SnapshotFont,
    pub color: Color,
}

/// Caret/selection overlay for the DOM textarea (screen space).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotOverlayTextEdit {
    pub caret: Point,
    pub selection: [usize; 2],
}

/// Everything the UI draws on top of the frame.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SnapshotOverlay {
    pub selection: Vec<SnapshotSelectionBox>,
    pub hover: Option<SnapshotSelectionBox>,
    pub handles: Vec<SnapshotHandle>,
    pub marquee: Option<Rect>,
    pub snap_lines: Vec<SnapshotSnapLine>,
    pub guides: Vec<SnapshotGuide>,
    pub cursors: Vec<SnapshotPeerCursor>,
    pub tool_hint: Option<String>,
    pub text_edit: Option<SnapshotOverlayTextEdit>,
}

/// Styles list entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotStyleEntry {
    pub id: StyleId,
    pub name: String,
}

/// Shared styles grouped by kind.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SnapshotStyles {
    pub paints: Vec<SnapshotStyleEntry>,
    pub texts: Vec<SnapshotStyleEntry>,
    pub effects: Vec<SnapshotStyleEntry>,
}

/// Component / component set entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotComponent {
    pub node_id: NodeId,
    pub name: String,
    pub kind: String,
}

/// Variable mode entry (mode names are not stored in schema v1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMode {
    pub id: ModeId,
    pub name: String,
}

/// Variable collection entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotVariableCollection {
    pub id: CollectionId,
    pub name: String,
    pub modes: Vec<SnapshotMode>,
}

/// One row of the layers panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotTreeNode {
    pub id: NodeId,
    pub name: String,
    pub kind: dp_model::NodeKind,
    pub depth: u32,
    pub parent: Option<NodeId>,
    pub visible: bool,
    pub locked: bool,
    pub child_count: usize,
    pub is_component: bool,
    pub is_instance: bool,
    /// World-space AABB.
    pub frame: Rect,
}

/// Page entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotPage {
    pub id: PageId,
    pub name: String,
}

/// Document tree summary for the panels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub pages: Vec<SnapshotPage>,
    pub active_page: PageId,
    pub nodes: Vec<SnapshotTreeNode>,
    pub styles: SnapshotStyles,
    pub components: Vec<SnapshotComponent>,
    pub variables: Vec<SnapshotVariableCollection>,
}

/// Full inspector detail for one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotDetail {
    pub id: NodeId,
    pub name: String,
    pub kind: dp_model::NodeKind,
    pub parent: Option<NodeId>,
    pub transform: dp_model::Transform,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f64,
    pub blend: String,
    pub fills: Vec<dp_model::Fill>,
    pub strokes: Vec<dp_model::Stroke>,
    pub effects: Vec<dp_model::Effect>,
    pub corner_radii: dp_geom::CornerRadii,
    pub fill_rule: dp_model::FillRule,
    pub text: Option<dp_model::TextContent>,
    pub layout: Option<dp_model::AutoLayout>,
    pub constraints: Option<dp_model::Constraints>,
    pub component: Option<dp_model::ComponentRole>,
    pub is_inside_instance: bool,
    /// World-space AABB.
    pub absolute_bounds: Rect,
    /// This build does not generate Dev Mode styles yet.
    pub resolved: Option<Value>,
}

/// Engine error in the contract's shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotError {
    pub code: String,
    pub message: String,
    pub severity: String,
}

/// The complete engine state returned by `engine_state` and every mutating
/// command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub doc: Option<SnapshotDocMeta>,
    pub revision: u64,
    pub frame_id: u64,
    pub viewport: SnapshotViewport,
    pub tool: ToolId,
    pub selection: Vec<NodeId>,
    pub hover: Option<NodeId>,
    pub overlay: SnapshotOverlay,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub summary: Option<SnapshotSummary>,
    pub selection_details: Vec<SnapshotDetail>,
    pub text_edit: Option<SnapshotTextEdit>,
    pub errors: Vec<SnapshotError>,
    pub status: Option<String>,
}

impl EditorState {
    /// Builds the contract-shaped snapshot of the current state.
    pub fn snapshot(&mut self) -> Snapshot {
        self.invalidate_layout();
        self.layout();
        let selection = self.selection.clone();
        let hover = self.hover;
        let overlay = self.build_overlay();
        let overlay_text_edit = overlay.text_edit.as_ref().map(caret_overlay);
        let summary = self.build_summary_snapshot();
        let selection_details = self.details_snapshot(&selection);
        let text_edit = self.text_edit_snapshot(&overlay);
        let errors = overlay
            .errors
            .iter()
            .map(|message| SnapshotError {
                code: "internal".to_owned(),
                message: message.clone(),
                severity: "error".to_owned(),
            })
            .collect();
        let viewport = self.viewport;
        Snapshot {
            doc: Some(SnapshotDocMeta {
                id: self.document.id.to_string(),
                name: self.document.name.clone(),
                path: self.path.as_ref().map(|path| path.display().to_string()),
                schema: self.document.schema,
                revision: self.document.revision,
            }),
            revision: self.document.revision,
            frame_id: self.frame_id,
            viewport: SnapshotViewport {
                x: viewport.x,
                y: viewport.y,
                zoom: viewport.zoom,
                width: viewport.width,
                height: viewport.height,
                dpr: viewport.dpr,
            },
            tool: self.tool,
            selection,
            hover: self.hover,
            overlay: overlay_snapshot(overlay, hover, overlay_text_edit),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            dirty: self.dirty,
            summary: Some(summary),
            selection_details,
            text_edit,
            errors,
            status: if self.status.is_empty() {
                None
            } else {
                Some(self.status.clone())
            },
        }
    }

    /// Snapshot returned when no document is open.
    pub fn empty_snapshot() -> Snapshot {
        Snapshot {
            doc: None,
            revision: 0,
            frame_id: 0,
            viewport: SnapshotViewport {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
                width: 0.0,
                height: 0.0,
                dpr: 1.0,
            },
            tool: ToolId::Move,
            selection: Vec::new(),
            hover: None,
            overlay: SnapshotOverlay::default(),
            can_undo: false,
            can_redo: false,
            dirty: false,
            summary: None,
            selection_details: Vec::new(),
            text_edit: None,
            errors: Vec::new(),
            status: None,
        }
    }

    /// Contract-shaped details for specific nodes.
    pub fn details_snapshot(&mut self, ids: &[NodeId]) -> Vec<SnapshotDetail> {
        self.layout();
        ids.iter().filter_map(|id| self.detail_for(*id)).collect()
    }

    fn detail_for(&mut self, id: NodeId) -> Option<SnapshotDetail> {
        let page = self.document.active_page();
        let node = page.get(id)?.clone();
        let world = self.world_bounds(id);
        let inside_instance = is_inside_instance(page, &node);
        Some(SnapshotDetail {
            id,
            name: node.name.clone(),
            kind: node.kind,
            parent: node.parent,
            transform: node.transform,
            visible: node.visible,
            locked: node.locked,
            opacity: node.opacity,
            blend: format!("{:?}", node.blend).to_lowercase().replace('_', "-"),
            fills: node.style.fills.clone(),
            strokes: node.style.strokes.clone(),
            effects: node.style.effects.clone(),
            corner_radii: node.style.corner_radii,
            fill_rule: node.style.fill_rule,
            text: node.text.clone(),
            layout: node.layout.clone(),
            constraints: node.constraints,
            component: node.component.clone(),
            is_inside_instance: inside_instance,
            absolute_bounds: world,
            resolved: None,
        })
    }

    fn build_summary_snapshot(&mut self) -> SnapshotSummary {
        self.layout();
        let page = self.document.active_page();
        let mut nodes = Vec::new();
        for root in page.roots.clone() {
            collect_snapshot_tree(page, self, root, 0, &mut nodes);
        }
        let components =
            self.document
                .components
                .components
                .values()
                .map(|component| SnapshotComponent {
                    node_id: component.node_id,
                    name: component.name.clone(),
                    kind: "component".to_owned(),
                })
                .chain(self.document.components.component_sets.values().map(|set| {
                    SnapshotComponent {
                        node_id: set.node_id,
                        name: set.name.clone(),
                        kind: "component_set".to_owned(),
                    }
                }))
                .collect();
        let styles = &self.document.styles;
        SnapshotSummary {
            pages: self
                .document
                .pages
                .iter()
                .map(|page| SnapshotPage {
                    id: page.id,
                    name: page.name.clone(),
                })
                .collect(),
            active_page: page.id,
            nodes,
            styles: SnapshotStyles {
                paints: styles
                    .paints
                    .values()
                    .map(|style| SnapshotStyleEntry {
                        id: style.id,
                        name: style.name.clone(),
                    })
                    .collect(),
                texts: styles
                    .texts
                    .values()
                    .map(|style| SnapshotStyleEntry {
                        id: style.id,
                        name: style.name.clone(),
                    })
                    .collect(),
                effects: styles
                    .effects
                    .values()
                    .map(|style| SnapshotStyleEntry {
                        id: style.id,
                        name: style.name.clone(),
                    })
                    .collect(),
            },
            components,
            variables: self
                .document
                .variables
                .collections
                .iter()
                .map(|collection| SnapshotVariableCollection {
                    id: collection.id,
                    name: collection.name.clone(),
                    modes: collection
                        .modes
                        .iter()
                        .enumerate()
                        .map(|(index, mode)| SnapshotMode {
                            id: *mode,
                            // Schema v1 stores mode ids, not names.
                            name: format!("Mode {}", index + 1),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn text_edit_snapshot(&mut self, overlay: &Overlay) -> Option<SnapshotTextEdit> {
        let edit = overlay.text_edit.as_ref()?;
        let node = self.document.active_page().get(edit.node_id)?.clone();
        let text = node.text.as_ref()?;
        Some(text_edit_from_overlay(
            edit,
            text.style.clone(),
            node.opacity,
        ))
    }

    fn world_bounds(&self, id: NodeId) -> Rect {
        if let Some(layout) = self.layout_cache.as_ref()
            && let Some(layout) = layout.get(id)
        {
            return layout.world;
        }
        self.document
            .active_page()
            .get(id)
            .map(|node| node.transform.bounds())
            .unwrap_or_default()
    }
}

fn overlay_snapshot(
    overlay: Overlay,
    hover_id: Option<NodeId>,
    text_edit: Option<SnapshotOverlayTextEdit>,
) -> SnapshotOverlay {
    let viewport = overlay.viewport;
    let zoom = if viewport.zoom.abs() < 1e-9 {
        1.0
    } else {
        viewport.zoom
    };
    let world_rect = |css: CssRect| -> Rect {
        Rect::new(
            viewport.x + css.x / zoom,
            viewport.y + css.y / zoom,
            css.w / zoom,
            css.h / zoom,
        )
    };
    let world_corners = |css: CssRect| {
        let rect = world_rect(css);
        [
            Point::new(rect.x, rect.y),
            Point::new(rect.right(), rect.y),
            Point::new(rect.right(), rect.bottom()),
            Point::new(rect.x, rect.bottom()),
        ]
    };
    let selection = overlay
        .selection
        .iter()
        .map(|item| SnapshotSelectionBox {
            id: item.id,
            corners: world_corners(item.outline),
        })
        .collect();
    let hover = overlay.hover.map(|css| SnapshotSelectionBox {
        id: hover_id.unwrap_or_else(|| NodeId::from_u128(0)),
        corners: world_corners(css),
    });
    let handles = overlay
        .handles
        .iter()
        .map(|HandleOverlay { kind, rect, cursor }| {
            let world = world_rect(*rect);
            SnapshotHandle {
                kind: handle_kind(kind),
                pos: Point::new(world.x + world.w * 0.5, world.y + world.h * 0.5),
                cursor: cursor.clone(),
            }
        })
        .collect();
    let snap_lines = overlay
        .snap_lines
        .iter()
        .map(|line| {
            if line.axis == "x" {
                SnapshotSnapLine {
                    axis: "x".to_owned(),
                    pos: viewport.x + line.x1 / zoom,
                    from: viewport.y + line.y1.min(line.y2) / zoom,
                    to: viewport.y + line.y1.max(line.y2) / zoom,
                }
            } else {
                SnapshotSnapLine {
                    axis: "y".to_owned(),
                    pos: viewport.y + line.y1 / zoom,
                    from: viewport.x + line.x1.min(line.x2) / zoom,
                    to: viewport.x + line.x1.max(line.x2) / zoom,
                }
            }
        })
        .collect();
    SnapshotOverlay {
        selection,
        hover,
        handles,
        marquee: overlay.marquee.map(world_rect),
        snap_lines,
        // Guides and remote cursors arrive with the collaboration milestone.
        guides: Vec::new(),
        cursors: Vec::new(),
        tool_hint: if overlay.tool_hint.is_empty() {
            None
        } else {
            Some(overlay.tool_hint)
        },
        text_edit,
    }
}

/// Approximate caret position for the overlay caret (v1: no glyph metrics).
fn caret_overlay(edit: &TextEditOverlay) -> SnapshotOverlayTextEdit {
    let line_count = edit.characters.split('\n').count().max(1) as f64;
    let line_height = (edit.rect.h / line_count).max(1.0);
    let char_width = line_height * 0.5;
    let mut line = 0usize;
    let mut column = 0usize;
    for (index, ch) in edit.characters.chars().enumerate() {
        if index >= edit.caret {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    let selection = edit
        .selection
        .map(|(start, end)| [start.min(end), start.max(end)])
        .unwrap_or([edit.caret, edit.caret]);
    SnapshotOverlayTextEdit {
        caret: Point::new(
            edit.rect.x + column as f64 * char_width,
            edit.rect.y + (line as f64 + 1.0) * line_height,
        ),
        selection,
    }
}

fn handle_kind(kind: &str) -> String {
    match kind {
        "nw" | "ne" | "se" | "sw" => "corner".to_owned(),
        "n" | "e" | "s" | "w" => "edge".to_owned(),
        "rotate" => "rotate".to_owned(),
        other => other.to_owned(),
    }
}

fn text_edit_from_overlay(
    edit: &TextEditOverlay,
    style: TextStyle,
    opacity: f64,
) -> SnapshotTextEdit {
    let line_height = style
        .line_height
        .resolve(style.font_size)
        .unwrap_or(style.font_size * 1.2);
    let align = match style_align(&style) {
        TextAlign::Left => "left",
        TextAlign::Center => "center",
        TextAlign::Right => "right",
        TextAlign::Justified => "justified",
    };
    let color = style
        .fills
        .iter()
        .find(|fill| fill.visible)
        .map(|fill| match &fill.paint {
            dp_model::Paint::Solid { color } => color.with_alpha(color.a * fill.opacity * opacity),
            dp_model::Paint::Linear { stops, .. } | dp_model::Paint::Radial { stops, .. } => {
                stops.first().map(|stop| stop.color).unwrap_or(Color::BLACK)
            }
            _ => Color::BLACK,
        })
        .unwrap_or(Color::WHITE);
    SnapshotTextEdit {
        node_id: edit.node_id,
        screen_rect: Rect::new(edit.rect.x, edit.rect.y, edit.rect.w, edit.rect.h),
        characters: edit.characters.clone(),
        caret: edit.caret,
        selection: edit.selection,
        font: SnapshotFont {
            family: style.font_family.clone(),
            weight: style.font_weight,
            italic: style.italic,
            size: style.font_size,
            line_height,
            letter_spacing: style.letter_spacing,
            align: align.to_owned(),
        },
        color,
    }
}

fn style_align(style: &TextStyle) -> TextAlign {
    // Paragraph alignment lives on TextContent, not TextStyle; the overlay
    // caller replaces this with the paragraph value when available.
    let _ = style;
    TextAlign::Left
}

fn collect_snapshot_tree(
    page: &dp_model::Page,
    state: &EditorState,
    id: NodeId,
    depth: u32,
    out: &mut Vec<SnapshotTreeNode>,
) {
    let Some(node) = page.get(id) else {
        return;
    };
    let bounds = state
        .layout_cache
        .as_ref()
        .and_then(|layout| layout.get(id))
        .map(|layout| layout.world)
        .unwrap_or_else(|| node.transform.bounds());
    out.push(SnapshotTreeNode {
        id,
        name: node.name.clone(),
        kind: node.kind,
        depth,
        parent: node.parent,
        visible: node.visible,
        locked: node.locked,
        child_count: node.children.len(),
        is_component: matches!(
            node.kind,
            dp_model::NodeKind::Component | dp_model::NodeKind::ComponentSet
        ),
        is_instance: node.kind == dp_model::NodeKind::Instance,
        frame: bounds,
    });
    for child in node.children.clone() {
        collect_snapshot_tree(page, state, child, depth + 1, out);
    }
}

fn is_inside_instance(page: &dp_model::Page, node: &dp_model::Node) -> bool {
    let mut current = node.parent;
    while let Some(parent_id) = current {
        let Some(parent) = page.get(parent_id) else {
            return false;
        };
        if parent.kind == dp_model::NodeKind::Instance {
            return true;
        }
        current = parent.parent;
    }
    false
}
