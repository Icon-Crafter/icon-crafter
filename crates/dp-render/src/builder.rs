//! Scene builder: resolves a document page into a display list.
//!
//! The builder walks the document in paint order (first child painted first),
//! resolves fills/strokes through shared styles and variables, shapes text
//! with the text engine, computes world matrices and clips, and culls nodes
//! outside the visible region. Unsupported features are skipped with explicit
//! warnings instead of being drawn incorrectly.

use std::sync::Arc;

use dp_geom::{FillRule as GeomFillRule, Mat2D, Point, Rect, Size, VectorPath};
use dp_layout::LayoutMap;
use dp_model::{
    Color, ComponentRole, Document, Fill, FillRule, Node, NodeId, NodeKind, NodeStyle, Page, PageId,
    Paint, Stroke, StrokeAlign, TextContent, TextStyle,
};
use dp_text::TextEngine;

use crate::scene::{
    DisplayList, SceneFill, SceneItem, ScenePaint, SceneShape, SceneStop, SceneStroke, SceneText,
};

/// Resolves a document page into a [`DisplayList`].
pub struct SceneBuilder<'a> {
    document: &'a Document,
    layout: Option<&'a LayoutMap>,
}

impl<'a> SceneBuilder<'a> {
    /// Creates a builder that computes its own layout.
    pub fn new(document: &'a Document) -> Self {
        SceneBuilder {
            document,
            layout: None,
        }
    }

    /// Creates a builder that reuses a previously computed layout.
    pub fn with_layout(document: &'a Document, layout: &'a LayoutMap) -> Self {
        SceneBuilder {
            document,
            layout: Some(layout),
        }
    }

    /// Builds the display list for a page.
    ///
    /// `visible` is the visible document rectangle used for culling.
    pub fn build(
        &self,
        page_id: PageId,
        visible: Rect,
        text: &mut TextEngine,
    ) -> Result<DisplayList, BuildError> {
        let page = self
            .document
            .page(page_id)
            .ok_or(BuildError::MissingPage(page_id))?;
        let fallback_layout;
        let layout: &LayoutMap = match self.layout {
            Some(layout) => layout,
            None => {
                let mut engine = dp_layout::LayoutEngine::new();
                fallback_layout = engine.compute_page(page);
                &fallback_layout
            }
        };

        let mut list = DisplayList {
            background: page.background,
            ..DisplayList::default()
        };
        let mut builder = Builder {
            document: self.document,
            page,
            layout,
            text,
            list: &mut list,
            visible,
        };
        for root in page.roots() {
            builder.visit(*root, None, 1.0);
        }
        Ok(list)
    }
}

/// Errors that prevent building a display list.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The requested page is not part of the document.
    #[error("page {0:?} does not exist")]
    MissingPage(PageId),
    /// The layout map has no entry for a node.
    #[error("node {0:?} has no layout")]
    MissingLayout(NodeId),
}

struct Builder<'a> {
    document: &'a Document,
    page: &'a Page,
    layout: &'a LayoutMap,
    text: &'a mut TextEngine,
    list: &'a mut DisplayList,
    visible: Rect,
}

