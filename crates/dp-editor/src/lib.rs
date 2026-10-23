//! Editor state machine for Icon Crafter.
//!
//! `dp-editor` is transport independent: the Tauri IPC layer, the browser dev
//! harness and tests all drive the same [`EditorState`] through pointer, wheel,
//! keyboard and command inputs. It owns:
//!
//! * the open document plus its undo [`History`];
//! * the viewport (pan/zoom/device pixel ratio);
//! * the active tool and selection;
//! * hit testing and drag interactions (move, marquee, shape creation);
//! * text editing sessions;
//! * snapping and the overlay description returned to the UI;
//! * rendering through `dp-render`, saving through `dp-persist` and export
//!   through `dp-io`.
//!
//! The UI layer only draws pixels and overlays and sends user intent; it never
//! mutates the document itself.

pub mod commands;
pub mod input;
pub mod overlay;
pub mod playback;
pub mod snapshot;

use std::path::{Path, PathBuf};

use dp_geom::{Mat2D, Point, Rect, SnapLine};
use dp_io::ExportOptions;
use dp_layout::{LayoutEngine, LayoutMap};
use dp_model::{
    Color, DocKind, Document, Edit, History, Node, NodeId, NodeKind, Page, PageId, Transaction,
    Transform,
};
use dp_persist::PersistError;
use dp_render::{RenderTarget, SceneBuilder, Viewport};
use dp_text::TextEngine;
use serde::{Deserialize, Serialize};

pub use commands::CommandOutcome;
pub use overlay::{
    CssRect, HandleOverlay, Overlay, SelectionOverlay, SnapLineOverlay, TextEditOverlay,
};
pub use snapshot::{
    Snapshot, SnapshotDetail, SnapshotDocMeta, SnapshotError, SnapshotOverlay, SnapshotSummary,
    SnapshotTreeNode, SnapshotViewport,
};

/// Version of the editor API surface.
pub const EDITOR_VERSION: &str = "0.1.0";

/// Default interaction threshold in CSS pixels.
pub const CLICK_TOLERANCE_CSS: f64 = 3.0;
/// Nudge distance in world units for a single arrow-key press.
pub const NUDGE_STEP: f64 = 1.0;
/// Nudge distance with Shift held.
pub const NUDGE_LARGE_STEP: f64 = 10.0;

/// Errors produced by editor operations.
#[derive(Debug, thiserror::Error)]
pub enum EditorError {
    /// Document model error.
    #[error(transparent)]
    Model(#[from] dp_model::ModelError),
    /// Page operation error.
    #[error(transparent)]
    Page(#[from] dp_model::PageError),
    /// Persistence error.
    #[error(transparent)]
    Persist(#[from] PersistError),
    /// Import/export error.
    #[error(transparent)]
    Export(#[from] dp_io::ExportError),
    /// The command name is unknown or not supported by this build.
    #[error("unsupported command: {0}")]
    Unsupported(String),
    /// Command arguments are missing or malformed.
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    /// There is no project path to save to.
    #[error("no project path is set; use save as")]
    NoPath,
    /// Rendering failed.
    #[error("render failed: {0}")]
    Render(String),
}

/// Tools available in the editor toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    /// Select and move.
    Move,
    /// Pan the viewport.
    Hand,
    /// Create a frame.
    Frame,
    /// Create an export slice.
    Slice,
    /// Create a rectangle.
    Rectangle,
    /// Create an ellipse.
    Ellipse,
    /// Create a line.
    Line,
    /// Create a polygon.
    Polygon,
    /// Create a star.
    Star,
    /// Pen / bezier editing (creation disabled until vector tools land).
    Pen,
    /// Node editing.
    Node,
    /// Create text.
    Text,
    /// Place an image (import flow is desktop-owned).
    Image,
    /// Comment tool (desktop-owned panel).
    Comment,
    /// Prototype connector.
    Connector,
}

impl ToolId {
    /// The node kind a drag with this tool creates, if any.
    pub fn shape_kind(self) -> Option<NodeKind> {
        match self {
            ToolId::Frame => Some(NodeKind::Frame),
            ToolId::Rectangle => Some(NodeKind::Rectangle),
            ToolId::Ellipse => Some(NodeKind::Ellipse),
            ToolId::Line => Some(NodeKind::Line),
            ToolId::Polygon => Some(NodeKind::Polygon),
            ToolId::Star => Some(NodeKind::Star),
            _ => None,
        }
    }

    /// Status-bar hint for the tool.
    pub fn hint(self) -> &'static str {
        match self {
            ToolId::Move => "Select and move layers · drag to marquee",
            ToolId::Hand => "Drag to pan the canvas",
            ToolId::Frame => "Drag to create a frame",
            ToolId::Slice => "Drag to mark an export slice",
            ToolId::Rectangle => "Drag to draw a rectangle",
            ToolId::Ellipse => "Drag to draw an ellipse",
            ToolId::Line => "Drag to draw a line",
            ToolId::Polygon => "Drag to draw a polygon",
            ToolId::Star => "Drag to draw a star",
            ToolId::Pen => "Pen: vector creation is not available yet",
            ToolId::Node => "Node editing: select a vector path",
            ToolId::Text => "Click to place text",
            ToolId::Image => "Drop an image file to place it",
            ToolId::Comment => "Click to leave a comment",
            ToolId::Connector => "Drag between frames to connect a prototype",
        }
    }
}

/// One rasterized frame plus the overlay the UI should draw on top.
#[derive(Debug, Clone)]
pub struct RenderFrame {
    /// Monotonic frame counter.
    pub frame_id: u64,
    /// Device pixel width of `rgba`.
    pub width: u32,
    /// Device pixel height of `rgba`.
    pub height: u32,
    /// Straight-alpha RGBA8 pixels.
    pub rgba: Vec<u8>,
    /// Overlay primitives in CSS pixels.
    pub overlay: Overlay,
}

/// Page entry of the layer tree summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageSummary {
    /// Page id.
    pub id: PageId,
    /// Page name.
    pub name: String,
}

/// Flattened node entry for the layers panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TreeNode {
    /// Node id.
    pub id: NodeId,
    /// Layer name.
    pub name: String,
    /// Node kind.
    pub kind: NodeKind,
    /// Depth in the tree (roots are 0).
    pub depth: u32,
    /// Parent id.
    pub parent: Option<NodeId>,
    /// Visibility.
    pub visible: bool,
    /// Lock state.
    pub locked: bool,
    /// Number of direct children.
    pub child_count: usize,
    /// Authored width.
    pub width: f64,
    /// Authored height.
    pub height: f64,
}

