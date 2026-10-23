//! Layout engine for Icon Crafter.
//!
//! The engine computes the *resolved geometry* of a page: absolute transforms,
//! resolved sizes (including hug/fill auto layout sizing), and world-space
//! bounding boxes used by hit testing, culling and the inspector.
//!
//! Design rules:
//!
//! * Pure function of `(document, page)`: it never mutates the document. All
//!   resolved geometry is returned in a [`LayoutMap`]. Persisted node transforms
//!   remain the authored intent; the engine derives the rest.
//! * Deterministic: iteration order follows document order, never hash order.
//! * Single-pass per node with a bounded recursion depth, then a placement pass.
//! * Text measurement is delegated to [`dp_text::TextEngine`] (which caches
//!   shaped layouts), so re-layout after an unrelated edit is cheap.
//!
//! Current limitations (tracked in `docs/FEATURE-CHECKLIST.md`):
//!
//! * Constraints are not re-solved here; the command layer applies them when a
//!   frame is resized, and this engine lays out the resulting transforms.
//! * Hug container with a fill child resolves the fill child as hug (Figma's
//!   behaviour differs for the "all children fill" degenerate case).
//! * Baseline alignment aligns the first text baseline of each child.

use std::collections::HashMap;

use dp_geom::{Mat2D, Point, Rect, Size};
use dp_model::geometry::{
    AutoLayout, AutoSize, CounterAlign, LayoutDirection, PrimaryAlign, SizeSpec, Sizing,
};
use dp_model::ids::NodeId;
use dp_model::node::{Node, NodeKind, Transform};
use dp_model::page::Page;

use dp_text::TextEngine;

/// Maximum nesting depth the solver descends into.
pub const MAX_LAYOUT_DEPTH: usize = 64;

/// Resolved layout for one node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeLayout {
    /// Local rectangle: position relative to the parent plus resolved size.
    pub local: Rect,
    /// Resolved size.
    pub size: Size,
    /// Local matrix including the node's own transform (rotation, flips).
    pub local_matrix: Mat2D,
    /// Page-space matrix (parent chain applied).
    pub matrix: Mat2D,
    /// Axis-aligned page-space bounds (conservative for rotated nodes).
    pub world: Rect,
}

/// Resolved layout for a whole page.
#[derive(Debug, Clone, Default)]
pub struct LayoutMap {
    nodes: HashMap<NodeId, NodeLayout>,
    order: Vec<NodeId>,
}

impl LayoutMap {
    /// Layout of one node.
    pub fn get(&self, id: NodeId) -> Option<&NodeLayout> {
        self.nodes.get(&id)
    }

    /// Number of laid out nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether no nodes were laid out.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Pre-order list of laid out node ids.
    pub fn order(&self) -> &[NodeId] {
        &self.order
    }

    /// Iterates over laid out nodes.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &NodeLayout)> {
        self.nodes.iter()
    }

    /// Page-space bounds of a node.
    pub fn world_bounds(&self, id: NodeId) -> Option<Rect> {
        self.nodes.get(&id).map(|layout| layout.world)
    }

    /// Resolved size of a node.
    pub fn size(&self, id: NodeId) -> Option<Size> {
        self.nodes.get(&id).map(|layout| layout.size)
    }

    fn insert(&mut self, id: NodeId, layout: NodeLayout) {
        if self.nodes.insert(id, layout).is_none() {
            self.order.push(id);
        }
    }
}

/// Computes resolved geometry for pages.
pub struct LayoutEngine {
    text: TextEngine,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    /// Creates an engine with its own text engine (loads system fonts).
    pub fn new() -> Self {
        Self {
            text: TextEngine::new(),
        }
    }

    /// Creates an engine that shares a text engine (useful in tests and for the
    /// editor, which also needs the text engine for the text editing overlay).
    pub fn with_text_engine(text: TextEngine) -> Self {
        Self { text }
    }

    /// The text engine used for measurement.
    pub fn text(&self) -> &TextEngine {
        &self.text
    }

