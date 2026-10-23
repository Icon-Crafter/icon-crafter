//! Pointer, wheel and keyboard handling.
//!
//! Input arrives in CSS pixels (Tauri window coordinates) and is converted to
//! world space through the viewport. Drag gestures mutate the document directly
//! (no history entries) and commit a single undoable transaction on release;
//! this keeps dragging responsive and undo meaningful.

use dp_geom::{Point, Rect, SnapOptions};
use dp_model::{Edit, Node, NodeId, NodeKind, PageId, Transaction, Transform};
use serde::{Deserialize, Serialize};

use crate::{
    CLICK_TOLERANCE_CSS, EditorError, EditorState, NUDGE_LARGE_STEP, NUDGE_STEP, ToolId, commands,
};

/// Keyboard modifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifiers {
    /// Shift.
    #[serde(default)]
    pub shift: bool,
    /// Alt/Option.
    #[serde(default)]
    pub alt: bool,
    /// Control.
    #[serde(default)]
    pub ctrl: bool,
    /// Command/Meta.
    #[serde(default)]
    pub meta: bool,
}

impl Modifiers {
    /// Primary modifier (Cmd on macOS, Ctrl elsewhere) — the UI sends both.
    pub fn primary(self) -> bool {
        self.ctrl || self.meta
    }
}

/// Pointer event kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerKind {
    /// Button pressed.
    Down,
    /// Pointer moved.
    Move,
    /// Button released.
    Up,
    /// Double click.
    DoubleClick,
    /// Pointer left the canvas.
    Leave,
}

/// One pointer event in CSS pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointerInput {
    /// Event kind.
    pub kind: PointerKind,
    /// Button index (0 left, 1 middle, 2 right).
    #[serde(default)]
    pub button: u8,
    /// CSS x.
    pub screen_x: f64,
    /// CSS y.
    pub screen_y: f64,
    /// Modifiers.
    #[serde(default)]
    pub modifiers: Modifiers,
    /// Pen pressure 0..1.
    #[serde(default = "default_pressure")]
    pub pressure: f64,
    /// Milliseconds timestamp.
    #[serde(default)]
    pub timestamp_ms: u64,
}

fn default_pressure() -> f64 {
    0.5
}

/// Wheel/trackpad event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WheelInput {
    /// Horizontal delta.
    pub delta_x: f64,
    /// Vertical delta.
    pub delta_y: f64,
    /// CSS x.
    pub screen_x: f64,
    /// CSS y.
    pub screen_y: f64,
    /// Modifiers (ctrl/meta means zoom).
    #[serde(default)]
    pub modifiers: Modifiers,
}

/// Key event kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyKind {
    /// Key pressed.
    Down,
    /// Key released.
    Up,
}

/// Keyboard event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyInput {
    /// Kind.
    pub kind: KeyKind,
    /// Logical key (e.g. `Enter`, `Delete`, `z`).
    pub key: String,
    /// Physical code (unused today).
    #[serde(default)]
    pub code: String,
    /// Modifiers.
    #[serde(default)]
    pub modifiers: Modifiers,
    /// Auto-repeat.
    #[serde(default)]
    pub repeat: bool,
}

/// Which resize handle a drag started on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// North-west.
    NorthWest,
    /// North.
    North,
    /// North-east.
    NorthEast,
    /// East.
    East,
    /// South-east.
    SouthEast,
    /// South.
    South,
    /// South-west.
    SouthWest,
    /// West.
    West,
}

impl HandleKind {
    /// Parses a handle kind from the overlay string.
    pub fn parse(value: &str) -> Option<HandleKind> {
        Some(match value {
            "nw" => HandleKind::NorthWest,
            "n" => HandleKind::North,
            "ne" => HandleKind::NorthEast,
            "e" => HandleKind::East,
            "se" => HandleKind::SouthEast,
            "s" => HandleKind::South,
            "sw" => HandleKind::SouthWest,
            "w" => HandleKind::West,
            _ => return None,
        })
    }