/// Summary of the document tree for the UI (excludes pixels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocSummary {
    /// Document name.
    pub document_name: String,
    /// Document revision.
    pub revision: u64,
    /// All pages.
    pub pages: Vec<PageSummary>,
    /// Active page id.
    pub active_page: PageId,
    /// Flat pre-order node list of the active page.
    pub nodes: Vec<TreeNode>,
    /// Component ids/names available to insert.
    pub components: Vec<String>,
    /// Shared style names.
    pub styles: Vec<String>,
    /// Variable names.
    pub variables: Vec<String>,
}

/// Absolute placement of a node in one struct.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AbsoluteBounds {
    /// Left coordinate in world space.
    pub x: f64,
    /// Top coordinate in world space.
    pub y: f64,
    /// Width in world space.
    pub width: f64,
    /// Height in world space.
    pub height: f64,
}

/// Full editable properties of one node for the inspector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDetail {
    /// Node id.
    pub id: NodeId,
    /// Layer name.
    pub name: String,
    /// Node kind.
    pub kind: NodeKind,
    /// Visibility.
    pub visible: bool,
    /// Lock state.
    pub locked: bool,
    /// Opacity.
    pub opacity: f64,
    /// Authored transform.
    pub transform: Transform,
    /// Absolute placement.
    pub absolute: AbsoluteBounds,
    /// Style (fills/strokes/effects/corner radii).
    pub style: dp_model::NodeStyle,
    /// Geometry (vector/polygon/star/boolean).
    pub geometry: Option<dp_model::NodeGeometry>,
    /// Text content.
    pub text: Option<dp_model::TextContent>,
    /// Auto layout settings.
    pub layout: Option<dp_model::AutoLayout>,
    /// Resolved text style name if bound (always `None` until style binding lands).
    pub text_style_name: Option<String>,
}

/// The complete editor state.
///
/// The struct is intentionally serialisable only through [`DocSummary`],
/// [`NodeDetail`], [`Overlay`] and [`RenderFrame`]; IPC sends those, not the raw
/// state.
pub struct EditorState {
    /// The open document.
    pub document: Document,
    /// Undo/redo history.
    pub history: History,
    /// Viewport transform and canvas size.
    pub viewport: Viewport,
    /// Active tool.
    pub tool: ToolId,
    /// Current selection in paint order.
    pub selection: Vec<NodeId>,
    /// Currently hovered node.
    pub hover: Option<NodeId>,
    /// Last status message (shown in the status bar).
    pub status: String,
    /// Whether the document has unsaved changes.
    pub dirty: bool,
    /// Project path, when the document came from or was saved to disk.
    pub path: Option<PathBuf>,
    /// Render frame counter.
    pub frame_id: u64,
    /// Layout engine (text centric auto layout).
    pub layout_engine: LayoutEngine,
    /// Text engine shared by layout and rendering.
    pub text_engine: TextEngine,
    /// Cached layout for the active page.
    pub(crate) layout_cache: Option<LayoutMap>,
    /// Active drag gesture.
    pub(crate) drag: Option<input::DragState>,
    /// Marquee rectangle in world space.
    pub(crate) marquee: Option<Rect>,
    /// Snap guides produced by the current drag.
    pub(crate) snap_lines: Vec<SnapLine>,
    /// Active text editing session.
    pub(crate) text_edit: Option<TextEditState>,
    /// Copied nodes (subtree snapshots, ids adjusted on paste).
    pub(crate) clipboard: Vec<ClipboardNode>,
    /// Errors accumulated during the last input batch.
    pub(crate) errors: Vec<String>,
    /// Whether Dev Mode is active.
    pub dev_mode: bool,
    /// Developer handoff state (annotations and readiness).
    pub handoff: dp_devmode::Handoff,
    /// Active prototype presentation session.
    pub playback: Option<playback::PrototypePlayback>,
    /// Pointer-down position while presenting (CSS pixels).
    pub(crate) proto_pointer_down: Option<Point>,
}

/// A node plus its parent reference inside the internal clipboard.
#[derive(Debug, Clone)]
pub(crate) struct ClipboardNode {
    pub node: Node,
    pub parent: Option<NodeId>,
}

/// Active text editing session.
#[derive(Debug, Clone)]
pub(crate) struct TextEditState {
    pub node_id: NodeId,
    pub page: PageId,
    pub original: Node,
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
}