    /// Mutable access to the text engine.
    pub fn text_mut(&mut self) -> &mut TextEngine {
        &mut self.text
    }

    /// Layouts one node subtree (for example inside a component editor).
    pub fn compute_node(&mut self, page: &Page, root: NodeId) -> LayoutMap {
        let mut solver = Solver::new(page, &mut self.text);
        let mut map = LayoutMap::default();
        if let Some(node) = page.nodes.get(&root).cloned() {
            let size = solver.compute_size(root, &node, None, 0);
            solver.place(
                root,
                &node,
                Mat2D::IDENTITY,
                Point::new(node.transform.x, node.transform.y),
                size,
                &mut map,
            );
        }
        map
    }

    /// Layouts every root of a page.
    pub fn compute_page(&mut self, page: &Page) -> LayoutMap {
        let mut solver = Solver::new(page, &mut self.text);
        let mut map = LayoutMap::default();
        for root in page.roots.clone() {
            let Some(node) = page.nodes.get(&root).cloned() else {
                continue;
            };
            let size = solver.compute_size(root, &node, None, 0);
            solver.place(
                root,
                &node,
                Mat2D::IDENTITY,
                Point::new(node.transform.x, node.transform.y),
                size,
                &mut map,
            );
        }
        map
    }
}

struct Solver<'a> {
    page: &'a Page,
    text: &'a mut TextEngine,
    sizes: HashMap<NodeId, Size>,
    positions: HashMap<NodeId, Point>,
    baselines: HashMap<NodeId, f64>,
}

impl<'a> Solver<'a> {
    fn new(page: &'a Page, text: &'a mut TextEngine) -> Self {
        Self {
            page,
            text,
            sizes: HashMap::new(),
            positions: HashMap::new(),
            baselines: HashMap::new(),
        }
    }

    /// Resolves the size of a node.
    ///
    /// `avail_width` is the width the parent offers (used by text wrapping in
    /// fill-width children). Computed sizes are cached per node; fill children
    /// are the only nodes measured twice and their final size overwrites the
    /// cached value.
    fn compute_size(
        &mut self,
        id: NodeId,
        node: &Node,
        avail_width: Option<f64>,
        depth: usize,
    ) -> Size {
        if depth > MAX_LAYOUT_DEPTH {
            tracing::warn!(
                ?id,
                "layout recursion depth exceeded; treating node as empty"
            );
            return Size::zero();
        }
        if avail_width.is_none() {
            if let Some(size) = self.sizes.get(&id) {
                return *size;
            }
        }

        let size = match node.kind {
            NodeKind::Text => self.text_size(node, avail_width),
            NodeKind::Group => self.group_size(node, depth),
            _ => {
                let has_auto_layout = node
                    .layout
                    .as_ref()
                    .map(|_| is_layout_container(node.kind))
                    .unwrap_or(false);
                if has_auto_layout {
                    self.auto_layout_size(node, node.layout.as_ref().unwrap(), depth)
                } else {
                    // Non-auto containers keep their authored size but their
                    // children still need resolved sizes for placement.
                    for child_id in node.children.clone() {
                        if let Some(child) = self.page.nodes.get(&child_id).cloned() {
                            self.compute_size(child_id, &child, None, depth + 1);
                        }
                    }
                    node.transform.size()
                }
            }
        };
        self.sizes.insert(id, size);
        size
    }

    fn text_size(&mut self, node: &Node, avail_width: Option<f64>) -> Size {
        let Some(content) = node.text.as_ref() else {
            return node.transform.size();
        };
        let size = match content.auto_size {
            AutoSize::WidthAndHeight => {
                let layout = self.text.layout(content, None, None);
                if let Some(line) = layout.lines.first() {
                    self.baselines.insert(node.id, line.baseline);
                }
                Size::new(layout.width, layout.height)
            }
            AutoSize::Height => {
                let width = avail_width
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .unwrap_or(node.transform.w);
                let layout = self.text.layout(content, Some(width), None);
                if let Some(line) = layout.lines.first() {
                    self.baselines.insert(node.id, line.baseline);
                }
                Size::new(width, layout.height)
            }
            AutoSize::None => {
                let width = avail_width
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .unwrap_or(node.transform.w);
                let height = node.transform.h;
                let layout = self.text.layout(content, Some(width), Some(height));
                if let Some(line) = layout.lines.first() {
                    self.baselines.insert(node.id, line.baseline);
                }
                Size::new(width, height)
            }
        };
        Size::new(size.w.max(0.0), size.h.max(0.0))
    }

