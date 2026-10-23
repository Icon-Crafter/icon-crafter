//! Prototype presentation: the editor-side playback session.
//!
//! The authoritative interaction model lives in `dp-model::prototype` and the
//! execution engine in `dp-prototype`. This module stores the presentation
//! session as a replayable action log so the editor never has to keep a
//! self-referential runtime alive: every interaction rebuilds the runtime from
//! the document and replays the recorded actions. That keeps the runtime a pure
//! function of `(document, actions)` — deterministic, undo-independent and
//! testable.

use dp_geom::{Mat2D, Point, Rect};
use dp_model::{Document, NodeId, NodeKind, Page};
use dp_prototype::{PrototypeRuntime, RuntimeError};

/// One recorded presentation interaction, replayed in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackAction {
    /// A click on a node that carries a click interaction.
    Click(NodeId),
    /// Navigate back (history or overlays).
    Back,
    /// Close the top overlay.
    CloseOverlay,
}

/// Editor-side state of one presentation session.
#[derive(Debug, Clone)]
pub struct PrototypePlayback {
    /// Frame the session started on (replay seed).
    pub start_frame: NodeId,
    /// Frame currently presented.
    pub frame: NodeId,
    /// Recorded interactions, replayable from `start_frame`.
    pub actions: Vec<PlaybackAction>,
    /// Navigation history length inside the runtime.
    pub history_len: usize,
    /// Open overlay count inside the runtime.
    pub overlay_count: usize,
}

impl PrototypePlayback {
    /// Creates a fresh session.
    pub fn new(start_frame: NodeId, frame: NodeId) -> PrototypePlayback {
        PrototypePlayback {
            start_frame,
            frame,
            actions: Vec::new(),
            history_len: 0,
            overlay_count: 0,
        }
    }

    /// Rebuilds and replays a runtime for the given actions.
    pub fn replay<'a>(
        document: &'a Document,
        start_frame: NodeId,
        actions: &[PlaybackAction],
    ) -> Result<PrototypeRuntime<'a>, RuntimeError> {
        let mut runtime = PrototypeRuntime::start(document, Some(start_frame))?;
        for action in actions {
            match action {
                PlaybackAction::Click(node_id) => {
                    runtime.click(*node_id)?;
                }
                PlaybackAction::Back => {
                    runtime.back();
                }
                PlaybackAction::CloseOverlay => {
                    runtime.close_overlay();
                }
            }
            // Finish any transition so the runtime state reflects the resulting
            // frame instead of a half-finished animation.
            runtime.tick(10_000.0);
        }
        Ok(runtime)
    }

    /// Captures the runtime state after replaying `actions`.
    pub fn from_runtime(
        runtime: &PrototypeRuntime<'_>,
        start_frame: NodeId,
        actions: Vec<PlaybackAction>,
    ) -> PrototypePlayback {
        PrototypePlayback {
            start_frame,
            frame: runtime.current_frame(),
            actions,
            history_len: runtime.history().len(),
            overlay_count: runtime.overlays().len(),
        }
    }

    /// Whether there is anywhere to go back to.
    pub fn can_back(&self) -> bool {
        self.history_len > 0 || self.overlay_count > 0
    }
}

/// Finds the topmost node carrying prototype interactions under a world point.
///
/// Hit testing is bounds-based (rotated nodes use their world AABB); interactions
/// on ancestor groups are reachable because every interactive ancestor is
/// considered. Later paint order wins, mirroring what the user sees.
pub fn interactive_node_at(document: &Document, point: Point) -> Option<NodeId> {
    let page = document.active_page();
    let mut result = None;
    for root in &page.roots {
        walk_interactive(page, *root, Mat2D::IDENTITY, point, &mut result);
    }
    result
}

fn walk_interactive(
    page: &Page,
    id: NodeId,
    parent_matrix: Mat2D,
    point: Point,
    result: &mut Option<NodeId>,
) {
    let Some(node) = page.nodes.get(&id) else {
        return;
    };
    if !node.visible {
        return;
    }
    let matrix = parent_matrix.mul(node.transform.matrix());
    let bounds = matrix.apply_rect(Rect::new(
        0.0,
        0.0,
        node.transform.w.max(0.0),
        node.transform.h.max(0.0),
    ));
    let interactive = node
        .prototype
        .as_ref()
        .is_some_and(|prototype| !prototype.interactions.is_empty());
    if interactive && point_in_rect(bounds, point) {
        *result = Some(id);
    }
    for child in &node.children {
        walk_interactive(page, *child, matrix, point, result);
    }
}

fn point_in_rect(rect: Rect, point: Point) -> bool {
    point.x >= rect.x && point.x <= rect.right() && point.y >= rect.y && point.y <= rect.bottom()
}

/// Whether a node can be used as a prototype start frame.
pub fn is_frame(document: &Document, id: NodeId) -> bool {
    document
        .node(id)
        .map(|node| node.kind == NodeKind::Frame)
        .unwrap_or(false)
}