impl EditorState {
    /// Creates an editor around a document and viewport.
    pub fn new(document: Document, viewport: Viewport) -> EditorState {
        let mut state = EditorState {
            document,
            history: History::new(),
            viewport,
            tool: ToolId::Move,
            selection: Vec::new(),
            hover: None,
            status: "Ready".to_owned(),
            dirty: false,
            path: None,
            frame_id: 0,
            layout_engine: LayoutEngine::new(),
            text_engine: TextEngine::new(),
            layout_cache: None,
            dev_mode: false,
            handoff: dp_devmode::Handoff::new(0),
            playback: None,
            proto_pointer_down: None,
            drag: None,
            marquee: None,
            snap_lines: Vec::new(),
            text_edit: None,
            clipboard: Vec::new(),
            errors: Vec::new(),
        };
        state.fit_to_content();
        state
    }

    /// Creates an empty editor document.
    pub fn new_empty(kind: DocKind) -> EditorState {
        let document = Document::new_with_kind("Untitled", kind);
        let viewport = Viewport::new(-400.0, -300.0, 1.0, 1200.0, 800.0, 1.0);
        EditorState::new(document, viewport)
    }

    /// Opens a project from disk.
    pub fn open(path: impl AsRef<Path>) -> Result<EditorState, EditorError> {
        let loaded = dp_persist::open_project(path.as_ref())?;
        let viewport = Viewport::new(0.0, 0.0, 1.0, 1200.0, 800.0, 1.0);
        let mut state = EditorState::new(loaded.document, viewport);
        state.path = Some(path.as_ref().to_path_buf());
        state.dirty = false;
        state.fit_to_content();
        state.status = format!("Opened {}", path.as_ref().display());
        Ok(state)
    }

    /// Active page.
    pub fn page(&self) -> &Page {
        self.document.active_page()
    }

    /// Invalidates the cached layout (call after any document mutation).
    pub fn invalidate_layout(&mut self) {
        self.layout_cache = None;
    }

    /// Recomputes the layout cache when invalid and returns it.
    pub fn layout(&mut self) -> &LayoutMap {
        if self.layout_cache.is_none() {
            let map = self.layout_engine.compute_page(self.document.active_page());
            self.layout_cache = Some(map);
        }
        self.layout_cache.as_ref().expect("layout just computed")
    }

    /// Turns Dev Mode on or off.
    pub fn set_dev_mode(&mut self, enabled: bool) {
        self.dev_mode = enabled;
        self.status = if enabled {
            "Dev Mode: select a layer to inspect".to_owned()
        } else {
            "Design mode".to_owned()
        };
    }

    /// Starts prototype presentation (or stops it when already presenting is
    /// not requested). `frame_id` defaults to the document's start frame.
    pub fn prototype_play(&mut self, frame_id: Option<NodeId>) -> Result<(), EditorError> {
        let runtime = dp_prototype::PrototypeRuntime::start(&self.document, frame_id).map_err(
            |error| EditorError::InvalidArguments(format!("cannot start prototype: {error}")),
        )?;
        let frame = runtime.current_frame();
        if !playback::is_frame(&self.document, frame) {
            return Err(EditorError::InvalidArguments(
                "prototype start frame must be a frame node".to_owned(),
            ));
        }
        self.selection.clear();
        self.hover = None;
        self.text_edit = None;
        self.drag = None;
        self.marquee = None;
        self.snap_lines.clear();
        self.playback = Some(playback::PrototypePlayback::new(frame, frame));
        self.fit_frame(frame);
        let name = self
            .document
            .node(frame)
            .map(|node| node.name.clone())
            .unwrap_or_default();
        self.status = format!("Presenting {name} — click hotspots, Backspace to go back, Esc to exit");
        Ok(())
    }

    /// Stops prototype presentation.
    pub fn prototype_stop(&mut self) {
        if self.playback.take().is_some() {
            self.proto_pointer_down = None;
            self.status = "Prototype stopped".to_owned();
        }
    }

    /// Fires a click interaction at a world-space point (presentation mode).
    pub fn prototype_click_world(&mut self, point: Point) -> Result<bool, EditorError> {
        let Some(playback) = self.playback.clone() else {
            return Ok(false);
        };
        let Some(node_id) = playback::interactive_node_at(&self.document, point) else {
            return Ok(false);
        };
        let mut actions = playback.actions.clone();
        actions.push(playback::PlaybackAction::Click(node_id));
        self.prototype_replay(playback.start_frame, actions)
    }

    /// Navigates back (history or top overlay).
    pub fn prototype_back(&mut self) -> Result<bool, EditorError> {
        let Some(playback) = self.playback.clone() else {
            return Ok(false);
        };
        if !playback.can_back() {
            return Ok(false);
        }
        let mut actions = playback.actions.clone();
        actions.push(playback::PlaybackAction::Back);
        self.prototype_replay(playback.start_frame, actions)
    }

    /// Closes the top overlay when one is open.
    pub fn prototype_close_overlay(&mut self) -> Result<bool, EditorError> {
        let Some(playback) = self.playback.clone() else {
            return Ok(false);
        };
        if playback.overlay_count == 0 {
            return Ok(false);
        }
        let mut actions = playback.actions.clone();
        actions.push(playback::PlaybackAction::CloseOverlay);
        self.prototype_replay(playback.start_frame, actions)
    }