    /// Groups without auto layout hug the union of their children.
    fn group_size(&mut self, node: &Node, depth: usize) -> Size {
        if node.layout.is_some() {
            return node.transform.size();
        }
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let children: Vec<NodeId> = node.children.clone();
        for child_id in children {
            let Some(child) = self.page.nodes.get(&child_id).cloned() else {
                continue;
            };
            let child_size = self.compute_size(child_id, &child, None, depth + 1);
            let x = child.transform.x;
            let y = child.transform.y;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x + child_size.w);
            max_y = max_y.max(y + child_size.h);
        }
        if !min_x.is_finite() {
            return node.transform.size();
        }
        let size = Size::new((max_x - min_x).max(0.0), (max_y - min_y).max(0.0));
        for child_id in node.children.iter().copied() {
            if let Some(child) = self.page.nodes.get(&child_id) {
                self.positions.insert(
                    child_id,
                    Point::new(child.transform.x - min_x, child.transform.y - min_y),
                );
            }
        }
        size
    }

    fn auto_layout_size(&mut self, node: &Node, layout: &AutoLayout, depth: usize) -> Size {
        let id = node.id;
        let padding = layout.padding;
        let direction = layout.direction;
        let hug_h = layout.sizing.h == SizeSpec::Hug;
        let hug_v = layout.sizing.v == SizeSpec::Hug;
        let vertical = matches!(direction, LayoutDirection::Vertical);

        // Fixed-axis content boxes; hugging axes are unknown until children are measured.
        let inner_w = if !hug_h {
            Some((node.transform.w - padding.horizontal()).max(0.0))
        } else {
            None
        };
        let inner_h = if !hug_v {
            Some((node.transform.h - padding.vertical()).max(0.0))
        } else {
            None
        };

        let mut child_ids: Vec<NodeId> = node
            .children
            .iter()
            .copied()
            .filter(|child_id| {
                self.page
                    .nodes
                    .get(child_id)
                    .map(|child| child.visible)
                    .unwrap_or(false)
            })
            .collect();
        if layout.item_reverse {
            child_ids.reverse();
        }

        // Measure children. Fill-width children in a vertical layout wrap their
        // text at the container width.
        let mut sizes: Vec<Size> = Vec::with_capacity(child_ids.len());
        for child_id in &child_ids {
            let Some(child) = self.page.nodes.get(child_id).cloned() else {
                sizes.push(Size::zero());
                continue;
            };
            let child_sizing = child_sizing(&child);
            let width_avail = if vertical && child_sizing.h == SizeSpec::Fill {
                inner_w
            } else {
                None
            };
            let size = self.compute_size(*child_id, &child, width_avail, depth + 1);
            sizes.push(size);
        }

        // Single arrangement pass (wrapping depends on the main axis extent).
        let inner_main_avail = if vertical { inner_h } else { inner_w };
        let arrangement = arrange(
            direction,
            layout.wrap,
            layout.gap.max(0.0),
            inner_main_avail,
            &sizes,
        );

        let mut content_main = 0.0_f64;
        let mut content_cross = 0.0_f64;
        for row in &arrangement.rows {
            content_main = content_main.max(row.main);
            content_cross += row.cross;
        }
        if arrangement.rows.len() > 1 {
            content_cross += layout.gap.max(0.0) * (arrangement.rows.len() - 1) as f64;
        }
        let (content_w, content_h) = if vertical {
            (content_cross, content_main)
        } else {
            (content_main, content_cross)
        };

        // Container size.
        let mut width = if hug_h {
            content_w + padding.horizontal()
        } else if layout.sizing.h == SizeSpec::Fill {
            inner_w
                .map(|value| value + padding.horizontal())
                .unwrap_or(node.transform.w)
        } else {
            node.transform.w
        };
        let mut height = if hug_v {
            content_h + padding.vertical()
        } else if layout.sizing.v == SizeSpec::Fill {
            inner_h
                .map(|value| value + padding.vertical())
                .unwrap_or(node.transform.h)
        } else {
            node.transform.h
        };
        let mut size = Size::new(width.max(0.0), height.max(0.0));
        if let Some(min) = layout.min_size {
            size = Size::new(size.w.max(min.w), size.h.max(min.h));
        }
        if let Some(max) = layout.max_size {
            size = Size::new(size.w.min(max.w.max(0.0)), size.h.min(max.h.max(0.0)));
        }
        width = size.w;
        height = size.h;
        self.sizes.insert(id, size);

        let inner_w = (width - padding.horizontal()).max(0.0);
        let inner_h = (height - padding.vertical()).max(0.0);
        let inner_main = if vertical { inner_h } else { inner_w };
        let inner_cross = if vertical { inner_w } else { inner_h };

        // Re-measure cross-axis fill children against the final container size.
        for (index, child_id) in child_ids.iter().enumerate() {
            let Some(child) = self.page.nodes.get(child_id).cloned() else {
                continue;
            };
            let child_sizing = child_sizing(&child);
            let fills_cross = if vertical {
                child_sizing.h == SizeSpec::Fill
            } else {
                child_sizing.v == SizeSpec::Fill
            };
            if !fills_cross {
                continue;
            }
            let mut child_size = sizes[index];
            if vertical {
                // Width changed: re-shape text at the new width.
                child_size.w = inner_cross;
                if child.kind == NodeKind::Text {
                    let avail = Some(inner_cross);
                    child_size = self.compute_size(*child_id, &child, avail, depth + 1);
                    child_size.w = inner_cross;
                } else if let Some(child_layout) = child.layout.as_ref() {
                    if is_layout_container(child.kind) {
                        let _ = child_layout;
                        child_size = self.compute_size(*child_id, &child, None, depth + 1);
                        child_size.w = inner_cross;
                    }
                }
            } else {
                child_size.h = inner_cross;
            }
            sizes[index] = child_size;
            self.sizes.insert(*child_id, child_size);
        }

        let arrangement = arrange(
            direction,
            layout.wrap,
            layout.gap.max(0.0),
            Some(inner_main),
            &sizes,
        );

        // Primary-axis distribution per row.
        for (row_index, row) in arrangement.rows.iter().enumerate() {
            let free = (inner_main - row.main).max(0.0);
            let n = row.indices.len();
            let space_between = layout.primary_align == PrimaryAlign::SpaceBetween && n > 1;
            let extra_gap = if space_between {
                free / (n - 1) as f64
            } else {
                0.0
            };
            let start = if space_between {
                0.0
            } else {
                match layout.primary_align {
                    PrimaryAlign::Start | PrimaryAlign::SpaceBetween => 0.0,
                    PrimaryAlign::Center => free / 2.0,
                    PrimaryAlign::End => free,
                }
            };
            let row_cross_offset: f64 = arrangement.rows[..row_index]
                .iter()
                .map(|previous| previous.cross + layout.gap.max(0.0))
                .sum();
            // With a single row the counter axis alignment happens inside the
            // container's content box; wrapped rows align inside their row.
            let row_cross_extent = if arrangement.rows.len() == 1 {
                inner_cross
            } else {
                row.cross
            };
            let mut cursor = start;
            for child_index in &row.indices {
                let child_id = child_ids[*child_index];
                let child_size = sizes[*child_index];
                let main_pos = cursor;
                let (main_extent, cross_extent) = if vertical {
                    (child_size.h, child_size.w)
                } else {
                    (child_size.w, child_size.h)
                };
                let child_baseline = self.baselines.get(&child_id).copied();
                let cross_pos = match layout.counter_align {
                    CounterAlign::Start | CounterAlign::Stretch => 0.0,
                    CounterAlign::Center => ((row_cross_extent - cross_extent) / 2.0).max(0.0),
                    CounterAlign::End => (row_cross_extent - cross_extent).max(0.0),
                    CounterAlign::Baseline => {
                        let row_baseline = row
                            .indices
                            .iter()
                            .filter_map(|index| self.baselines.get(&child_ids[*index]).copied())
                            .fold(0.0_f64, f64::max);
                        match child_baseline {
                            Some(baseline) => (row_baseline - baseline).max(0.0),
                            None => 0.0,
                        }
                    }
                };
                let cross_pos = cross_pos + row_cross_offset;
                let (x, y) = if vertical {
                    (padding.left + cross_pos, padding.top + main_pos)
                } else {
                    (padding.left + main_pos, padding.top + cross_pos)
                };
                self.positions.insert(child_id, Point::new(x, y));
                cursor += main_extent + layout.gap.max(0.0) + extra_gap;
            }
        }

        // Stretch overrides the resolved cross size after positioning.
        for child_id in &child_ids {
            let Some(child) = self.page.nodes.get(child_id).cloned() else {
                continue;
            };
            let child_sizing = child_sizing(&child);
            let stretches = if vertical {
                child_sizing.h == SizeSpec::Fill
            } else {
                child_sizing.v == SizeSpec::Fill
            };
            let stretch_by_align = layout.counter_align == CounterAlign::Stretch;
            if stretches || stretch_by_align {
                if let Some(size) = self.sizes.get_mut(child_id) {
                    if vertical {
                        size.w = inner_w;
                    } else {
                        size.h = inner_h;
                    }
                    if let Some(position) = self.positions.get_mut(child_id) {
                        if vertical {
                            position.x = padding.left;
                        } else {
                            position.y = padding.top;
                        }
                    }
                }
            }
        }

        size
    }

    fn place(
        &self,
        id: NodeId,
        node: &Node,
        parent_matrix: Mat2D,
        default_position: Point,
        size: Size,
        map: &mut LayoutMap,
    ) {
        let position = self.positions.get(&id).copied().unwrap_or(default_position);
        let transform = Transform {
            x: position.x,
            y: position.y,
            w: size.w,
            h: size.h,
            ..node.transform
        };
        let local_matrix = transform.matrix();
        let matrix = parent_matrix.mul(local_matrix);
        let world = matrix.apply_rect(Rect::new(0.0, 0.0, size.w, size.h));
        map.insert(
            id,
            NodeLayout {
                local: Rect::new(position.x, position.y, size.w, size.h),
                size,
                local_matrix,
                matrix,
                world,
            },
        );
        for child_id in node.children.iter().copied() {
            let Some(child) = self.page.nodes.get(&child_id) else {
                continue;
            };
            let child_size = self.sizes.get(&child_id).copied().unwrap_or(Size::zero());
            let default = Point::new(child.transform.x, child.transform.y);
            self.place(child_id, child, matrix, default, child_size, map);
        }
    }
}

