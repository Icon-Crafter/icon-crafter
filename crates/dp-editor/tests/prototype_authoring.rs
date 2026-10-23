//! Integration tests for prototype interaction authoring and Dev Mode
//! annotation resolution.

use dp_editor::EditorState;
use dp_model::{DocKind, Node, NodeId};
use serde_json::json;

fn editor() -> EditorState {
    EditorState::new_empty(DocKind::Design)
}

fn add_frame(state: &mut EditorState, name: &str, x: f64, y: f64) -> NodeId {
    let node = Node::frame(name, x, y, 400.0, 600.0);
    let id = node.id;
    let page_id = state.document.active_page_id();
    state
        .document
        .page_mut(page_id)
        .expect("active page")
        .insert_node(node, None, None)
        .expect("insert frame");
    state.invalidate_layout();
    id
}

fn add_rect(state: &mut EditorState, name: &str, x: f64, y: f64) -> NodeId {
    let node = Node::rectangle(name, x, y, 120.0, 48.0);
    let id = node.id;
    let page_id = state.document.active_page_id();
    state
        .document
        .page_mut(page_id)
        .expect("active page")
        .insert_node(node, None, None)
        .expect("insert rect");
    state.invalidate_layout();
    id
}

#[test]
fn interactions_are_authored_and_removed_through_commands() {
    let mut state = editor();
    let target = add_frame(&mut state, "Home", 0.0, 0.0);
    let hotspot = add_rect(&mut state, "Get started", 40.0, 40.0);
    state.select(hotspot, false);

    let outcome = state.command(
        "add_interaction",
        &json!({
            "trigger": {"type": "click"},
            "actions": [{"type": "navigate_to", "frame_id": target.to_string()}],
        }),
    );
    assert!(
        matches!(
            outcome,
            dp_editor::commands::CommandOutcome::Ok { changed: true }
        ),
        "unexpected outcome: {outcome:?}"
    );

    let node = state.document.node(hotspot).expect("hotspot").clone();
    let prototype = node.prototype.expect("prototype");
    assert_eq!(prototype.interactions.len(), 1);
    let interaction = &prototype.interactions[0];
    assert!(matches!(
        interaction.trigger,
        dp_model::prototype::Trigger::Click { .. }
    ));
    assert_eq!(interaction.actions.len(), 1);

    // Undo restores the node without interactions.
    assert!(state.undo().expect("undo"));
    let node = state.document.node(hotspot).expect("hotspot").clone();
    assert!(node.prototype.is_none());

    // Redo brings it back, then the interaction is removed by id.
    assert!(state.redo().expect("redo"));
    let node = state.document.node(hotspot).expect("hotspot").clone();
    let interaction_id = node.prototype.expect("prototype").interactions[0].id;
    let outcome = state.command(
        "remove_interaction",
        &json!({
            "node_id": hotspot.to_string(),
            "interaction_id": interaction_id.to_string(),
        }),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Ok { changed: true }
    ));
    let node = state.document.node(hotspot).expect("hotspot").clone();
    assert!(node.prototype.is_none());

    // Unknown interactions are reported as no-ops, not errors.
    let outcome = state.command(
        "remove_interaction",
        &json!({
            "node_id": hotspot.to_string(),
            "interaction_id": dp_model::InteractionId::new_v4().to_string(),
        }),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Ok { changed: false }
    ));
}

#[test]
fn invalid_interaction_payloads_are_rejected() {
    let mut state = editor();
    let hotspot = add_rect(&mut state, "Hotspot", 0.0, 0.0);
    state.select(hotspot, false);

    // Empty action list is a model validation error.
    let outcome = state.command(
        "add_interaction",
        &json!({"node_id": hotspot.to_string(), "trigger": {"type": "click"}, "actions": []}),
    );
    assert!(
        matches!(outcome, dp_editor::commands::CommandOutcome::Error { .. }),
        "unexpected outcome: {outcome:?}"
    );

    // Unknown trigger type is rejected by serde.
    let outcome = state.command(
        "add_interaction",
        &json!({"trigger": {"type": "telepathy"}, "actions": [{"type": "back"}]}),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Error { .. }
    ));
}

#[test]
fn dev_annotations_can_be_resolved() {
    let mut state = editor();
    let node = add_rect(&mut state, "Card", 0.0, 0.0);
    state.select(node, false);

    let id = state
        .dev_add_annotation(
            node,
            "This card uses a 12px radius; reuse the Card style.".to_owned(),
            "lead".to_owned(),
            "2026-10-24T00:00:00Z".to_owned(),
            dp_devmode::AnnotationKind::Implementation,
            Some("https://example.invalid/design-system".to_owned()),
        )
        .expect("add annotation");
    assert_eq!(state.handoff.annotations.len(), 1);
    assert_eq!(state.handoff.open_annotations().len(), 1);

    let outcome = state.command(
        "dev_resolve_annotation",
        &json!({"annotation_id": id.to_string()}),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Ok { changed: true }
    ));
    assert_eq!(state.handoff.open_annotations().len(), 0);
    assert!(state.handoff.annotations[0].resolved);

    let outcome = state.command(
        "dev_resolve_annotation",
        &json!({"annotation_id": uuid::Uuid::new_v4().to_string()}),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Ok { changed: false }
    ));
}