impl Builder<'_> {
    fn visit(&mut self, id: NodeId, parent_clip: Option<Rect>, parent_opacity: f64) {
        let Some(node) = self.page.get(id) else {
            return;
        };
        if !node.visible {
            self.list.culled += 1;
            return;
        }
        let Some(layout) = self.layout.get(id) else {
            return;
        };

        let opacity = (parent_opacity * node.opacity).clamp(0.0, 1.0);
        let matrix = layout.matrix;
        let local = layout.size;
        let world_bounds = matrix.apply_rect(Rect::new(0.0, 0.0, local.w, local.h));

        // A clipping container also clips its own bounds.
        let mut clip = parent_clip;
        if node.kind.clips_content() {
            clip = Some(match parent_clip {
                Some(existing) => intersect_rects(existing, world_bounds),
                None => world_bounds,
            });
        }

        // Component instances render their master's content at the instance
        // transform. Masters on other pages are not rendered yet (warned)
        // because a builder only holds one page; the instance then draws its
        // own box as an honest fallback.
        if node.kind == NodeKind::Instance {
            if self.render_instance(node, matrix, clip, opacity) {
                return;
            }
            if let Some(item) = self.make_item(node, matrix, local, world_bounds, clip, opacity) {
                self.list.items.push(item);
            }
            return;
        }

        // Cull leaves outside the visible region. Containers are always walked
        // because unclipped children may extend past their bounds.
        let is_container = !node.children.is_empty();
        let culled = !intersects_rect(world_bounds, self.visible);
        if culled {
            self.list.culled += 1;
            if !is_container {
                return;
            }
        }

        if !culled
            && let Some(item) = self.make_item(node, matrix, local, world_bounds, clip, opacity)
        {
            self.list.items.push(item);
        }

        let child_clip = if node.kind.clips_content() {
            clip
        } else {
            parent_clip
        };
        if let Some(children) = self.page.children(id) {
            for child in children {
                self.visit(*child, child_clip, opacity);
            }
        }
    }

    /// Renders a component instance's master subtree.
    ///
    /// Returns `true` when the instance was expanded (or deliberately skipped
    /// with a warning) and the caller must not draw the instance box; `false`
    /// asks the caller to fall back to drawing the instance itself.
    fn render_instance(
        &mut self,
        instance: &Node,
        instance_matrix: Mat2D,
        clip: Option<Rect>,
        opacity: f64,
    ) -> bool {
        let Some(role) = &instance.component else {
            self.list
                .warn("component instance has no component reference; drawing its box instead");
            return false;
        };
        let ComponentRole::Instance { component_id, .. } = role else {
            self.list
                .warn("instance node has a non-instance component role; drawing its box instead");
            return false;
        };
        let Some(meta) = self.document.components.component(*component_id) else {
            self.list
                .warn("component instance references a missing component; drawing its box instead");
            return false;
        };
        let Some((master_page, master)) = self.document.locate_node(meta.node_id) else {
            self.list
                .warn("component instance references a missing master node; drawing its box instead");
            return false;
        };
        if master_page.id != self.page.id {
            self.list.warn(
                "component masters on other pages are not rendered yet; drawing the instance box instead",
            );
            return false;
        }
        self.visit_master(master, instance_matrix, clip, opacity, 0);
        true
    }

    /// Walks a component master subtree using transforms only (the instance
    /// replaces the master's own transform, so layout-cache matrices do not
    /// apply).
    fn visit_master(
        &mut self,
        node: &Node,
        base: Mat2D,
        parent_clip: Option<Rect>,
        parent_opacity: f64,
        depth: u32,
    ) {
        if depth > 16 {
            self.list.warn(
                "component instance nesting is deeper than 16 levels; deeper content was skipped",
            );
            return;
        }
        let opacity = (parent_opacity * node.opacity).clamp(0.0, 1.0);
        let local = Size::new(node.transform.w.max(0.0), node.transform.h.max(0.0));
        let world_bounds = base.apply_rect(Rect::new(0.0, 0.0, local.w, local.h));
        let mut clip = parent_clip;
        if node.kind.clips_content() {
            clip = Some(match parent_clip {
                Some(existing) => intersect_rects(existing, world_bounds),
                None => world_bounds,
            });
        }
        let is_container = !node.children.is_empty();
        let culled = !intersects_rect(world_bounds, self.visible);
        if culled && !is_container {
            self.list.culled += 1;
            return;
        }
        if !culled
            && let Some(item) = self.make_item(node, base, local, world_bounds, clip, opacity)
        {
            self.list.items.push(item);
        }
        for child_id in &node.children {
            let Some(child) = self.page.get(*child_id) else {
                self.list
                    .warn("component master has a child outside the page; skipped");
                continue;
            };
            if !child.visible {
                continue;
            }
            let child_base = base.mul(child.transform.matrix());
            self.visit_master(child, child_base, clip, opacity, depth + 1);
        }
    }

    fn make_item(
        &mut self,
        node: &Node,
        matrix: Mat2D,
        local: Size,
        world_bounds: Rect,
        clip: Option<Rect>,
        opacity: f64,
    ) -> Option<SceneItem> {
        let kind = node.kind;
        let size = Size::new(local.w.max(0.0), local.h.max(0.0));

        // Wireframe nodes are not rendered but are kept out of the display list.
        if matches!(
            kind,
            NodeKind::Slice | NodeKind::Sticky | NodeKind::Connector
        ) {
            return None;
        }

        let mut shape = SceneShape::Empty;
        let mut text_payload = None;
        let mut fills = Vec::new();
        let mut strokes = Vec::new();

        match kind {
            NodeKind::Frame
            | NodeKind::Group
            | NodeKind::Component
            | NodeKind::ComponentSet
            | NodeKind::Instance => {
                if kind != NodeKind::Group && has_paint(&node.style) {
                    shape = SceneShape::Path(Arc::new(rectangle_path(node)));
                }
            }
            NodeKind::Rectangle => {
                shape = SceneShape::Path(Arc::new(rectangle_path(node)));
            }
            NodeKind::Ellipse => {
                shape = SceneShape::Path(Arc::new(dp_geom::shapes::ellipse_path(Size::new(
                    node.transform.w.max(0.0),
                    node.transform.h.max(0.0),
                ))));
            }
            NodeKind::Line => {
                shape = SceneShape::Path(Arc::new(dp_geom::shapes::line_path(Size::new(
                    node.transform.w.max(0.0),
                    node.transform.h.max(0.0),
                ))));
            }
            NodeKind::Polygon => {
                let points = match node.geometry {
                    Some(dp_model::NodeGeometry::Polygon { point_count }) => point_count,
                    _ => 3,
                };
                shape = SceneShape::Path(Arc::new(dp_geom::shapes::polygon_path(
                    Size::new(node.transform.w.max(0.0), node.transform.h.max(0.0)),
                    points,
                )));
            }
            NodeKind::Star => {
                let (points, inner) = match node.geometry {
                    Some(dp_model::NodeGeometry::Star {
                        point_count,
                        inner_ratio,
                    }) => (point_count, inner_ratio),
                    _ => (5, 0.38),
                };
                shape = SceneShape::Path(Arc::new(dp_geom::shapes::star_path(
                    Size::new(node.transform.w.max(0.0), node.transform.h.max(0.0)),
                    points,
                    inner,
                )));
            }
            NodeKind::Vector => {
                if let Some(dp_model::NodeGeometry::Vector { path }) = &node.geometry {
                    shape = SceneShape::Path(Arc::new(path.clone()));
                }
            }
            NodeKind::Text => {
                if let Some(content) = &node.text {
                    let layout = self.layout_text(content, node);
                    let color = self.text_color(content, size);
                    text_payload = layout.map(|layout| SceneText {
                        layout: Arc::new(layout),
                        color,
                    });
                }
                fills = self.resolve_fills(&node.style, opacity, size);
                strokes = self.resolve_strokes(&node.style, size);
            }
            NodeKind::Boolean => {
                self.list.warn(
                    "boolean nodes render their operand children; evaluated boolean geometry is not rasterized yet",
                );
            }
            NodeKind::Image => {
                self.list
                    .warn("image fills are not rendered by the CPU renderer yet");
            }
            NodeKind::Slice | NodeKind::Sticky | NodeKind::Connector => {}
        }

        if matches!(
            kind,
            NodeKind::Frame
                | NodeKind::Group
                | NodeKind::Component
                | NodeKind::ComponentSet
                | NodeKind::Instance
                | NodeKind::Rectangle
                | NodeKind::Ellipse
                | NodeKind::Line
                | NodeKind::Polygon
                | NodeKind::Star
                | NodeKind::Vector
        ) {
            fills = self.resolve_fills(&node.style, opacity, size);
            strokes = self.resolve_strokes(&node.style, size);
        }

        if !node.style.effects.is_empty() {
            self.list
                .warn("layer effects (shadows/blur) are not rendered by the CPU renderer yet");
        }
        if !node.style.inset_stroke
            && node
                .style
                .strokes
                .iter()
                .any(|stroke| stroke.align != StrokeAlign::Center)
        {
            self.list
                .warn("inside/outside stroke alignment is approximated with a centred stroke");
        }

        if matches!(shape, SceneShape::Empty) && text_payload.is_none() {
            return None;
        }

        Some(SceneItem {
            node_id: node.id,
            matrix,
            world_bounds,
            clip,
            opacity,
            blend: node.blend,
            shape,
            fills,
            strokes,
            text: text_payload,
        })
    }

    fn layout_text(&mut self, content: &TextContent, node: &Node) -> Option<dp_text::TextLayout> {
        use dp_model::AutoSize;
        let (max_width, max_height) = match content.auto_size {
            AutoSize::WidthAndHeight => (None, None),
            AutoSize::Height => (Some(node.transform.w.max(0.0)), None),
            AutoSize::None => (
                Some(node.transform.w.max(0.0)),
                Some(node.transform.h.max(0.0)),
            ),
        };
        if content.characters.is_empty() {
            return None;
        }
        Some(self.text.layout(content, max_width, max_height))
    }

    fn text_color(&self, content: &TextContent, size: Size) -> Color {
        for fill in &content.style.fills {
            if !fill.visible {
                continue;
            }
            if let Some(paint) = self.resolve_paint(fill, size) {
                return match paint {
                    ScenePaint::Solid(color) => color,
                    ScenePaint::Linear { stops, .. } | ScenePaint::Radial { stops, .. } => {
                        stops.first().map(|stop| stop.color).unwrap_or(Color::BLACK)
                    }
                };
            }
        }
        Color::BLACK
    }

    fn resolve_fills(&self, style: &NodeStyle, opacity: f64, size: Size) -> Vec<SceneFill> {
        let mut out = Vec::new();
        for fill in &style.fills {
            if !fill.visible || fill.opacity <= 0.0 {
                continue;
            }
            let Some(paint) = self.resolve_paint(fill, size) else {
                continue;
            };
            out.push(SceneFill {
                paint,
                rule: match style.fill_rule {
                    FillRule::NonZero => GeomFillRule::NonZero,
                    FillRule::EvenOdd => GeomFillRule::EvenOdd,
                },
                opacity: (fill.opacity * opacity).clamp(0.0, 1.0),
            });
        }
        out
    }

    fn resolve_strokes(&self, style: &NodeStyle, size: Size) -> Vec<SceneStroke> {
        let mut out = Vec::new();
        for stroke in &style.strokes {
            if !stroke.visible || stroke.opacity <= 0.0 || stroke.weight <= 0.0 {
                continue;
            }
            let Some(paint) = self.resolve_stroke_paint(stroke, size) else {
                continue;
            };
            out.push(SceneStroke {
                paint,
                weight: stroke.weight,
                align: stroke.align,
                cap: stroke.cap,
                join: stroke.join,
                miter_limit: stroke.miter_limit,
                dash: stroke.dash.clone(),
                opacity: stroke.opacity.clamp(0.0, 1.0),
            });
        }
        out
    }

    fn resolve_stroke_paint(&self, stroke: &Stroke, size: Size) -> Option<ScenePaint> {
        self.resolve_paint_parts(&stroke.paint, stroke.style_id, stroke.variable_id, size)
    }

    /// Resolves a fill's paint through variables and shared styles.
    fn resolve_paint(&self, fill: &Fill, size: Size) -> Option<ScenePaint> {
        self.resolve_paint_parts(&fill.paint, fill.style_id, fill.variable_id, size)
    }

    fn resolve_paint_parts(
        &self,
        paint: &Paint,
        style_id: Option<dp_model::StyleId>,
        variable_id: Option<dp_model::VariableId>,
        size: Size,
    ) -> Option<ScenePaint> {
        let mode = self.page_mode();
        if let Some(variable) = variable_id {
            if let Some(color) = self.document.variables.resolve_color(variable, mode) {
                return Some(ScenePaint::Solid(color));
            }
            if let Some(dp_model::VariableValue::Color { color }) =
                self.document.variables.resolve(variable, mode)
            {
                return Some(ScenePaint::Solid(color));
            }
        }
        if let Some(style_id) = style_id
            && let Some(shared) = self.document.styles.paint(style_id)
        {
            return paint_to_scene(&shared.paint, size);
        }
        paint_to_scene(paint, size)
    }

    /// The active variable mode used for paint resolution.
    ///
    /// A random sentinel mode makes the registry fall back to each
    /// collection's default mode, which is the documented behaviour when the
    /// document has no explicitly activated modes.
    fn page_mode(&self) -> dp_model::ModeId {
        self.document
            .settings
            .active_modes
            .values()
            .next()
            .copied()
            .unwrap_or_else(dp_model::ModeId::new_v4)
    }
}

