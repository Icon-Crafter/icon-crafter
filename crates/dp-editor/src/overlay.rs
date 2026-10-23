//! Overlay description returned with every rendered frame.
//!
//! The overlay is expressed in CSS pixels (the UI's coordinate space) and is
//! drawn by the React shell on top of the RGBA frame. Keeping selection,
//! handles, marquee, snap lines and text caret in Rust means hit testing and
//! snapping stay consistent with the rendered scene.

use dp_geom::{Rect, SnapLine};
use dp_model::NodeId;
use serde::{Deserialize, Serialize};

use crate::EditorState;

/// A rectangle in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct CssRect {
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

impl CssRect {
    /// Converts a world rectangle through a viewport (CSS pixel space).
    pub fn from_world(viewport: &dp_render::Viewport, rect: Rect) -> CssRect {
        let zoom = viewport.zoom;
        CssRect {
            x: (rect.x - viewport.x) * zoom,
            y: (rect.y - viewport.y) * zoom,
            w: rect.w * zoom,
            h: rect.h * zoom,
        }
    }
}

/// One selected node outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionOverlay {
    /// Node id.
    pub id: NodeId,
    /// Screen-space outline.
    pub outline: CssRect,
    /// Layer name shown in tooltips.
    pub name: String,
}

/// One resize handle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandleOverlay {
    /// Handle kind: `nw`, `n`, `ne`, `e`, `se`, `s`, `sw`, `w`.
    pub kind: String,
    /// Screen-space square.
    pub rect: CssRect,
    /// CSS cursor name.
    pub cursor: String,
}

/// A snap line to draw while dragging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapLineOverlay {
    /// `x` for a vertical line, `y` for a horizontal line.
    pub axis: String,
    /// CSS start point.
    pub x1: f64,
    /// CSS start point.
    pub y1: f64,
    /// CSS end point.
    pub x2: f64,
    /// CSS end point.
    pub y2: f64,
    /// Snap kind for styling: `grid`, `guide`, `object_edge`, `object_center`.
    pub kind: String,
}

/// Text editing caret/selection overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEditOverlay {
    /// Node being edited.
    pub node_id: NodeId,
    /// Screen-space text box.
    pub rect: CssRect,
    /// Current characters.
    pub characters: String,
    /// Caret position (character index).
    pub caret: usize,
    /// Selected character range.
    pub selection: Option<(usize, usize)>,
}

/// Viewport snapshot, so the UI can draw rulers without a second round trip.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ViewportOverlay {
    /// World x at the left edge.
    pub x: f64,
    /// World y at the top edge.
    pub y: f64,
    /// Zoom factor.
    pub zoom: f64,
    /// CSS width.
    pub width: f64,
    /// CSS height.
    pub height: f64,
    /// Device pixel ratio.
    pub dpr: f64,
}

/// Everything the UI should draw on top of the raster frame.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Overlay {
    /// Viewport transform used for this frame.
    pub viewport: ViewportOverlay,
    /// Selected node outlines.
    pub selection: Vec<SelectionOverlay>,
    /// Union bounds of the selection, when non-empty.
    pub selection_bounds: Option<CssRect>,
    /// Resize handles for the selection bounds.
    pub handles: Vec<HandleOverlay>,
    /// Hovered node outline.
    pub hover: Option<CssRect>,
    /// Marquee rectangle while dragging on empty canvas.
    pub marquee: Option<CssRect>,
    /// Snap guides produced by the current drag.
    pub snap_lines: Vec<SnapLineOverlay>,
    /// Active text editing session.
    pub text_edit: Option<TextEditOverlay>,
    /// Active tool hint for the status bar.
    pub tool_hint: String,
    /// Editor status message.
    pub status: String,
    /// Errors accumulated since the previous frame.
    pub errors: Vec<String>,
    /// Whether undo is available.
    pub can_undo: bool,
    /// Whether redo is available.
    pub can_redo: bool,
    /// Whether the document has unsaved changes.
    pub dirty: bool,
    /// Frame counter matching the rendered frame.
    pub frame_id: u64,
}