/// One wrapped row or column of children.
struct Row {
    indices: Vec<usize>,
    /// Main-axis extent of the row.
    main: f64,
    /// Cross-axis extent of the row (max child extent).
    cross: f64,
}

struct Arrangement {
    rows: Vec<Row>,
}

fn arrange(
    direction: LayoutDirection,
    wrap: bool,
    gap: f64,
    main_limit: Option<f64>,
    sizes: &[Size],
) -> Arrangement {
    let vertical = matches!(direction, LayoutDirection::Vertical);
    let mut rows: Vec<Row> = Vec::new();
    let mut current = Row {
        indices: Vec::new(),
        main: 0.0,
        cross: 0.0,
    };
    for (index, size) in sizes.iter().enumerate() {
        let main = if vertical { size.h } else { size.w };
        let cross = if vertical { size.w } else { size.h };
        let would_overflow = match main_limit {
            Some(limit) if wrap && !current.indices.is_empty() => {
                current.main + gap + main > limit + 1e-9
            }
            _ => false,
        };
        if would_overflow {
            rows.push(std::mem::replace(
                &mut current,
                Row {
                    indices: Vec::new(),
                    main: 0.0,
                    cross: 0.0,
                },
            ));
        }
        if current.indices.is_empty() {
            current.main = main;
            current.cross = cross;
        } else {
            current.main += gap + main;
            current.cross = current.cross.max(cross);
        }
        current.indices.push(index);
    }
    if !current.indices.is_empty() || rows.is_empty() {
        rows.push(current);
    }
    Arrangement { rows }
}