/// Converts a model paint into scene paint in absolute node-local coordinates.
///
/// Gradient positions in the document model are normalised to the node box
/// (`0..=1` on both axes); the renderer works in node-local units, so they are
/// scaled by the node size here. Radial radii are relative to the smaller box
/// extent, matching the model's documentation.
fn paint_to_scene(paint: &Paint, size: Size) -> Option<ScenePaint> {
    let w = size.w.max(0.0);
    let h = size.h.max(0.0);
    match paint {
        Paint::Solid { color } => Some(ScenePaint::Solid(*color)),
        Paint::Linear {
            from,
            to,
            stops,
            transform,
        } if transform.is_none() => Some(ScenePaint::Linear {
            from: Point::new(from.x * w, from.y * h),
            to: Point::new(to.x * w, to.y * h),
            stops: scene_stops(stops),
        }),
        Paint::Radial {
            center,
            radius,
            stops,
            transform,
        } if transform.is_none() => Some(ScenePaint::Radial {
            center: Point::new(center.x * w, center.y * h),
            radius: radius.max(0.0) * w.min(h),
            stops: scene_stops(stops),
        }),
        // Angular gradients, image fills and transformed gradients are not
        // supported by the CPU renderer yet. They are skipped (the builder can
        // warn) rather than approximated silently.
        _ => None,
    }
}