impl EditorState {
    /// Builds the overlay for the current state (also drains queued errors).
    pub(crate) fn build_overlay(&mut self) -> Overlay {
        let errors: Vec<String> = std::mem::take(&mut self.errors);
        let viewport = self.viewport;
        let css = |rect: Rect| CssRect::from_world(&viewport, rect);

        let ids = self.selection.clone();
        let mut selection = Vec::new();
        let mut missing = Vec::new();
        for id in &ids {
            match self.node_bounds(*id) {
                Some(bounds) => {
                    let name = self
                        .document
                        .node(*id)
                        .map(|node| node.name.clone())
                        .unwrap_or_default();
                    selection.push(SelectionOverlay {
                        id: *id,
                        outline: css(bounds),
                        name,
                    });
                }
                None => missing.push(*id),
            }
        }
        if !missing.is_empty() {
            self.selection.retain(|id| !missing.contains(id));
        }

        let selection_bounds = self.selection_bounds().map(css);
        let mut handles = Vec::new();
        if let Some(bounds) = selection_bounds {
            let cx = bounds.x + bounds.w * 0.5;
            let cy = bounds.y + bounds.h * 0.5;
            let positions = [
                ("nw", bounds.x, bounds.y, "nwse-resize"),
                ("n", cx, bounds.y, "ns-resize"),
                ("ne", bounds.x + bounds.w, bounds.y, "nesw-resize"),
                ("e", bounds.x + bounds.w, cy, "ew-resize"),
                (
                    "se",
                    bounds.x + bounds.w,
                    bounds.y + bounds.h,
                    "nwse-resize",
                ),
                ("s", cx, bounds.y + bounds.h, "ns-resize"),
                ("sw", bounds.x, bounds.y + bounds.h, "nesw-resize"),
                ("w", bounds.x, cy, "ew-resize"),
            ];
            for (kind, x, y, cursor) in positions {
                handles.push(HandleOverlay {
                    kind: kind.to_owned(),
                    rect: CssRect {
                        x: x - 4.0,
                        y: y - 4.0,
                        w: 8.0,
                        h: 8.0,
                    },
                    cursor: cursor.to_owned(),
                });
            }
        }

        let hover_id = self.hover;
        let hover = hover_id.and_then(|id| self.node_bounds(id)).map(css);
        let marquee = self.marquee.map(css);
        let snap_lines = self
            .snap_lines
            .iter()
            .map(|line| snap_line_to_css(&viewport, line))
            .collect();
        let text_edit = if let Some(edit) = self.text_edit.clone() {
            if let Some(bounds) = self.node_bounds(edit.node_id) {
                Some(TextEditOverlay {
                    node_id: edit.node_id,
                    rect: css(bounds),
                    characters: self
                        .document
                        .node(edit.node_id)
                        .and_then(|node| node.text.as_ref())
                        .map(|text| text.characters.clone())
                        .unwrap_or_default(),
                    caret: edit.caret,
                    selection: edit.selection,
                })
            } else {
                None
            }
        } else {
            None
        };

        Overlay {
            viewport: ViewportOverlay {
                x: viewport.x,
                y: viewport.y,
                zoom: viewport.zoom,
                width: viewport.width,
                height: viewport.height,
                dpr: viewport.dpr,
            },
            selection,
            selection_bounds,
            handles,
            hover,
            marquee,
            snap_lines,
            text_edit,
            tool_hint: self.tool.hint().to_owned(),
            status: self.status.clone(),
            errors,
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            dirty: self.dirty,
            frame_id: self.frame_id,
        }
    }
}

fn snap_line_to_css(viewport: &dp_render::Viewport, line: &SnapLine) -> SnapLineOverlay {
    let axis = format!("{:?}", line.axis).to_lowercase();
    let kind = format!("{:?}", line.kind)
        .chars()
        .enumerate()
        .flat_map(|(index, ch)| {
            if index > 0 && ch.is_uppercase() {
                vec!['_', ch.to_lowercase().next().unwrap_or(ch)]
            } else {
                vec![ch.to_lowercase().next().unwrap_or(ch)]
            }
        })
        .collect::<String>();
    let zoom = viewport.zoom;
    // SnapLine holds an axis plus a coordinate interval: for a vertical line
    // (`x` axis) `pos` is the x coordinate and `from`/`to` are y coordinates.
    let vertical = axis == "x";
    let (x1, y1, x2, y2) = if vertical {
        let x = (line.pos - viewport.x) * zoom;
        (
            x,
            (line.from - viewport.y) * zoom,
            x,
            (line.to - viewport.y) * zoom,
        )
    } else {
        let y = (line.pos - viewport.y) * zoom;
        (
            (line.from - viewport.x) * zoom,
            y,
            (line.to - viewport.x) * zoom,
            y,
        )
    };
    SnapLineOverlay {
        axis,
        x1,
        y1,
        x2,
        y2,
        kind,
    }
}