    fn positions(self, rect: Rect) -> (Point, Point) {
        let left = rect.x;
        let right = rect.right();
        let top = rect.y;
        let bottom = rect.bottom();
        let cx = rect.x + rect.w * 0.5;
        let cy = rect.y + rect.h * 0.5;
        let fixed = match self {
            HandleKind::NorthWest => Point::new(right, bottom),
            HandleKind::North => Point::new(cx, bottom),
            HandleKind::NorthEast => Point::new(left, bottom),
            HandleKind::East => Point::new(left, cy),
            HandleKind::SouthEast => Point::new(left, top),
            HandleKind::South => Point::new(cx, top),
            HandleKind::SouthWest => Point::new(right, top),
            HandleKind::West => Point::new(right, cy),
        };
        let moving = match self {
            HandleKind::NorthWest => Point::new(left, top),
            HandleKind::North => Point::new(cx, top),
            HandleKind::NorthEast => Point::new(right, top),
            HandleKind::East => Point::new(right, cy),
            HandleKind::SouthEast => Point::new(right, bottom),
            HandleKind::South => Point::new(cx, bottom),
            HandleKind::SouthWest => Point::new(left, bottom),
            HandleKind::West => Point::new(left, cy),
        };
        (fixed, moving)
    }
}

/// Active drag gesture.
#[derive(Debug, Clone)]
pub(crate) enum DragState {
    /// Panning the viewport.
    Pan { last_screen: Point },
    /// Moving the selection.
    Moving {
        page: PageId,
        start_world: Point,
        before: Vec<Node>,
        snap_targets: Vec<Rect>,
        moved: bool,
    },
    /// Marquee selection.
    Marquee { start_world: Point, additive: bool },
    /// Creating a shape by dragging.
    Creating {
        page: PageId,
        tool: ToolId,
        node_id: NodeId,
        start_world: Point,
        moved: bool,
    },
    /// Resizing the selection through a handle.
    Resizing {
        page: PageId,
        handle: HandleKind,
        original_bounds: Rect,
        before: Vec<Node>,
    },
}

impl EditorState {
    /// Handles a pointer event.
    pub fn pointer(&mut self, input: &PointerInput) -> Result<(), EditorError> {
        if self.playback.is_some() {
            self.prototype_pointer(input);
            return Ok(());
        }
        match input.kind {
            PointerKind::Down => self.pointer_down(input),
            PointerKind::Move => self.pointer_move(input),
            PointerKind::Up => self.pointer_up(input),
            PointerKind::DoubleClick => self.pointer_double_click(input),
            PointerKind::Leave => {
                self.hover = None;
                Ok(())
            }
        }
    }

    /// Pointer handling while a prototype presentation is active.
    fn prototype_pointer(&mut self, input: &PointerInput) {
        match input.kind {
            PointerKind::Down => {
                self.proto_pointer_down = Some(Point::new(input.screen_x, input.screen_y));
            }
            PointerKind::Up => {
                let Some(down) = self.proto_pointer_down.take() else {
                    return;
                };
                let dx = input.screen_x - down.x;
                let dy = input.screen_y - down.y;
                if dx * dx + dy * dy > 16.0 {
                    return;
                }
                let world = self
                    .viewport
                    .css_to_world(Point::new(input.screen_x, input.screen_y));
                match self.prototype_click_world(world) {
                    Ok(true) => {}
                    Ok(false) => {
                        self.status = "No prototype hotspot here".to_owned();
                    }
                    Err(error) => self.errors.push(error.to_string()),
                }
            }
            _ => {}
        }
    }

    fn pointer_down(&mut self, input: &PointerInput) -> Result<(), EditorError> {
        self.snap_lines.clear();
        let world = self
            .viewport
            .css_to_world(Point::new(input.screen_x, input.screen_y));

        if self.text_edit.is_some() {
            let editing = self.text_edit.as_ref().map(|edit| edit.node_id);
            let clicked = self.hit_test(world);
            if clicked != editing {
                self.text_edit_commit()?;
            }
        }

        if input.button == 1 || self.tool == ToolId::Hand {
            self.drag = Some(DragState::Pan {
                last_screen: Point::new(input.screen_x, input.screen_y),
            });
            return Ok(());
        }

        match self.tool {
            ToolId::Move | ToolId::Node | ToolId::Slice => {
                if let Some(handle) = self.handle_at(input.screen_x, input.screen_y) {
                    self.begin_resize(handle)?;
                    return Ok(());
                }
                match self.hit_test(world) {
                    Some(id) => {
                        if input.modifiers.shift {
                            self.select(id, true);
                        } else if !self.selection.contains(&id) {
                            self.select(id, false);
                        }
                        self.begin_move(world);
                    }
                    None => {
                        if !input.modifiers.shift {
                            self.clear_selection();
                        }
                        self.drag = Some(DragState::Marquee {
                            start_world: world,
                            additive: input.modifiers.shift,
                        });
                    }
                }
            }
            ToolId::Text => {
                self.create_text_node(world)?;
            }
            ToolId::Pen | ToolId::Image | ToolId::Comment | ToolId::Connector => {
                self.status = format!("{} tool is not implemented yet", self.tool.hint());
            }
            tool => {
                if let Some(kind) = tool.shape_kind() {
                    self.begin_create(world, kind)?;
                }
            }
        }
        Ok(())
    }