fn scene_stops(stops: &[dp_model::GradientStop]) -> Vec<SceneStop> {
    let mut out: Vec<SceneStop> = stops
        .iter()
        .map(|stop| SceneStop {
            position: stop.position.clamp(0.0, 1.0),
            color: stop.color,
        })
        .collect();
    out.sort_by(|a, b| a.position.total_cmp(&b.position));
    out
}

fn rectangle_path(node: &Node) -> VectorPath {
    dp_geom::shapes::rectangle_path(
        Size::new(node.transform.w.max(0.0), node.transform.h.max(0.0)),
        node.style.corner_radii,
    )
}

fn has_paint(style: &NodeStyle) -> bool {
    style.fills.iter().any(|fill| fill.visible) || style.strokes.iter().any(|s| s.visible)
}

fn intersects_rect(a: Rect, b: Rect) -> bool {
    a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
}

fn intersect_rects(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.bottom().min(b.bottom());
    Rect::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
}

/// Text styles are only used for colour selection here; layout is handled by
/// the text engine. This helper keeps the intent explicit.
#[allow(dead_code)]
fn first_solid(style: &TextStyle) -> Option<Color> {
    style.fills.first().map(|fill| match &fill.paint {
        Paint::Solid { color } => *color,
        _ => Color::BLACK,
    })
}