fn is_layout_container(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::Frame | NodeKind::Group)
}

fn child_sizing(node: &Node) -> Sizing {
    node.layout
        .as_ref()
        .map(|layout| layout.sizing)
        .unwrap_or_default()
}

impl NodeLayout {
    /// Whether a page-space point is inside the node's resolved bounds.
    pub fn contains(&self, point: Point) -> bool {
        let Some(inverse) = self.matrix.inverse() else {
            return false;
        };
        let local = inverse.apply_point(point);
        local.x >= 0.0 && local.y >= 0.0 && local.x <= self.size.w && local.y <= self.size.h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_model::node::Node;
    use dp_model::page::Page;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn engine() -> MutexGuard<'static, LayoutEngine> {
        static ENGINE: OnceLock<Mutex<LayoutEngine>> = OnceLock::new();
        ENGINE
            .get_or_init(|| Mutex::new(LayoutEngine::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn rect(name: &str, x: f64, y: f64, w: f64, h: f64) -> Node {
        Node::rectangle(name, x, y, w, h)
    }

    fn add(page: &mut Page, child: Node, parent: Option<NodeId>) -> NodeId {
        let id = child.id;
        page.insert_node(child, parent, None).expect("insert");
        id
    }

    fn with_sizing(mut node: Node, sizing: Sizing) -> Node {
        let mut layout = node.layout.clone().unwrap_or_default();
        layout.sizing = sizing;
        node.layout = Some(layout);
        node
    }

    #[test]
    fn horizontal_hug_positions_children_with_gap() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::horizontal(8.0, 0.0);
        layout.sizing = Sizing::HUG;
        let frame = Node::frame("Row", 0.0, 0.0, 0.0, 0.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let a = add(&mut page, rect("a", 0.0, 0.0, 20.0, 10.0), Some(frame_id));
        let b = add(&mut page, rect("b", 0.0, 0.0, 20.0, 10.0), Some(frame_id));
        let c = add(&mut page, rect("c", 0.0, 0.0, 20.0, 10.0), Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(frame_id), Some(Size::new(76.0, 10.0)));
        assert_eq!(
            map.get(frame_id).unwrap().world,
            Rect::new(0.0, 0.0, 76.0, 10.0)
        );
        assert_eq!(map.get(a).unwrap().local.x, 0.0);
        assert_eq!(map.get(b).unwrap().local.x, 28.0);
        assert_eq!(map.get(c).unwrap().local.x, 56.0);
    }

    #[test]
    fn padding_offsets_children() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::horizontal(4.0, 10.0);
        layout.sizing = Sizing::HUG;
        let frame = Node::frame("Row", 0.0, 0.0, 0.0, 0.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let a = add(&mut page, rect("a", 0.0, 0.0, 20.0, 10.0), Some(frame_id));
        let b = add(&mut page, rect("b", 0.0, 0.0, 20.0, 10.0), Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(frame_id), Some(Size::new(64.0, 30.0)));
        assert_eq!(map.get(a).unwrap().local, Rect::new(10.0, 10.0, 20.0, 10.0));
        assert_eq!(map.get(b).unwrap().local, Rect::new(34.0, 10.0, 20.0, 10.0));
    }

    #[test]
    fn vertical_centered_children() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::vertical(0.0, 0.0);
        layout.counter_align = CounterAlign::Center;
        let frame = Node::frame("Column", 0.0, 0.0, 100.0, 100.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let a = add(&mut page, rect("a", 0.0, 0.0, 20.0, 10.0), Some(frame_id));
        let b = add(&mut page, rect("b", 0.0, 0.0, 20.0, 10.0), Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(frame_id), Some(Size::new(100.0, 100.0)));
        assert_eq!(map.get(a).unwrap().local, Rect::new(40.0, 0.0, 20.0, 10.0));
        assert_eq!(map.get(b).unwrap().local, Rect::new(40.0, 10.0, 20.0, 10.0));
    }

    #[test]
    fn vertical_fill_child_takes_inner_width() {
        let mut page = Page::new("Page");
        let layout = AutoLayout::vertical(0.0, 10.0);
        let frame = Node::frame("Column", 0.0, 0.0, 200.0, 100.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let child = with_sizing(
            rect("fill", 0.0, 0.0, 20.0, 20.0),
            Sizing {
                h: SizeSpec::Fill,
                v: SizeSpec::Fixed,
            },
        );
        let child_id = add(&mut page, child, Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(child_id).unwrap().w, 180.0);
        assert_eq!(map.size(child_id).unwrap().h, 20.0);
        assert_eq!(map.get(child_id).unwrap().local.x, 10.0);
        assert_eq!(map.get(child_id).unwrap().local.y, 10.0);
    }

    #[test]
    fn nested_layouts_resolve_bottom_up() {
        let mut page = Page::new("Page");
        let mut outer_layout = AutoLayout::horizontal(0.0, 0.0);
        outer_layout.sizing = Sizing::HUG;
        let outer = Node::frame("Outer", 0.0, 0.0, 0.0, 0.0).with_layout(outer_layout);
        let outer_id = add(&mut page, outer, None);

        let mut inner_layout = AutoLayout::vertical(4.0, 0.0);
        inner_layout.sizing = Sizing::HUG;
        let inner = Node::frame("Inner", 0.0, 0.0, 0.0, 0.0).with_layout(inner_layout);
        let inner_id = add(&mut page, inner, Some(outer_id));
        let a = add(&mut page, rect("a", 0.0, 0.0, 10.0, 10.0), Some(inner_id));
        let b = add(&mut page, rect("b", 0.0, 0.0, 10.0, 10.0), Some(inner_id));
        let sibling = add(
            &mut page,
            rect("sibling", 0.0, 0.0, 20.0, 20.0),
            Some(outer_id),
        );

        let map = engine().compute_page(&page);
        assert_eq!(map.size(inner_id), Some(Size::new(10.0, 24.0)));
        assert_eq!(map.size(outer_id), Some(Size::new(30.0, 24.0)));
        assert_eq!(map.get(a).unwrap().local, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(map.get(b).unwrap().local, Rect::new(0.0, 14.0, 10.0, 10.0));
        assert_eq!(map.get(inner_id).unwrap().local.x, 0.0);
        assert_eq!(map.get(sibling).unwrap().local.x, 10.0);
    }

    #[test]
    fn wrapping_breaks_into_rows() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::horizontal(0.0, 0.0);
        layout.wrap = true;
        layout.sizing = Sizing {
            h: SizeSpec::Fixed,
            v: SizeSpec::Hug,
        };
        let frame = Node::frame("Wrap", 0.0, 0.0, 100.0, 0.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let a = add(&mut page, rect("a", 0.0, 0.0, 40.0, 20.0), Some(frame_id));
        let b = add(&mut page, rect("b", 0.0, 0.0, 40.0, 20.0), Some(frame_id));
        let c = add(&mut page, rect("c", 0.0, 0.0, 40.0, 20.0), Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(frame_id), Some(Size::new(100.0, 40.0)));
        assert_eq!(map.get(a).unwrap().local, Rect::new(0.0, 0.0, 40.0, 20.0));
        assert_eq!(map.get(b).unwrap().local, Rect::new(40.0, 0.0, 40.0, 20.0));
        assert_eq!(map.get(c).unwrap().local, Rect::new(0.0, 20.0, 40.0, 20.0));
    }

    #[test]
    fn text_drives_hug_size() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::horizontal(0.0, 0.0);
        layout.sizing = Sizing::HUG;
        let frame = Node::frame("Row", 0.0, 0.0, 0.0, 0.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let text = Node::text("Label", "Hello design", 0.0, 0.0);
        let text_id = add(&mut page, text, Some(frame_id));

        let map = engine().compute_page(&page);
        let text_size = map.size(text_id).unwrap();
        assert!(text_size.w > 0.0);
        assert!(text_size.h > 0.0);
        assert_eq!(map.size(frame_id), Some(text_size));
        assert_eq!(map.get(text_id).unwrap().local.x, 0.0);
    }

    #[test]
    fn min_size_clamps_hug() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::vertical(0.0, 0.0);
        layout.sizing = Sizing::HUG;
        layout.min_size = Some(Size::new(100.0, 50.0));
        let frame = Node::frame("Min", 0.0, 0.0, 0.0, 0.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        add(&mut page, rect("a", 0.0, 0.0, 10.0, 10.0), Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(frame_id), Some(Size::new(100.0, 50.0)));
    }

    #[test]
    fn group_hugs_union_of_children() {
        let mut page = Page::new("Page");
        let group = Node::new(NodeKind::Group, "Group", Transform::new(0.0, 0.0, 0.0, 0.0));
        let group_id = add(&mut page, group, None);
        let a = add(&mut page, rect("a", 10.0, 20.0, 30.0, 30.0), Some(group_id));
        let b = add(&mut page, rect("b", 50.0, 0.0, 10.0, 10.0), Some(group_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(group_id), Some(Size::new(50.0, 50.0)));
        assert_eq!(map.get(a).unwrap().local, Rect::new(0.0, 20.0, 30.0, 30.0));
        assert_eq!(map.get(b).unwrap().local, Rect::new(40.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn world_matrix_composes_parent_offsets() {
        let mut page = Page::new("Page");
        let frame = Node::frame("Frame", 100.0, 50.0, 400.0, 300.0);
        let frame_id = add(&mut page, frame, None);
        let child = add(
            &mut page,
            rect("child", 10.0, 10.0, 20.0, 20.0),
            Some(frame_id),
        );

        let map = engine().compute_page(&page);
        assert_eq!(
            map.get(child).unwrap().world,
            Rect::new(110.0, 60.0, 20.0, 20.0)
        );
        assert!(map.get(child).unwrap().contains(Point::new(115.0, 65.0)));
        assert!(!map.get(child).unwrap().contains(Point::new(105.0, 65.0)));
    }

    #[test]
    fn hidden_children_are_excluded_from_flow() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::vertical(0.0, 0.0);
        layout.sizing = Sizing::HUG;
        let frame = Node::frame("Column", 0.0, 0.0, 0.0, 0.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let first = add(
            &mut page,
            rect("first", 0.0, 0.0, 10.0, 10.0),
            Some(frame_id),
        );
        let mut hidden = rect("hidden", 0.0, 0.0, 10.0, 30.0);
        hidden.visible = false;
        add(&mut page, hidden, Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.size(frame_id), Some(Size::new(10.0, 10.0)));
        assert_eq!(map.get(first).unwrap().local.y, 0.0);
    }

    #[test]
    fn item_reverse_reverses_order() {
        let mut page = Page::new("Page");
        let mut layout = AutoLayout::horizontal(2.0, 0.0);
        layout.sizing = Sizing::HUG;
        layout.item_reverse = true;
        let frame = Node::frame("Row", 0.0, 0.0, 0.0, 0.0).with_layout(layout);
        let frame_id = add(&mut page, frame, None);
        let a = add(&mut page, rect("a", 0.0, 0.0, 10.0, 10.0), Some(frame_id));
        let b = add(&mut page, rect("b", 0.0, 0.0, 10.0, 10.0), Some(frame_id));

        let map = engine().compute_page(&page);
        assert_eq!(map.get(b).unwrap().local.x, 0.0);
        assert_eq!(map.get(a).unwrap().local.x, 12.0);
    }
}