    fn pointer_move(&mut self, input: &PointerInput) -> Result<(), EditorError> {
        let world = self
            .viewport
            .css_to_world(Point::new(input.screen_x, input.screen_y));

        let Some(drag) = self.drag.take() else {
            if self.tool == ToolId::Move && self.text_edit.is_none() {
                self.hover = self.hit_test(world);
            }
            return Ok(());
        };

        match drag {
            DragState::Pan { last_screen } => {
                let dx = input.screen_x - last_screen.x;
                let dy = input.screen_y - last_screen.y;
                self.viewport.pan_css(dx, dy);
                self.drag = Some(DragState::Pan {
                    last_screen: Point::new(input.screen_x, input.screen_y),
                });
            }
            DragState::Marquee {
                start_world,
                additive,
            } => {
                let rect = Rect::from_points(start_world, world);
                self.marquee = Some(rect);
                let mut found = self.nodes_in_rect(rect);
                if additive {
                    for id in found {
                        if !self.selection.contains(&id) {
                            self.selection.push(id);
                        }
                    }
                } else {
                    found.sort_by_key(|id| self.z_index(*id).unwrap_or(0));
                    self.selection = found;
                }
                self.drag = Some(DragState::Marquee {
                    start_world,
                    additive,
                });
            }
            DragState::Moving {
                page,
                start_world,
                before,
                snap_targets,
                moved: _,
            } => {
                let raw_delta = Point::new(world.x - start_world.x, world.y - start_world.y);
                let bounds = snapshot_bounds(&before);
                let delta = if let Some(bounds) = bounds {
                    let moving = Rect::new(
                        bounds.x + raw_delta.x,
                        bounds.y + raw_delta.y,
                        bounds.w,
                        bounds.h,
                    );
                    let options = SnapOptions {
                        snap_grid: input.modifiers.ctrl,
                        snap_guides: false,
                        ..SnapOptions::default()
                    };
                    let snap = dp_geom::snap::snap_rect(moving, &snap_targets, &[], &options);
                    self.snap_lines = snap.lines.clone();
                    Point::new(raw_delta.x + snap.delta.x, raw_delta.y + snap.delta.y)
                } else {
                    raw_delta
                };
                self.apply_move_delta(page, &before, delta);
                self.drag = Some(DragState::Moving {
                    page,
                    start_world,
                    before,
                    snap_targets,
                    moved: true,
                });
            }
            DragState::Creating {
                page,
                tool,
                node_id,
                start_world,
                moved: _,
            } => {
                let rect = drag_rect(start_world, world, tool);
                if rect.w > 0.0 || rect.h > 0.0 {
                    if let Some(page_mut) = self.document.page_mut(page)
                        && let Some(node) = page_mut.nodes.get_mut(&node_id)
                    {
                        node.transform.x = rect.x;
                        node.transform.y = rect.y;
                        node.transform.w = rect.w;
                        node.transform.h = rect.h;
                    }
                    self.invalidate_layout();
                }
                self.drag = Some(DragState::Creating {
                    page,
                    tool,
                    node_id,
                    start_world,
                    moved: true,
                });
            }
            DragState::Resizing {
                page,
                handle,
                original_bounds,
                before,
            } => {
                let (fixed, _) = handle.positions(original_bounds);
                let mut min = Point::new(world.x.min(fixed.x), world.y.min(fixed.y));
                let mut max = Point::new(world.x.max(fixed.x), world.y.max(fixed.y));
                if input.modifiers.shift {
                    let aspect = original_bounds.w / original_bounds.h.max(1e-6);
                    let width = max.x - min.x;
                    let height = max.y - min.y;
                    if width / aspect > height {
                        max.y = min.y + width / aspect;
                    } else {
                        max.x = min.x + height * aspect;
                    }
                }
                if input.modifiers.alt {
                    let center = original_bounds.center();
                    min = Point::new(2.0 * center.x - max.x, 2.0 * center.y - max.y);
                }
                let target = Rect::from_points(min, max);
                if original_bounds.w > 1e-6 && original_bounds.h > 1e-6 {
                    let sx = target.w / original_bounds.w;
                    let sy = target.h / original_bounds.h;
                    for original in &before {
                        if let Some(page_mut) = self.document.page_mut(page)
                            && let Some(node) = page_mut.nodes.get_mut(&original.id)
                        {
                            let local_x = original.transform.x - original_bounds.x;
                            let local_y = original.transform.y - original_bounds.y;
                            node.transform.x = target.x + local_x * sx;
                            node.transform.y = target.y + local_y * sy;
                            node.transform.w = (original.transform.w * sx).max(1.0);
                            node.transform.h = (original.transform.h * sy).max(1.0);
                        }
                    }
                    self.invalidate_layout();
                }
                self.drag = Some(DragState::Resizing {
                    page,
                    handle,
                    original_bounds,
                    before,
                });
            }
        }
        Ok(())
    }