    fn prototype_replay(
        &mut self,
        start_frame: NodeId,
        actions: Vec<playback::PlaybackAction>,
    ) -> Result<bool, EditorError> {
        let runtime = playback::PrototypePlayback::replay(&self.document, start_frame, &actions)
            .map_err(|error| {
                EditorError::InvalidArguments(format!("prototype interaction failed: {error}"))
            })?;
        let frame = runtime.current_frame();
        let next = playback::PrototypePlayback::from_runtime(&runtime, start_frame, actions);
        let previous = self.playback.as_ref().map(|state| state.frame);
        self.playback = Some(next);
        if previous != Some(frame) {
            self.fit_frame(frame);
            let name = self
                .document
                .node(frame)
                .map(|node| node.name.clone())
                .unwrap_or_default();
            self.status = format!("Presenting {name}");
        }
        Ok(true)
    }

    fn fit_frame(&mut self, frame: NodeId) {
        if let Some(bounds) = self.node_bounds(frame) {
            self.viewport.fit_rect(bounds, 64.0);
        }
    }

    /// Computes a fresh layout map without holding a borrow of the cache.
    fn dev_layout_map(&mut self) -> LayoutMap {
        let page = self.document.active_page().clone();
        self.layout_engine.compute_page(&page)
    }

    /// Developer inspection data for the current selection.
    pub fn inspect_selection(&mut self) -> dp_devmode::InspectSnapshot {
        let ids = self.selection.clone();
        let layout = self.dev_layout_map();
        self.handoff.revision = self.document.revision;
        dp_devmode::inspect(&self.document, &layout, &ids)
    }

    /// Deterministic code generation for the current selection.
    pub fn generate_code(
        &mut self,
        target: dp_devmode::CodeTarget,
        precision: u32,
    ) -> Result<dp_devmode::GeneratedCode, EditorError> {
        let ids = self.selection.clone();
        let layout = self.dev_layout_map();
        let options = dp_devmode::CodegenOptions {
            target,
            precision: precision.clamp(0, 4),
            include_comments: true,
            embed_wrapper: true,
        };
        dp_devmode::generate(&self.document, &layout, &ids, &options)
            .map_err(|error| EditorError::InvalidArguments(error.to_string()))
    }

    /// Structural comparison against another revision of the same document.
    pub fn compare_revision(&self, other: &Document) -> dp_devmode::RevisionDiff {
        dp_devmode::compare_documents(&self.document, other)
    }

    /// Serialized document used to capture a revision for later comparison.
    pub fn document_json(&self) -> Result<String, EditorError> {
        self.document.to_json_string().map_err(EditorError::from)
    }

    /// Adds a developer annotation to a node and returns its id.
    pub fn dev_add_annotation(
        &mut self,
        node: NodeId,
        body: String,
        author: String,
        created_at: String,
        kind: dp_devmode::AnnotationKind,
        link: Option<String>,
    ) -> Result<uuid::Uuid, EditorError> {
        let Some(target) = self.document.node(node) else {
            return Err(EditorError::InvalidArguments(format!(
                "unknown node {node}"
            )));
        };
        let name = target.name.clone();
        let id = self
            .handoff
            .add_annotation(node, name, body, author, created_at, kind);
        if let Some(link) = link {
            self.handoff.link_annotation(id, link);
        }
        self.handoff.revision = self.document.revision;
        Ok(id)
    }

    /// Marks the current handoff readiness.
    pub fn dev_mark_ready(
        &mut self,
        status: dp_devmode::HandoffStatus,
        by: String,
        at: String,
    ) {
        self.handoff.revision = self.document.revision;
        self.handoff.mark_ready(status, by, at);
    }

    /// World matrix of a node, honouring auto layout.
    pub fn node_matrix(&mut self, id: NodeId) -> Option<Mat2D> {
        self.layout().get(id).map(|layout| layout.matrix)
    }

    /// World bounds of a node, honouring auto layout.
    pub fn node_bounds(&mut self, id: NodeId) -> Option<Rect> {
        self.layout().get(id).map(|layout| layout.world)
    }

    /// Union of the world bounds of the current selection.
    pub fn selection_bounds(&mut self) -> Option<Rect> {
        let ids = self.selection.clone();
        let layout = self.layout();
        let mut bounds: Option<Rect> = None;
        for id in ids {
            if let Some(node_layout) = layout.get(id) {
                bounds = Some(match bounds {
                    Some(current) => current.union(node_layout.world),
                    None => node_layout.world,
                });
            }
        }
        bounds
    }

    /// Selects one node (replacing the selection unless `additive`).
    pub fn select(&mut self, id: NodeId, additive: bool) {
        if additive {
            if let Some(index) = self.selection.iter().position(|candidate| *candidate == id) {
                self.selection.remove(index);
            } else {
                self.selection.push(id);
            }
        } else {
            self.selection = vec![id];
        }
        self.update_selection_status();
    }

    /// Replaces the selection by id list, keeping only nodes on the active page.
    pub fn select_many(&mut self, ids: &[NodeId]) {
        let page = self.document.active_page();
        self.selection = ids
            .iter()
            .copied()
            .filter(|id| page.get(*id).is_some())
            .collect();
        self.update_selection_status();
    }

