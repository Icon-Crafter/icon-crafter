//! Integration tests for the editor state machine: selection, dragging,
//! undo/redo, snapping, text editing, saving/reopening and export.
//!
//! These tests drive the same API the Tauri shell uses, so they cover the real
//! interaction path rather than a mock.

use dp_editor::commands::CommandOutcome;
use dp_editor::input::{KeyInput, KeyKind, Modifiers, PointerInput, PointerKind, WheelInput};
use dp_editor::{EditorState, ToolId};
use dp_model::{DocKind, Node, NodeId, NodeKind};
use serde_json::json;

fn new_editor() -> EditorState {
    EditorState::new_empty(DocKind::Design)
}

fn add_rect(state: &mut EditorState, name: &str, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    let node = Node::rectangle(name, x, y, w, h);
    let id = node.id;
    let page_id = state.document.active_page_id();
    state
        .document
        .page_mut(page_id)
        .expect("active page")
        .insert_node(node, None, None)
        .expect("insert node");
    state.invalidate_layout();
    id
}

fn add_text(state: &mut EditorState, characters: &str, x: f64, y: f64) -> NodeId {
    let node = Node::text("Text", characters, x, y);
    let id = node.id;
    let page_id = state.document.active_page_id();
    state
        .document
        .page_mut(page_id)
        .expect("active page")
        .insert_node(node, None, None)
        .expect("insert node");
    state.invalidate_layout();
    id
}

fn pointer(kind: PointerKind, x: f64, y: f64) -> PointerInput {
    PointerInput {
        kind,
        button: 0,
        screen_x: x,
        screen_y: y,
        modifiers: Modifiers::default(),
        pressure: 0.5,
        timestamp_ms: 0,
    }
}

fn key(k: &str, modifiers: Modifiers) -> KeyInput {
    KeyInput {
        kind: KeyKind::Down,
        key: k.to_owned(),
        code: String::new(),
        modifiers,
        repeat: false,
    }
}

/// Screen position of a world point with the default test viewport.
fn screen(state: &EditorState, wx: f64, wy: f64) -> (f64, f64) {
    let zoom = state.viewport.zoom;
    (
        (wx - state.viewport.x) * zoom,
        (wy - state.viewport.y) * zoom,
    )
}

#[test]
fn new_document_has_one_page_and_empty_selection() {
    let state = new_editor();
    let summary = state.summary();
    assert_eq!(summary.pages.len(), 1);
    assert!(summary.nodes.is_empty());
    assert!(state.selection.is_empty());
}

#[test]
fn click_selects_node_under_cursor() {
    let mut state = new_editor();
    let id = add_rect(&mut state, "Card", 100.0, 100.0, 200.0, 120.0);
    let (sx, sy) = screen(&state, 200.0, 160.0);
    state.pointer(&pointer(PointerKind::Down, sx, sy)).unwrap();
    state.pointer(&pointer(PointerKind::Up, sx, sy)).unwrap();
    assert_eq!(state.selection, vec![id]);
    assert_eq!(state.summary().nodes.len(), 1);
}

#[test]
fn drag_moves_selection_and_undo_restores() {
    let mut state = new_editor();
    let id = add_rect(&mut state, "Card", 100.0, 100.0, 200.0, 120.0);
    let (sx, sy) = screen(&state, 200.0, 160.0);
    state.pointer(&pointer(PointerKind::Down, sx, sy)).unwrap();
    assert_eq!(state.selection, vec![id]);
    let (mx, my) = screen(&state, 250.0, 210.0);
    state.pointer(&pointer(PointerKind::Move, mx, my)).unwrap();
    state.pointer(&pointer(PointerKind::Up, mx, my)).unwrap();
    let node = state.document.node(id).unwrap();
    assert!(
        (node.transform.x - 150.0).abs() < 0.5,
        "x = {}",
        node.transform.x
    );
    assert!(
        (node.transform.y - 150.0).abs() < 0.5,
        "y = {}",
        node.transform.y
    );
    state.undo().unwrap();
    let node = state.document.node(id).unwrap();
    assert!((node.transform.x - 100.0).abs() < 0.001);
}

