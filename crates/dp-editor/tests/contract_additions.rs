//! Contract-addition tests: variable listing (names/types/values/active mode)
//! and presentation hotspots.

use dp_editor::EditorState;
use dp_model::{DocKind, Node, NodeId};
use serde_json::json;

fn editor() -> EditorState {
    EditorState::new_empty(DocKind::Design)
}

fn insert(state: &mut EditorState, node: Node, parent: Option<NodeId>) -> NodeId {
    let id = node.id;
    let page_id = state.document.active_page_id();
    state
        .document
        .page_mut(page_id)
        .expect("active page")
        .insert_node(node, parent, None)
        .expect("insert node");
    state.invalidate_layout();
    id
}

#[test]
fn variables_summary_lists_names_types_values_and_active_mode() {
    let mut state = editor();
    // Create a collection + colour variable through the real command path.
    let outcome = state.command(
        "create_color_variable",
        &json!({"collection": "Theme", "name": "color/brand", "color": "#ff6b4a"}),
    );
    assert!(
        matches!(outcome, dp_editor::commands::CommandOutcome::Ok { changed: true }),
        "unexpected outcome: {outcome:?}"
    );
    let outcome = state.command(
        "create_float_variable",
        &json!({"collection": "Theme", "name": "space/gap", "value": 12.0}),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Ok { changed: true }
    ));

    let summary = state.snapshot().summary.expect("summary");
    assert_eq!(summary.variables.len(), 1);
    let collection = &summary.variables[0];
    assert_eq!(collection.name, "Theme");
    assert_eq!(collection.variables.len(), 2);
    assert!(collection
        .variables
        .iter()
        .any(|variable| variable.name == "color/brand"));
    let brand = collection
        .variables
        .iter()
        .find(|variable| variable.name == "color/brand")
        .expect("brand variable");
    assert!(matches!(
        brand.resolved_type,
        dp_model::VariableType::Color
    ));
    assert_eq!(brand.values.len(), 1);
    assert!(matches!(
        brand.values.values().next(),
        Some(dp_model::VariableValue::Color { .. })
    ));
    // The engine leaves active_mode_id unset until the mode is pinned.
    assert!(collection.active_mode_id.is_none());

    // Pin a mode explicitly and confirm the summary exposes it.
    let mode_id = collection.modes[0].id;
    let outcome = state.command(
        "set_active_mode",
        &json!({"collection_id": collection.id.to_string(), "mode_id": mode_id.to_string()}),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Ok { changed: true }
    ));
    let summary = state.snapshot().summary.expect("summary");
    assert_eq!(summary.variables[0].active_mode_id, Some(mode_id));
}

#[test]
fn presentation_snapshot_lists_clickable_hotspots() {
    let mut state = editor();
    let frame = insert(
        &mut state,
        Node::frame("Home", 0.0, 0.0, 400.0, 600.0),
        None,
    );
    let hotspot = insert(
        &mut state,
        Node::rectangle("Get started", 40.0, 40.0, 120.0, 48.0),
        Some(frame),
    );
    state.select(hotspot, false);

    let outcome = state.command(
        "add_interaction",
        &json!({
            "node_id": hotspot.to_string(),
            "trigger": {"type": "click"},
            "actions": [{"type": "navigate_to", "frame_id": frame.to_string()}],
        }),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Ok { changed: true }
    ));

    state.prototype_play(Some(frame)).expect("present");
    let snapshot = state.snapshot();
    let prototype = snapshot.prototype.expect("prototype state");
    assert!(prototype.active);
    assert_eq!(prototype.frame_id, frame);
    assert_eq!(prototype.hotspots.len(), 1);
    let found = &prototype.hotspots[0];
    assert_eq!(found.node_id, hotspot);
    assert_eq!(found.label, "Get started");
    assert!(found.rect.w > 0.0 && found.rect.h > 0.0);

    state.prototype_stop();
    assert!(state.snapshot().prototype.is_none());
}
