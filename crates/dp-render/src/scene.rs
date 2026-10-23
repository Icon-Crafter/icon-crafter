//! Display list: the renderer-input representation of one frame.
//!
//! The scene builder resolves the document tree into a flat, paint-ordered
//! list of draw items. Each item carries a world matrix, resolved paints and an
//! accumulated clip rectangle. The CPU rasterizer and any future GPU backend
//! consume this list; neither has to understand the document model.

use std::sync::Arc;

use dp_geom::{FillRule, Mat2D, Point, Rect, VectorPath};
use dp_model::{BlendMode, Color, NodeId, StrokeAlign};
use dp_text::TextLayout;

/// A gradient stop after resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneStop {
    /// Position in the gradient, 0..=1.
    pub position: f64,
    /// Stop colour.
    pub color: Color,
}

/// A resolved paint (variables and shared styles already applied).
#[derive(Debug, Clone, PartialEq)]
pub enum ScenePaint {
    /// Flat colour.
    Solid(Color),
    /// Linear gradient in node-local coordinates.
    Linear {
        /// Start point.
        from: Point,
        /// End point.
        to: Point,
        /// Sorted stops.
        stops: Vec<SceneStop>,
    },
    /// Radial gradient in node-local coordinates.
    Radial {
        /// Centre point.
        center: Point,
        /// Radius.
        radius: f64,
        /// Sorted stops.
        stops: Vec<SceneStop>,
    },
}

/// A resolved fill.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneFill {
    /// Paint.
    pub paint: ScenePaint,
    /// Fill rule for the shape.
    pub rule: FillRule,
    /// Fill-level opacity (0..=1).
    pub opacity: f64,
}

/// A resolved stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneStroke {
    /// Paint.
    pub paint: ScenePaint,
    /// Stroke width in document units.
    pub weight: f64,
    /// Requested alignment (inside/outside are approximated, see builder).
    pub align: StrokeAlign,
    /// Line cap.
    pub cap: dp_model::LineCap,
    /// Line join.
    pub join: dp_model::LineJoin,
    /// Miter limit.
    pub miter_limit: f64,
    /// Dash pattern in document units, if any.
    pub dash: Option<Vec<f64>>,
    /// Stroke-level opacity (0..=1).
    pub opacity: f64,
}

/// A text draw item.
#[derive(Debug, Clone)]
pub struct SceneText {
    /// Shaped and wrapped layout.
    pub layout: Arc<TextLayout>,
    /// Colour used to tint the glyph mask (first solid fill).
    pub color: Color,
}

/// Geometry to fill, already in node-local coordinates.
#[derive(Debug, Clone)]
pub enum SceneShape {
    /// A vector path (curves preserved).
    Path(Arc<VectorPath>),
    /// Nothing to draw (containers, unsupported nodes).
    Empty,
}

/// One drawable item in paint order (back to front).
#[derive(Debug, Clone)]
pub struct SceneItem {
    /// Source document node.
    pub node_id: NodeId,
    /// Node-local to device-pixel matrix (includes the viewport).
    pub matrix: Mat2D,
    /// Axis-aligned bounds in document coordinates.
    pub world_bounds: Rect,
    /// Accumulated clip rectangle in document coordinates.
    pub clip: Option<Rect>,
    /// Effective opacity including ancestors (0..=1).
    pub opacity: f64,
    /// Blend mode.
    pub blend: BlendMode,
    /// Shape.
    pub shape: SceneShape,
    /// Fills, painted in order.
    pub fills: Vec<SceneFill>,
    /// Strokes, painted after fills.
    pub strokes: Vec<SceneStroke>,
    /// Text payload, if this is a text node.
    pub text: Option<SceneText>,
}

/// A resolved frame ready for rasterization.
#[derive(Debug, Clone, Default)]
pub struct DisplayList {
    /// Page background.
    pub background: Color,
    /// Draw items in paint order.
    pub items: Vec<SceneItem>,
    /// Non-fatal issues encountered while building (unsupported features).
    pub warnings: Vec<String>,
    /// Number of nodes skipped by culling or invisibility.
    pub culled: usize,
}

impl DisplayList {
    /// Number of draw items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is drawn.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Adds a warning once (deduplicated).
    pub fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }
}