#[test]
fn marquee_selects_intersecting_nodes() {
    let mut state = new_editor();
    let a = add_rect(&mut state, "A", 0.0, 0.0, 50.0, 50.0);
    let b = add_rect(&mut state, "B", 200.0, 200.0, 50.0, 50.0);
    let (sx, sy) = screen(&state, -20.0, -20.0);
    let (ex, ey) = screen(&state, 100.0, 100.0);
    state.pointer(&pointer(PointerKind::Down, sx, sy)).unwrap();
    state.pointer(&pointer(PointerKind::Move, ex, ey)).unwrap();
    state.pointer(&pointer(PointerKind::Up, ex, ey)).unwrap();
    assert_eq!(state.selection, vec![a]);
    assert!(!state.selection.contains(&b));
}

#[test]
fn wheel_pans_and_ctrl_wheel_zooms() {
    let mut state = new_editor();
    let before_x = state.viewport.x;
    let before_y = state.viewport.y;
    state.wheel(&WheelInput {
        delta_x: 0.0,
        delta_y: 120.0,
        screen_x: 100.0,
        screen_y: 100.0,
        modifiers: Modifiers::default(),
    });
    assert!(state.viewport.y > before_y);
    assert_eq!(state.viewport.x, before_x);
    let zoom_before = state.viewport.zoom;
    state.wheel(&WheelInput {
        delta_x: 0.0,
        delta_y: -200.0,
        screen_x: 300.0,
        screen_y: 200.0,
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    });
    assert!(state.viewport.zoom > zoom_before);
}

#[test]
fn delete_and_undo_restore_node() {
    let mut state = new_editor();
    let id = add_rect(&mut state, "Card", 100.0, 100.0, 200.0, 120.0);
    state.select(id, false);
    state.key(&key("Delete", Modifiers::default())).unwrap();
    assert!(state.document.node(id).is_none());
    state
        .key(&key(
            "z",
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        ))
        .unwrap();
    assert!(state.document.node(id).is_some());
}

#[test]
fn keyboard_nudge_moves_by_one_pixel() {
    let mut state = new_editor();
    let id = add_rect(&mut state, "Card", 10.0, 10.0, 20.0, 20.0);
    state.select(id, false);
    state.key(&key("ArrowRight", Modifiers::default())).unwrap();
    assert!((state.document.node(id).unwrap().transform.x - 11.0).abs() < 1e-9);
    state
        .key(&key(
            "ArrowDown",
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        ))
        .unwrap();
    assert!((state.document.node(id).unwrap().transform.y - 20.0).abs() < 1e-9);
}

#[test]
fn render_returns_pixels_and_overlay() {
    let mut state = new_editor();
    add_rect(&mut state, "Card", 100.0, 100.0, 200.0, 120.0);
    let frame = state.render().unwrap();
    assert_eq!(frame.width, state.viewport.width as u32);
    assert_eq!(
        frame.rgba.len(),
        (frame.width as usize) * (frame.height as usize) * 4
    );
    assert!(frame.overlay.viewport.zoom > 0.0);
}

#[test]
fn selected_node_pixels_are_painted() {
    let mut state = new_editor();
    let id = add_rect(&mut state, "Card", 100.0, 100.0, 200.0, 120.0);
    state.select(id, false);
    let frame = state.render().unwrap();
    // The node center in device pixels must not be the page background.
    let device_x = ((200.0 - state.viewport.x) * state.viewport.zoom * state.viewport.dpr) as u32;
    let device_y = ((160.0 - state.viewport.y) * state.viewport.zoom * state.viewport.dpr) as u32;
    let idx = ((device_y * frame.width + device_x) * 4) as usize;
    let pixel = &frame.rgba[idx..idx + 4];
    assert_eq!(pixel[3], 255, "node center should be opaque");
}