    fn pointer_up(&mut self, input: &PointerInput) -> Result<(), EditorError> {
        let Some(drag) = self.drag.take() else {
            return Ok(());
        };
        match drag {
            DragState::Pan { .. } => {}
            DragState::Marquee { .. } => {
                self.marquee = None;
            }
            DragState::Moving {
                page,
                before,
                moved,
                ..
            } => {
                self.snap_lines.clear();
                if moved {
                    self.commit_edits(page, &before, "Move")?;
                }
            }
            DragState::Creating {
                page,
                node_id,
                moved,
                ..
            } => {
                let mut final_node = self
                    .document
                    .node(node_id)
                    .cloned()
                    .unwrap_or_else(|| Node::rectangle("Rectangle", 0.0, 0.0, 1.0, 1.0));
                let too_small =
                    final_node.transform.w.abs() < 2.0 && final_node.transform.h.abs() < 2.0;
                if !moved || too_small {
                    // Clicking with a shape tool creates a Figma-style default
                    // layer instead of a degenerate zero-size node.
                    if final_node.kind == NodeKind::Line {
                        final_node.transform.w = 100.0;
                        final_node.transform.h = 0.0;
                    } else {
                        final_node.transform.w = 100.0;
                        final_node.transform.h = 100.0;
                    }
                    self.status = format!("Created {} (default size)", final_node.name);
                }
                if let Some(page_mut) = self.document.page_mut(page) {
                    page_mut.remove_node(node_id);
                }
                let mut transaction = Transaction::new("Create layer");
                transaction.push(Edit::InsertNode {
                    page,
                    node: final_node.clone(),
                    parent: final_node.parent,
                    index: usize::MAX,
                });
                self.apply(transaction)?;
                self.selection = vec![final_node.id];
                if too_small || !moved {
                    // status already explains the default size
                } else {
                    self.status = format!("Created {}", final_node.name);
                }
                self.invalidate_layout();
            }
            DragState::Resizing { page, before, .. } => {
                self.commit_edits(page, &before, "Resize")?;
            }
        }
        let _ = input;
        Ok(())
    }

    fn pointer_double_click(&mut self, input: &PointerInput) -> Result<(), EditorError> {
        let world = self
            .viewport
            .css_to_world(Point::new(input.screen_x, input.screen_y));
        let Some(id) = self.hit_test(world) else {
            return Ok(());
        };
        let is_text = self
            .document
            .node(id)
            .map(|node| node.kind == NodeKind::Text)
            .unwrap_or(false);
        if is_text {
            self.text_edit_begin(id)?;
        }
        Ok(())
    }

    /// Handles a wheel event: ctrl/meta zooms at the cursor, otherwise pans.
    pub fn wheel(&mut self, input: &WheelInput) {
        if input.modifiers.primary() {
            let anchor = Point::new(input.screen_x, input.screen_y);
            let factor = (-input.delta_y * 0.0015).exp();
            let new_zoom =
                (self.viewport.zoom * factor).clamp(dp_render::MIN_ZOOM, dp_render::MAX_ZOOM);
            self.viewport.zoom_at_css(anchor, new_zoom);
            return;
        }
        let zoom = self.viewport.zoom.max(1e-6);
        self.viewport.x += input.delta_x / zoom;
        self.viewport.y += input.delta_y / zoom;
    }