    /// Clears the selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.update_selection_status();
    }

    /// Sets the active tool.
    pub fn set_tool(&mut self, tool: ToolId) {
        self.tool = tool;
        if tool != ToolId::Move && tool != ToolId::Node {
            self.hover = None;
        }
        self.status = tool.hint().to_owned();
    }

    /// Sets the viewport size (CSS pixels and device pixel ratio).
    pub fn set_viewport_size(&mut self, width: f64, height: f64, dpr: f64) {
        self.viewport.width = width.max(1.0);
        self.viewport.height = height.max(1.0);
        self.viewport.dpr = if dpr.is_finite() && dpr > 0.0 {
            dpr.clamp(0.5, 4.0)
        } else {
            1.0
        };
    }

    /// Zooms to an absolute zoom factor, anchored at the viewport centre.
    pub fn zoom_to(&mut self, zoom: f64) {
        self.viewport.zoom_to(zoom);
    }

    /// Zooms in one step.
    pub fn zoom_in(&mut self) {
        self.viewport
            .zoom_to((self.viewport.zoom * 1.25).min(dp_render::MAX_ZOOM));
    }

    /// Zooms out one step.
    pub fn zoom_out(&mut self) {
        self.viewport
            .zoom_to((self.viewport.zoom / 1.25).max(dp_render::MIN_ZOOM));
    }

    /// Resets zoom to 100%.
    pub fn zoom_100(&mut self) {
        self.viewport.zoom_to(1.0);
    }

    /// Fits the page content into the viewport.
    pub fn fit_to_content(&mut self) {
        let bounds = self.content_bounds_world();
        match bounds {
            Some(bounds) if bounds.w > 0.0 && bounds.h > 0.0 => {
                self.viewport.fit_rect(bounds, 48.0);
            }
            _ => {
                self.viewport.zoom_to(1.0);
            }
        }
    }

    /// Fits the selection into the viewport.
    pub fn zoom_to_selection(&mut self) {
        if let Some(bounds) = self.selection_bounds() {
            self.viewport.fit_rect(bounds, 48.0);
        }
    }

    /// Pans the viewport by a CSS-pixel delta.
    pub fn pan_by_css(&mut self, dx: f64, dy: f64) {
        self.viewport.pan_css(dx, dy);
    }

    /// World bounds of all visible page content.
    pub fn content_bounds_world(&mut self) -> Option<Rect> {
        let page = self.document.active_page_id();
        let mut text = std::mem::replace(&mut self.text_engine, TextEngine::new());
        let visible = Rect::new(-1.0e7, -1.0e7, 2.0e7, 2.0e7);
        let result = SceneBuilder::new(&self.document)
            .build(page, visible, &mut text)
            .ok()
            .and_then(|list| {
                use dp_render::SceneShape;
                let mut bounds: Option<Rect> = None;
                for item in &list.items {
                    if matches!(item.shape, SceneShape::Empty) {
                        continue;
                    }
                    bounds = Some(match bounds {
                        Some(current) => current.union(item.world_bounds),
                        None => item.world_bounds,
                    });
                }
                bounds
            });
        self.text_engine = text;
        self.invalidate_layout();
        result
    }

    /// Undoes the last transaction.
    pub fn undo(&mut self) -> Result<bool, EditorError> {
        match self.history.undo(&mut self.document)? {
            Some(transaction) => {
                self.invalidate_layout();
                self.dirty = true;
                self.text_edit = None;
                let remaining: Vec<NodeId> = self
                    .selection
                    .iter()
                    .copied()
                    .filter(|id| self.document.node(*id).is_some())
                    .collect();
                self.selection = remaining;
                self.status = format!("Undid {}", transaction.label);
                Ok(true)
            }
            None => {
                self.status = "Nothing to undo".to_owned();
                Ok(false)
            }
        }
    }

    /// Redoes the last undone transaction.
    pub fn redo(&mut self) -> Result<bool, EditorError> {
        match self.history.redo(&mut self.document)? {
            Some(transaction) => {
                self.invalidate_layout();
                self.dirty = true;
                self.text_edit = None;
                self.status = format!("Redid {}", transaction.label);
                Ok(true)
            }
            None => {
                self.status = "Nothing to redo".to_owned();
                Ok(false)
            }
        }
    }

    /// Applies a prepared transaction and records it in history.
    pub fn apply(&mut self, transaction: Transaction) -> Result<Transaction, EditorError> {
        let label = transaction.label.clone();
        let applied =
            dp_model::apply_transaction(&mut self.document, &mut self.history, transaction)?;
        self.invalidate_layout();
        self.dirty = true;
        self.status = label.to_string();
        Ok(applied)
    }

    /// Builds and applies a single-node property edit from before/after snapshots.
    pub(crate) fn push_node_change(
        &mut self,
        page: PageId,
        before: &Node,
        after: &Node,
        label: impl Into<String>,
    ) -> Result<bool, EditorError> {
        if before == after {
            return Ok(false);
        }
        let mut transaction = Transaction::new(label);
        transaction.push(Edit::SetNode {
            page,
            before: before.clone(),
            after: after.clone(),
        });
        self.apply(transaction)?;
        Ok(true)
    }

    /// Saves the project to its current path (or a suggested path).
    pub fn save(&mut self) -> Result<PathBuf, EditorError> {
        let path = match &self.path {
            Some(path) => path.clone(),
            None => {
                let name = dp_persist::suggested_file_name(&self.document);
                let dir = std::env::current_dir()
                    .map_err(|error| EditorError::Persist(PersistError::Io(error)))?;
                dir.join(name)
            }
        };
        self.save_as(path)
    }

    /// Saves the project to a specific path.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<PathBuf, EditorError> {
        let path = path.as_ref().to_path_buf();
        let report = dp_persist::save_project(&path, &self.document)?;
        self.path = Some(path.clone());
        self.dirty = false;
        dp_persist::clear_recovery(&path)?;
        self.status = format!(
            "Saved {} ({} nodes, {} pages, {} bytes)",
            path.display(),
            report.node_count,
            report.page_count,
            report.bytes_written
        );
        Ok(path)
    }

    /// Writes a crash-recovery snapshot when the document is dirty.
    pub fn autosave(&mut self) -> Result<bool, EditorError> {
        if !self.dirty {
            return Ok(false);
        }
        let path = match &self.path {
            Some(path) => path.clone(),
            None => return Ok(false),
        };
        dp_persist::write_recovery(&path, &self.document)?;
        Ok(true)
    }

    /// Renders the current viewport into a frame.
    pub fn render(&mut self) -> Result<RenderFrame, EditorError> {
        let page_id = self.document.active_page_id();
        self.invalidate_layout();
        self.layout();
        let target = {
            let layout = self
                .layout_cache
                .as_ref()
                .ok_or_else(|| EditorError::Render("layout unavailable".to_owned()))?;
            let builder = SceneBuilder::with_layout(&self.document, layout);
            let mut list = builder
                .build(
                    page_id,
                    self.viewport.visible_world_rect(),
                    &mut self.text_engine,
                )
                .map_err(|error| EditorError::Render(error.to_string()))?;
            // The interactive viewport paints workspace chrome, not the page
            // background: pages are infinite canvases whose artboards are
            // frames. White page backgrounds are an export-time decision (see
            // `export_options_for`), so the editor stays dark like the design
            // system's --bg-canvas token (`#101318`).
            list.background = if self.playback.is_some() {
                self.document.prototype.background
            } else {
                Color::from_hex("#101318").unwrap_or(Color::BLACK)
            };
            // While a DOM textarea owns the text being edited, the engine must
            // not paint the same text underneath it (double-vision in the UI).
            if let Some(edit) = &self.text_edit
                && let Some(item) = list.items.iter_mut().find(|item| item.node_id == edit.node_id)
            {
                item.text = None;
            }
            RenderTarget::render(&list, &self.viewport, &mut self.text_engine)
                .map_err(|error| EditorError::Render(error.to_string()))?
        };
        self.frame_id += 1;
        let mut overlay = self.build_overlay();
        if self.playback.is_some() {
            // Presentation mode shows the prototype, not the editing chrome.
            overlay.selection.clear();
            overlay.handles.clear();
            overlay.hover = None;
            overlay.marquee = None;
            overlay.snap_lines.clear();
            overlay.text_edit = None;
            overlay.tool_hint = String::new();
        }
        Ok(RenderFrame {
            frame_id: self.frame_id,
            width: target.width(),
            height: target.height(),
            rgba: target.data_straight_rgba8(),
            overlay,
        })
    }

    /// Layer tree summary for the pages/layers panels.
    pub fn summary(&self) -> DocSummary {
        let page = self.document.active_page();
        let mut nodes = Vec::new();
        for root in &page.roots {
            collect_tree(page, *root, 0, &mut nodes);
        }
        DocSummary {
            document_name: self.document.name.clone(),
            revision: self.document.revision,
            pages: self
                .document
                .pages
                .iter()
                .map(|list| PageSummary {
                    id: list.id,
                    name: list.name.clone(),
                })
                .collect(),
            active_page: page.id,
            nodes,
            components: self
                .document
                .components
                .components
                .values()
                .map(|component| component.name.clone())
                .collect(),
            styles: {
                let styles = &self.document.styles;
                let mut names: Vec<String> = Vec::new();
                names.extend(styles.paints.values().map(|style| style.name.clone()));
                names.extend(styles.texts.values().map(|style| style.name.clone()));
                names.extend(styles.effects.values().map(|style| style.name.clone()));
                names
            },
            variables: self
                .document
                .variables
                .variables
                .values()
                .map(|variable| variable.name.clone())
                .collect(),
        }
    }

    /// Inspector details for the requested nodes.
    pub fn node_details(&mut self, ids: &[NodeId]) -> Vec<NodeDetail> {
        self.layout();
        let mut details = Vec::new();
        for id in ids {
            let Some(node) = self.document.node(*id) else {
                continue;
            };
            let world = self
                .layout_cache
                .as_ref()
                .and_then(|layout| layout.get(*id))
                .map(|layout| layout.world)
                .unwrap_or(Rect::new(
                    node.transform.x,
                    node.transform.y,
                    node.transform.w,
                    node.transform.h,
                ));
            details.push(NodeDetail {
                id: *id,
                name: node.name.clone(),
                kind: node.kind,
                visible: node.visible,
                locked: node.locked,
                opacity: node.opacity,
                transform: node.transform,
                absolute: AbsoluteBounds {
                    x: world.x,
                    y: world.y,
                    width: world.w,
                    height: world.h,
                },
                style: node.style.clone(),
                geometry: node.geometry.clone(),
                text: node.text.clone(),
                layout: node.layout.clone(),
                text_style_name: None,
            });
        }
        details
    }

    /// Exports a PNG of the selection (or the whole page).
    pub fn export_png(
        &mut self,
        path: impl AsRef<Path>,
        scale: f64,
    ) -> Result<PathBuf, EditorError> {
        self.export_png_scoped(path, scale, false, &[])
    }

    /// Exports a PNG honouring an explicit node scope and transparency.
    pub fn export_png_scoped(
        &mut self,
        path: impl AsRef<Path>,
        scale: f64,
        transparent: bool,
        ids: &[NodeId],
    ) -> Result<PathBuf, EditorError> {
        let options = self.export_options_for(scale, ids, transparent);
        let page = self.document.active_page_id();
        dp_io::export_page_to_file(
            &self.document,
            page,
            dp_io::ExportFormat::Png,
            &options,
            path.as_ref(),
        )?;
        self.status = format!("Exported PNG {}", path.as_ref().display());
        Ok(path.as_ref().to_path_buf())
    }

    /// Exports an SVG of the selection (or the whole page).
    pub fn export_svg(&mut self, path: impl AsRef<Path>) -> Result<PathBuf, EditorError> {
        self.export_svg_scoped(path, &[])
    }

    /// Exports an SVG for an explicit node scope.
    pub fn export_svg_scoped(
        &mut self,
        path: impl AsRef<Path>,
        ids: &[NodeId],
    ) -> Result<PathBuf, EditorError> {
        let options = self.export_options_for(1.0, ids, true);
        let page = self.document.active_page_id();
        dp_io::export_page_to_file(
            &self.document,
            page,
            dp_io::ExportFormat::Svg,
            &options,
            path.as_ref(),
        )?;
        self.status = format!("Exported SVG {}", path.as_ref().display());
        Ok(path.as_ref().to_path_buf())
    }

    /// Exports a PDF of the selection (or the whole page).
    pub fn export_pdf(&mut self, path: impl AsRef<Path>) -> Result<PathBuf, EditorError> {
        self.export_pdf_scoped(path, &[])
    }

    /// Exports a PDF for an explicit node scope.
    pub fn export_pdf_scoped(
        &mut self,
        path: impl AsRef<Path>,
        ids: &[NodeId],
    ) -> Result<PathBuf, EditorError> {
        let options = self.export_options_for(1.0, ids, false);
        let page = self.document.active_page_id();
        dp_io::export_page_to_file(
            &self.document,
            page,
            dp_io::ExportFormat::Pdf,
            &options,
            path.as_ref(),
        )?;
        self.status = format!("Exported PDF {}", path.as_ref().display());
        Ok(path.as_ref().to_path_buf())
    }

    /// Imports SVG artwork from a file as a new frame on the active page.
    ///
    /// Artwork stays editable: paths become vector nodes with cubic anchors,
    /// solid fills and strokes are preserved, gradients are reduced to their
    /// first stop, and text is converted to outlines by the importer (the
    /// returned warnings say so explicitly).
    pub fn import_svg(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(NodeId, Vec<String>), EditorError> {
        self.import_svg_at(path, None, None)
    }

    /// Imports SVG artwork into a specific page and optional container.
    ///
    /// `target_page` defaults to the active page; `parent` must be a container
    /// node on that page (defaults to the page root).
    pub fn import_svg_at(
        &mut self,
        path: impl AsRef<Path>,
        target_page: Option<PageId>,
        parent: Option<NodeId>,
    ) -> Result<(NodeId, Vec<String>), EditorError> {
        let path = path.as_ref();
        let bytes =
            std::fs::read(path).map_err(|error| EditorError::Persist(PersistError::Io(error)))?;
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Imported SVG")
            .to_owned();
        let imported = dp_io::import::import_svg_bytes(&bytes, &name)?;
        let source_page = imported
            .document
            .page(imported.document.active_page_id())
            .ok_or_else(|| EditorError::Unsupported("imported SVG has no page".to_owned()))?;
        let ids = source_page.subtree_ids(imported.frame);
        if ids.is_empty() {
            return Err(EditorError::Unsupported("imported SVG is empty".to_owned()));
        }

        let mut remap = std::collections::HashMap::with_capacity(ids.len());
        for id in &ids {
            remap.insert(*id, NodeId::new_v4());
        }

        // The imported document is self-contained; copy the frame subtree into
        // the open document with fresh ids. `Page::insert_node` rebuilds the
        // parent/child links, so child lists are cleared here (pre-order inserts
        // then preserve sibling order).
        let page = match target_page {
            Some(id) => {
                if self.document.page(id).is_none() {
                    return Err(dp_model::ModelError::PageNotFound(id).into());
                }
                id
            }
            None => self.document.active_page_id(),
        };
        if let Some(parent_id) = parent {
            let valid = self
                .document
                .page(page)
                .and_then(|target| target.get(parent_id))
                .map(|node| node.kind.is_container())
                .unwrap_or(false);
            if !valid {
                return Err(EditorError::Unsupported(
                    "SVG import target must be a container on the target page".to_owned(),
                ));
            }
        }
        let mut transaction = Transaction::new("Import SVG");
        for id in &ids {
            let Some(source) = imported.document.node(*id) else {
                continue;
            };
            let mut node = source.clone();
            node.id = remap[id];
            node.parent = if *id == imported.frame {
                parent
            } else {
                source.parent.and_then(|parent| remap.get(&parent).copied())
            };
            node.children.clear();
            node.rev = 0;
            transaction.push(Edit::InsertNode {
                page,
                node: node.clone(),
                parent: node.parent,
                // usize::MAX is clamped to the end, so pre-order inserts append.
                index: usize::MAX,
            });
        }
        let root = remap[&imported.frame];
        self.apply(transaction)?;
        if page == self.document.active_page_id() {
            self.select(root, false);
        }
        self.status = if imported.warnings.is_empty() {
            format!("Imported {} as editable vectors", path.display())
        } else {
            imported.warnings.join("; ")
        };
        Ok((root, imported.warnings))
    }

    /// Image import needs the asset-storage pipeline (hash, dedupe, embedded
    /// bytes); it is intentionally not faked here.
    pub fn import_image(&mut self, _path: impl AsRef<Path>) -> Result<(), EditorError> {
        Err(EditorError::Unsupported(
            "image import is not implemented yet: it lands with the asset-storage milestone"
                .to_owned(),
        ))
    }

    fn export_options_for(
        &mut self,
        scale: f64,
        ids: &[NodeId],
        transparent: bool,
    ) -> ExportOptions {
        let explicit_bounds = if ids.is_empty() {
            None
        } else {
            self.nodes_bounds(ids)
        };
        let bounds = explicit_bounds.or_else(|| self.selection_bounds());
        let background = if transparent {
            Some(Color::TRANSPARENT)
        } else {
            Some(self.document.active_page().background)
        };
        ExportOptions {
            scale,
            background,
            bounds,
            clip: None,
            precision: 2,
            jpeg_quality: 90,
        }
    }

    /// Union of the world bounds of the given nodes (used by scoped exports).
    pub fn nodes_bounds(&mut self, ids: &[NodeId]) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        for id in ids {
            let Some(rect) = self.node_bounds(*id) else {
                continue;
            };
            bounds = Some(match bounds {
                Some(current) => current.union(rect),
                None => rect,
            });
        }
        bounds
    }

    /// Starts editing the text of a node.
    pub fn text_edit_begin(&mut self, node_id: NodeId) -> Result<(), EditorError> {
        let page = self.document.active_page_id();
        let node = self
            .document
            .active_page()
            .get(node_id)
            .ok_or(dp_model::ModelError::NodeNotFound(node_id))?;
        if node.kind != NodeKind::Text || node.text.is_none() {
            return Err(EditorError::Unsupported(
                "text editing is only available for text layers".to_owned(),
            ));
        }
        self.text_edit = Some(TextEditState {
            node_id,
            page,
            original: node.clone(),
            caret: node
                .text
                .as_ref()
                .map(|text| text.characters.chars().count())
                .unwrap_or(0),
            selection: None,
        });
        self.selection = vec![node_id];
        self.status = "Editing text — Enter commits, Esc cancels".to_owned();
        Ok(())
    }

    /// Updates the text being edited.
    pub fn text_edit_input(
        &mut self,
        characters: &str,
        caret: usize,
        selection: Option<(usize, usize)>,
    ) -> Result<(), EditorError> {
        let Some(edit) = self.text_edit.as_mut() else {
            return Err(EditorError::Unsupported("no active text edit".to_owned()));
        };
        let node_id = edit.node_id;
        if let Some(node) = self
            .document
            .page_mut(edit.page)
            .and_then(|page| page.nodes.get_mut(&node_id))
        {
            if let Some(text) = node.text.as_mut() {
                text.characters = characters.to_owned();
            }
            let len = characters.chars().count();
            edit.caret = caret.min(len);
            edit.selection = selection;
        }
        self.invalidate_layout();
        Ok(())
    }

    /// Moves the caret during a text edit.
    pub fn text_edit_set_caret(
        &mut self,
        caret: usize,
        selection: Option<(usize, usize)>,
    ) -> Result<(), EditorError> {
        let Some(edit) = self.text_edit.as_mut() else {
            return Err(EditorError::Unsupported("no active text edit".to_owned()));
        };
        let len = self
            .document
            .node(edit.node_id)
            .and_then(|node| node.text.as_ref())
            .map(|text| text.characters.chars().count())
            .unwrap_or(0);
        edit.caret = caret.min(len);
        edit.selection = selection;
        Ok(())
    }

    /// Commits the active text edit as one undoable transaction.
    pub fn text_edit_commit(&mut self) -> Result<bool, EditorError> {
        let Some(edit) = self.text_edit.take() else {
            return Ok(false);
        };
        let after = self
            .document
            .node(edit.node_id)
            .cloned()
            .unwrap_or_else(|| edit.original.clone());
        let changed = self.push_node_change(edit.page, &edit.original, &after, "Edit text")?;
        if !changed {
            self.status = "Text unchanged".to_owned();
        }
        self.invalidate_layout();
        Ok(changed)
    }

    /// Cancels the active text edit and restores the original text.
    pub fn text_edit_cancel(&mut self) -> bool {
        let Some(edit) = self.text_edit.take() else {
            return false;
        };
        if let Some(page) = self.document.page_mut(edit.page) {
            page.nodes.insert(edit.node_id, edit.original);
        }
        self.invalidate_layout();
        self.status = "Text edit cancelled".to_owned();
        true
    }

    /// Records a UI-level status message.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    fn update_selection_status(&mut self) {
        self.status = match self.selection.len() {
            0 => "Nothing selected".to_owned(),
            1 => self
                .selection
                .first()
                .and_then(|id| self.document.node(*id))
                .map(|node| format!("Selected {}", node.name))
                .unwrap_or_else(|| "Selected 1 layer".to_owned()),
            count => format!("Selected {count} layers"),
        };
    }
}

fn collect_tree(page: &Page, id: NodeId, depth: u32, out: &mut Vec<TreeNode>) {
    let Some(node) = page.get(id) else {
        return;
    };
    out.push(TreeNode {
        id,
        name: node.name.clone(),
        kind: node.kind,
        depth,
        parent: node.parent,
        visible: node.visible,
        locked: node.locked,
        child_count: node.children.len(),
        width: node.transform.w,
        height: node.transform.h,
    });
    for child in &node.children {
        collect_tree(page, *child, depth + 1, out);
    }
}

/// Helper used by tests and the desktop shell: a dark canvas default colour.
pub fn default_canvas_color() -> Color {
    Color::from_hex("#101318").unwrap_or(Color::BLACK)
}