#[test]
fn text_editing_commits_as_undoable_transaction() {
    let mut state = new_editor();
    let id = add_text(&mut state, "Hello", 0.0, 0.0);
    state.text_edit_begin(id).unwrap();
    state.text_edit_input("Hello world", 11, None).unwrap();
    state.text_edit_commit().unwrap();
    assert_eq!(
        state
            .document
            .node(id)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .characters,
        "Hello world"
    );
    state.undo().unwrap();
    assert_eq!(
        state
            .document
            .node(id)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .characters,
        "Hello"
    );
}

#[test]
fn text_cancel_restores_original() {
    let mut state = new_editor();
    let id = add_text(&mut state, "Hello", 0.0, 0.0);
    state.text_edit_begin(id).unwrap();
    state.text_edit_input("Changed", 7, None).unwrap();
    assert!(state.text_edit_cancel());
    assert_eq!(
        state
            .document
            .node(id)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .characters,
        "Hello"
    );
}

#[test]
fn commands_dispatch_and_unknown_is_unsupported() {
    let mut state = new_editor();
    let outcome = state.command("set_tool", &json!({ "tool": "rectangle" }));
    assert_eq!(outcome, CommandOutcome::Ok { changed: false });
    assert_eq!(state.tool, ToolId::Rectangle);
    let outcome = state.command("definitely_not_a_command", &json!({}));
    assert!(matches!(outcome, CommandOutcome::Unsupported { .. }));
}

#[test]
fn select_nodes_command_updates_selection() {
    let mut state = new_editor();
    let a = add_rect(&mut state, "A", 0.0, 0.0, 10.0, 10.0);
    let b = add_rect(&mut state, "B", 20.0, 0.0, 10.0, 10.0);
    let outcome = state.command(
        "select_nodes",
        &json!({ "ids": [a.to_string(), b.to_string()] }),
    );
    assert_eq!(outcome, CommandOutcome::Ok { changed: false });
    assert_eq!(state.selection, vec![a, b]);
}

#[test]
fn group_and_ungroup_commands_work() {
    let mut state = new_editor();
    let a = add_rect(&mut state, "A", 0.0, 0.0, 50.0, 50.0);
    let b = add_rect(&mut state, "B", 60.0, 0.0, 50.0, 50.0);
    state.select_many(&[a, b]);
    let outcome = state.command("group", &json!({}));
    assert_eq!(outcome, CommandOutcome::Ok { changed: true });
    let page = state.document.active_page();
    assert!(page.nodes.values().any(|node| node.kind == NodeKind::Group));
    state.undo().unwrap();
    let page = state.document.active_page();
    assert!(!page.nodes.values().any(|node| node.kind == NodeKind::Group));
}

#[test]
fn align_command_moves_nodes() {
    let mut state = new_editor();
    let a = add_rect(&mut state, "A", 0.0, 0.0, 50.0, 50.0);
    let b = add_rect(&mut state, "B", 200.0, 100.0, 50.0, 50.0);
    state.select_many(&[a, b]);
    let outcome = state.command("align", &json!({ "align": "left" }));
    assert_eq!(outcome, CommandOutcome::Ok { changed: true });
    let left_a = state.document.node(a).unwrap().transform.x;
    let left_b = state.document.node(b).unwrap().transform.x;
    assert!((left_a - left_b).abs() < 1e-6);
}

#[test]
fn save_and_reopen_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roundtrip.icraft");
    let mut state = new_editor();
    add_rect(&mut state, "Card", 100.0, 100.0, 200.0, 120.0);
    state.save_as(&path).unwrap();
    assert!(path.exists());
    let reopened = EditorState::open(&path).unwrap();
    assert_eq!(reopened.summary().nodes.len(), 1);
    assert_eq!(reopened.summary().nodes[0].name, "Card");
}

#[test]
fn png_and_svg_export_produce_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = new_editor();
    add_rect(&mut state, "Card", 0.0, 0.0, 200.0, 120.0);
    let png = dir.path().join("card.png");
    state.export_png(&png, 1.0).unwrap();
    assert!(std::fs::metadata(&png).unwrap().len() > 8);
    let svg = dir.path().join("card.svg");
    state.export_svg(&svg).unwrap();
    let content = std::fs::read_to_string(&svg).unwrap();
    assert!(content.contains("<svg"));
}