    /// Handles a key event.
    pub fn key(&mut self, input: &KeyInput) -> Result<(), EditorError> {
        if input.kind != KeyKind::Down {
            return Ok(());
        }
        if self.playback.is_some() {
            match input.key.as_str() {
                "Escape" => self.prototype_stop(),
                "Backspace" | "ArrowLeft" => {
                    if let Err(error) = self.prototype_back() {
                        self.errors.push(error.to_string());
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        let modifiers = input.modifiers;
        let key = input.key.as_str();
        if key == "Escape" {
            if self.text_edit_cancel() {
                return Ok(());
            }
            if self.drag.take().is_some() {
                self.marquee = None;
                self.snap_lines.clear();
                return Ok(());
            }
            self.clear_selection();
            return Ok(());
        }
        if self.text_edit.is_some() {
            // Text editing consumes Enter and ignores editing shortcuts.
            if key == "Enter" {
                self.text_edit_commit()?;
            }
            return Ok(());
        }
        if modifiers.primary() {
            match key.to_ascii_lowercase().as_str() {
                "z" if modifiers.shift => {
                    self.redo()?;
                }
                "z" => {
                    self.undo()?;
                }
                "y" => {
                    self.redo()?;
                }
                "d" => {
                    commands::duplicate_selection(self)?;
                }
                "g" if modifiers.shift => {
                    commands::ungroup_selection(self)?;
                }
                "g" => {
                    commands::group_selection(self)?;
                }
                "a" => {
                    commands::select_all(self);
                }
                "0" => {
                    self.zoom_100();
                }
                "1" => {
                    self.fit_to_content();
                }
                "=" | "+" => {
                    self.zoom_in();
                }
                "-" => {
                    self.zoom_out();
                }
                "c" => {
                    commands::copy_selection(self);
                }
                "x" => {
                    commands::copy_selection(self);
                    commands::delete_selection(self)?;
                }
                "v" => {
                    commands::paste_clipboard(self)?;
                }
                _ => {}
            }
            return Ok(());
        }
        match key {
            "Delete" | "Backspace" => {
                commands::delete_selection(self)?;
            }
            "Enter" => {
                if let Some(id) = self.selection.first().copied() {
                    let is_text = self
                        .document
                        .node(id)
                        .map(|node| node.kind == NodeKind::Text)
                        .unwrap_or(false);
                    if is_text {
                        self.text_edit_begin(id)?;
                    }
                }
            }
            "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" => {
                let step = if modifiers.shift {
                    NUDGE_LARGE_STEP
                } else {
                    NUDGE_STEP
                };
                let (dx, dy) = match key {
                    "ArrowLeft" => (-step, 0.0),
                    "ArrowRight" => (step, 0.0),
                    "ArrowUp" => (0.0, -step),
                    _ => (0.0, step),
                };
                let ids = self.selection.clone();
                commands::nudge_selection(self, dx, dy, &ids)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Hit tests a world point against visible layers (topmost first).
    pub fn hit_test(&mut self, world: Point) -> Option<NodeId> {
        self.ensure_layout();
        let layout = self.layout_cache.as_ref()?;
        let order: Vec<NodeId> = layout.order().to_vec();
        let candidates = match self.hit_index.as_ref() {
            Some(index) => index.query_point(world),
            None => Vec::new(),
        };
        let mut best: Option<(usize, NodeId)> = None;
        for id in candidates {
            let Some(node) = self.document.node(id) else {
                continue;
            };
            if !node.visible || node.locked {
                continue;
            }
            let Some(position) = order.iter().position(|candidate| *candidate == id) else {
                continue;
            };
            if best.is_none_or(|(best_position, _)| position > best_position) {
                best = Some((position, id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Returns ids whose world bounds intersect `rect`.
    pub fn nodes_in_rect(&mut self, rect: Rect) -> Vec<NodeId> {
        self.ensure_layout();
        let order: Vec<NodeId> = match self.layout_cache.as_ref() {
            Some(layout) => layout.order().to_vec(),
            None => return Vec::new(),
        };
        let candidates = match self.hit_index.as_ref() {
            Some(index) => index.query(rect),
            None => Vec::new(),
        };
        let mut found: Vec<NodeId> = Vec::new();
        for id in candidates {
            let Some(node) = self.document.node(id) else {
                continue;
            };
            if !node.visible || node.locked {
                continue;
            }
            if let Some(layout) = self.layout_cache.as_ref()
                && let Some(node_layout) = layout.get(id)
                && rect.contains_rect(node_layout.world)
            {
                found.push(id);
            }
        }
        found.sort_by_key(|id| {
            order
                .iter()
                .position(|candidate| candidate == id)
                .unwrap_or(usize::MAX)
        });
        found
    }

    fn z_index(&mut self, id: NodeId) -> Option<usize> {
        self.ensure_layout();
        let layout = self.layout_cache.as_ref()?;
        layout.order().iter().position(|candidate| *candidate == id)
    }

    fn begin_move(&mut self, world: Point) {
        let page = self.document.active_page_id();
        let selected: Vec<NodeId> = self.selection.clone();
        let mut before = Vec::new();
        for id in &selected {
            if let Some(node) = self.document.node(*id)
                && !node.locked
                && node.visible
            {
                before.push(node.clone());
            }
        }
        let mut snap_targets = Vec::new();
        if !before.is_empty() {
            let selected_ids: Vec<NodeId> = before.iter().map(|node| node.id).collect();
            self.ensure_layout();
            if let Some(layout) = self.layout_cache.as_ref() {
                let mut ids = Vec::new();
                let page_ref = self.document.active_page();
                for root in page_ref.roots.clone() {
                    collect_ids(page_ref, root, &mut ids);
                }
                for id in ids {
                    if selected_ids.contains(&id) {
                        continue;
                    }
                    if let Some(node) = self.document.node(id)
                        && !node.visible
                    {
                        continue;
                    }
                    if let Some(node_layout) = layout.get(id) {
                        snap_targets.push(node_layout.world);
                    }
                }
            }
        }
        self.drag = Some(DragState::Moving {
            page,
            start_world: world,
            before,
            snap_targets,
            moved: false,
        });
    }

    fn begin_resize(&mut self, handle: HandleKind) -> Result<(), EditorError> {
        let Some(bounds) = self.selection_bounds() else {
            return Ok(());
        };
        let page = self.document.active_page_id();
        let mut before = Vec::new();
        for id in self.selection.clone() {
            if let Some(node) = self.document.node(id)
                && !node.locked
            {
                before.push(node.clone());
            }
        }
        self.drag = Some(DragState::Resizing {
            page,
            handle,
            original_bounds: bounds,
            before,
        });
        Ok(())
    }

    fn handle_at(&mut self, screen_x: f64, screen_y: f64) -> Option<HandleKind> {
        let bounds = self.selection_bounds()?;
        let zoom = self.viewport.zoom;
        let screen = Rect::new(
            (bounds.x - self.viewport.x) * zoom,
            (bounds.y - self.viewport.y) * zoom,
            bounds.w * zoom,
            bounds.h * zoom,
        );
        let slop = (CLICK_TOLERANCE_CSS * 2.0).max(6.0);
        let matches =
            |px: f64, py: f64| (screen_x - px).abs() <= slop && (screen_y - py).abs() <= slop;
        let left = screen.x;
        let right = screen.right();
        let top = screen.y;
        let bottom = screen.bottom();
        let cx = screen.x + screen.w * 0.5;
        let cy = screen.y + screen.h * 0.5;
        let candidates = [
            (HandleKind::NorthWest, left, top),
            (HandleKind::North, cx, top),
            (HandleKind::NorthEast, right, top),
            (HandleKind::East, right, cy),
            (HandleKind::SouthEast, right, bottom),
            (HandleKind::South, cx, bottom),
            (HandleKind::SouthWest, left, bottom),
            (HandleKind::West, left, cy),
        ];
        candidates
            .into_iter()
            .find(|(_, x, y)| matches(*x, *y))
            .map(|(kind, _, _)| kind)
    }

    fn begin_create(&mut self, world: Point, kind: NodeKind) -> Result<(), EditorError> {
        let page = self.document.active_page_id();
        let name = match kind {
            NodeKind::Frame => "Frame",
            NodeKind::Rectangle => "Rectangle",
            NodeKind::Ellipse => "Ellipse",
            NodeKind::Line => "Line",
            NodeKind::Polygon => "Polygon",
            NodeKind::Star => "Star",
            _ => "Layer",
        };
        let mut node = Node {
            kind,
            name: name.to_owned(),
            ..Node::rectangle(name, world.x, world.y, 1.0, 1.0)
        };
        // New layers must be visible immediately: Figma-style defaults so a
        // freshly drawn shape never looks like nothing happened.
        node.style = match kind {
            NodeKind::Frame => dp_model::NodeStyle::filled(dp_model::Color::WHITE),
            NodeKind::Line => dp_model::NodeStyle {
                strokes: vec![dp_model::Stroke::solid(dp_model::Color::BLACK)],
                ..dp_model::NodeStyle::default()
            },
            _ => dp_model::NodeStyle::filled(
                dp_model::Color::from_hex("#d9d9d9").unwrap_or(dp_model::Color::BLACK),
            ),
        };
        let node_id = node.id;
        if let Some(page_mut) = self.document.page_mut(page) {
            page_mut.insert_node(node, None, None)?;
        }
        self.selection = vec![node_id];
        self.invalidate_layout();
        self.drag = Some(DragState::Creating {
            page,
            tool: self.tool,
            node_id,
            start_world: world,
            moved: false,
        });
        Ok(())
    }

    fn create_text_node(&mut self, world: Point) -> Result<(), EditorError> {
        let page = self.document.active_page_id();
        let mut node = Node::text("Text", "Text", world.x, world.y);
        // Text needs an explicit colour; the model has no implicit text paint.
        if let Some(text) = node.text.as_mut() {
            text.style.fills = vec![dp_model::Fill::solid(
                dp_model::Color::from_hex("#1f2430").unwrap_or(dp_model::Color::BLACK),
            )];
        }
        let node_id = node.id;
        if let Some(page_mut) = self.document.page_mut(page) {
            page_mut.insert_node(node.clone(), None, None)?;
        }
        let mut transaction = Transaction::new("Create text");
        if let Some(page_mut) = self.document.page_mut(page) {
            page_mut.remove_node(node_id);
        }
        transaction.push(Edit::InsertNode {
            page,
            node,
            parent: None,
            index: usize::MAX,
        });
        self.apply(transaction)?;
        self.selection = vec![node_id];
        self.text_edit_begin(node_id)?;
        Ok(())
    }

    fn apply_move_delta(&mut self, page: PageId, before: &[Node], delta: Point) {
        if let Some(page_mut) = self.document.page_mut(page) {
            for original in before {
                if let Some(node) = page_mut.nodes.get_mut(&original.id) {
                    node.transform.x = original.transform.x + delta.x;
                    node.transform.y = original.transform.y + delta.y;
                }
            }
        }
        self.invalidate_layout();
    }

    fn commit_edits(
        &mut self,
        page: PageId,
        before: &[Node],
        label: &str,
    ) -> Result<(), EditorError> {
        let mut transaction = Transaction::new(label.to_owned());
        for original in before {
            let Some(after) = self.document.node(original.id).cloned() else {
                continue;
            };
            if after.transform != original.transform
                || after.style != original.style
                || after.opacity != original.opacity
            {
                transaction.push(Edit::SetNode {
                    page,
                    before: original.clone(),
                    after,
                });
            }
        }
        if transaction.is_empty() {
            return Ok(());
        }
        self.apply(transaction)?;
        Ok(())
    }

    fn ensure_layout(&mut self) {
        if self.layout_cache.is_none() {
            let map = self.layout_engine.compute_page(self.document.active_page());
            self.layout_cache = Some(map);
        }
    }
}

fn collect_ids(page: &dp_model::Page, id: NodeId, out: &mut Vec<NodeId>) {
    let Some(node) = page.get(id) else {
        return;
    };
    out.push(id);
    for child in &node.children {
        collect_ids(page, *child, out);
    }
}

fn snapshot_bounds(nodes: &[Node]) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    for node in nodes {
        let rect = Rect::new(
            node.transform.x,
            node.transform.y,
            node.transform.w,
            node.transform.h,
        );
        bounds = Some(match bounds {
            Some(current) => current.union(rect),
            None => rect,
        });
    }
    bounds
}

fn drag_rect(start: Point, current: Point, tool: ToolId) -> Rect {
    if tool == ToolId::Line {
        let x = start.x.min(current.x);
        let y = start.y.min(current.y);
        return Rect::new(
            x,
            y,
            (current.x - start.x).abs().max(1.0),
            (current.y - start.y).abs().max(1.0),
        );
    }
    Rect::from_points(start, current)
}

/// Unused transform helper retained for the frame/auto-layout integration.
#[allow(dead_code)]
fn transform_with(node: &Node, x: f64, y: f64, w: f64, h: f64) -> Transform {
    Transform {
        x,
        y,
        w,
        h,
        ..node.transform
    }
}