#[test]
fn page_commands_manage_pages() {
    let mut state = new_editor();
    let outcome = state.command("add_page", &json!({ "name": "Second" }));
    assert_eq!(outcome, CommandOutcome::Ok { changed: true });
    assert_eq!(state.summary().pages.len(), 2);
    let second = state.summary().pages[1].id.to_string();
    let outcome = state.command("set_active_page", &json!({ "page_id": second }));
    assert_eq!(outcome, CommandOutcome::Ok { changed: false });
    assert_eq!(
        state.summary().pages[state.summary().pages.len() - 1].name,
        "Second"
    );
    let outcome = state.command(
        "delete_page",
        &json!({ "page_id": state.summary().pages[1].id.to_string() }),
    );
    assert_eq!(outcome, CommandOutcome::Ok { changed: true });
    assert_eq!(state.summary().pages.len(), 1);
}

#[test]
fn rename_and_opacity_commands_apply_patches() {
    let mut state = new_editor();
    let id = add_rect(&mut state, "Rect", 0.0, 0.0, 20.0, 20.0);
    state.select(id, false);
    state
        .command(
            "rename_node",
            &json!({ "id": id.to_string(), "name": "Card" }),
        )
        .into_ok();
    state
        .command("set_opacity", &json!({ "opacity": 0.5 }))
        .into_ok();
    let node = state.document.node(id).unwrap();
    assert_eq!(node.name, "Card");
    assert!((node.opacity - 0.5).abs() < 1e-9);
}

#[test]
fn components_can_be_created_instanced_and_detached() {
    let mut state = new_editor();
    let a = add_rect(&mut state, "Button label", 0.0, 0.0, 120.0, 40.0);
    let b = add_rect(&mut state, "Button icon", 140.0, 0.0, 16.0, 16.0);
    state
        .command("select_nodes", &json!({ "ids": [a, b] }))
        .into_ok();
    state
        .command("create_component", &json!({ "name": "Button" }))
        .into_ok();
    let snapshot = state.snapshot();
    let summary = snapshot.summary.expect("summary");
    assert_eq!(summary.components.len(), 1);
    let component_id = summary.components[0]
        .component_id
        .expect("component registry id");
    let master_id = summary.components[0].node_id;
    assert!(matches!(
        state.document.node(master_id).map(|node| node.kind),
        Some(NodeKind::Component)
    ));

    state
        .command(
            "place_instance",
            &json!({ "component_id": component_id, "x": 300.0, "y": 0.0 }),
        )
        .into_ok();
    let instance_id = *state.selection.first().expect("instance selected");
    assert_eq!(
        state.document.node(instance_id).map(|node| node.kind),
        Some(NodeKind::Instance)
    );
    let frame = state.render().expect("render instance");
    assert!(frame.width > 0 && frame.height > 0);

    state
        .command(
            "detach_instance",
            &json!({ "ids": [instance_id.to_string()] }),
        )
        .into_ok();
    assert_eq!(
        state.document.node(instance_id).map(|node| node.kind),
        Some(NodeKind::Frame)
    );
}

#[test]
fn shared_styles_can_be_created_and_applied() {
    let mut state = new_editor();
    let source = add_rect(&mut state, "Source", 0.0, 0.0, 100.0, 50.0);
    let target = add_rect(&mut state, "Target", 200.0, 0.0, 100.0, 50.0);
    let page_id = state.document.active_page_id();
    let page = state.document.page_mut(page_id).expect("page");
    page.nodes
        .get_mut(&source)
        .expect("source node")
        .style
        .fills = vec![dp_model::Fill::solid(
        dp_model::Color::from_hex("#ff6b4a").expect("colour"),
    )];
    state.select(source, false);
    state
        .command("create_style_from_node", &json!({ "name": "Brand Fill" }))
        .into_ok();
    let snapshot = state.snapshot();
    let style = snapshot.summary.expect("summary").styles.paints[0].clone();
    assert_eq!(style.name, "Brand Fill");

    state
        .command("select_nodes", &json!({ "ids": [target.to_string()] }))
        .into_ok();
    state
        .command("apply_style", &json!({ "style_id": style.id }))
        .into_ok();
    let target_node = state.document.node(target).expect("target");
    assert_eq!(
        target_node
            .style
            .fills
            .first()
            .and_then(|fill| fill.style_id),
        Some(style.id)
    );
}

#[test]
fn variable_collections_and_values_work_through_commands() {
    let mut state = new_editor();
    state
        .command(
            "create_color_variable",
            &json!({ "collection": "Brand", "name": "color/primary", "color": "#ff6b4a" }),
        )
        .into_ok();
    let snapshot = state.snapshot();
    let summary = snapshot.summary.expect("summary");
    let collection = summary
        .variables
        .iter()
        .find(|collection| collection.name == "Brand")
        .expect("Brand variable collection");
    assert!(!collection.modes.is_empty());
    let variable_id = state
        .document
        .variables
        .variables
        .values()
        .find(|variable| variable.name == "color/primary")
        .expect("variable")
        .id;
    let mode = collection.modes[0].id;
    state
        .command(
            "set_variable_value",
            &json!({ "variable_id": variable_id, "value": "#00ff00", "mode_id": mode }),
        )
        .into_ok();
    let resolved = state
        .document
        .variables
        .resolve_color(variable_id, mode)
        .expect("resolved colour");
    assert_eq!(resolved.to_hex(), "#00ff00");

    state
        .command(
            "set_active_mode",
            &json!({ "collection_id": collection.id, "mode_id": mode }),
        )
        .into_ok();
    assert_eq!(
        state
            .document
            .settings
            .active_modes
            .get(&collection.id)
            .copied(),
        Some(mode)
    );
}

#[test]
fn prototype_presentation_navigates_and_goes_back() {
    use dp_geom::Point;
    use dp_model::PrototypeNode;
    use dp_model::prototype::Interaction;

    let mut state = new_editor();
    let page_id = state.document.active_page_id();
    let page = state.document.page_mut(page_id).expect("page");
    let frame_a = Node::frame("Screen A", 0.0, 0.0, 200.0, 200.0);
    let frame_a_id = frame_a.id;
    page.insert_node(frame_a, None, None).expect("frame A");
    let frame_b = Node::frame("Screen B", 300.0, 0.0, 200.0, 200.0);
    let frame_b_id = frame_b.id;
    page.insert_node(frame_b, None, None).expect("frame B");
    let mut hotspot = Node::rectangle("Hotspot", 20.0, 20.0, 100.0, 40.0);
    hotspot.prototype = Some(PrototypeNode {
        interactions: vec![Interaction::navigate_on_click(frame_b_id)],
    });
    page.insert_node(hotspot, Some(frame_a_id), None)
        .expect("hotspot");
    state.invalidate_layout();

    state
        .prototype_play(Some(frame_a_id))
        .expect("start presenting");
    assert_eq!(
        state.playback.as_ref().map(|playback| playback.frame),
        Some(frame_a_id)
    );
    assert!(!state.playback.as_ref().expect("playback").can_back());

    let clicked = state
        .prototype_click_world(Point::new(60.0, 40.0))
        .expect("click hotspot");
    assert!(clicked);
    assert_eq!(
        state.playback.as_ref().map(|playback| playback.frame),
        Some(frame_b_id)
    );
    assert!(state.playback.as_ref().expect("playback").can_back());

    assert!(state.prototype_back().expect("navigate back"));
    assert_eq!(
        state.playback.as_ref().map(|playback| playback.frame),
        Some(frame_a_id)
    );

    // Clicking empty canvas does nothing and keeps the current frame.
    assert!(
        !state
            .prototype_click_world(Point::new(190.0, 190.0))
            .expect("click empty area")
    );
    state.prototype_stop();
    assert!(state.playback.is_none());
}

trait IntoOk {
    fn into_ok(self);
}

impl IntoOk for CommandOutcome {
    fn into_ok(self) {
        match self {
            CommandOutcome::Ok { .. } => {}
            other => panic!("expected Ok, got {other:?}"),
        }
    }
}
