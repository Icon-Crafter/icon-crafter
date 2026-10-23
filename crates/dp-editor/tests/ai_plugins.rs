//! Integration tests for AI assistance and the plugin registry through the real
//! editor/IPC-shaped path.

use dp_editor::EditorState;
use dp_model::DocKind;
use serde_json::Value;

fn editor() -> EditorState {
    EditorState::new_empty(DocKind::Design)
}

#[test]
fn ai_layout_preview_apply_and_undo() {
    let mut state = editor();
    let steps = state
        .ai_generate_layout("login screen for a mobile app", 390, 844)
        .expect("offline AI layout");
    assert!(steps >= 3);
    let preview = state.snapshot().ai_preview.expect("ai preview in snapshot");
    assert_eq!(preview.steps, steps);
    assert!(!preview.summary.is_empty());
    assert!(!preview.requires_confirmation);
    assert_eq!(state.selection.len(), 0);

    let applied = state.ai_apply_last_proposal(false).expect("apply");
    assert_eq!(applied, steps);
    let summary = state.snapshot().summary.expect("summary");
    assert!(summary.nodes.len() >= 3);
    assert!(state.last_ai_proposal.is_none());

    // Every AI change is a normal undo step.
    let undo_changed = state.undo().expect("undo");
    assert!(undo_changed);
}

#[test]
fn ai_apply_without_preview_is_a_validation_error() {
    let mut state = editor();
    let outcome = state.command("ai_apply_last", &Value::Object(Default::default()));
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Error { .. }
    ));
}

#[test]
fn ai_discard_removes_preview() {
    let mut state = editor();
    state.ai_generate_layout("dashboard", 1440, 900).unwrap();
    assert!(state.snapshot().ai_preview.is_some());
    let outcome = state.command("ai_discard_last", &Value::Null);
    assert!(matches!(outcome, dp_editor::commands::CommandOutcome::Ok { .. }));
    assert!(state.snapshot().ai_preview.is_none());
    assert!(state.page().nodes.is_empty());
}

#[test]
fn ai_text_task_reports_honest_offline_output() {
    let mut state = editor();
    let outcome = state.command(
        "ai_text_task",
        &serde_json::json!({"task": "improve", "input": "frame Login"}),
    );
    assert!(matches!(outcome, dp_editor::commands::CommandOutcome::Ok { .. }));
    assert!(state.status.contains("Offline analysis"));
    let outcome = state.command(
        "ai_text_task",
        &serde_json::json!({"task": "nope", "input": ""}),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Error { .. }
    ));
}

#[test]
fn plugins_install_toggle_and_remove_through_commands() {
    let mut state = editor();
    let outcome = state.command("plugin_install_example", &Value::Null);
    assert!(matches!(outcome, dp_editor::commands::CommandOutcome::Ok { .. }));
    let plugins = state.snapshot().plugins;
    assert_eq!(plugins.len(), 1);
    let plugin = plugins[0].clone();
    assert!(!plugin.enabled);
    assert!(!plugin.id.is_empty());
    assert!(plugin.manifest.is_object());

    let outcome = state.command(
        "plugin_set_enabled",
        &serde_json::json!({"plugin_id": plugin.id, "enabled": true}),
    );
    assert!(matches!(outcome, dp_editor::commands::CommandOutcome::Ok { .. }));
    assert!(state.snapshot().plugins[0].enabled);

    let outcome = state.command(
        "plugin_remove",
        &serde_json::json!({"plugin_id": plugin.id}),
    );
    assert!(matches!(outcome, dp_editor::commands::CommandOutcome::Ok { .. }));
    assert!(state.snapshot().plugins.is_empty());
}

#[test]
fn invalid_plugin_manifest_reports_validation_error() {
    let mut state = editor();
    let outcome = state.command(
        "plugin_install",
        &serde_json::json!({"manifest": {"id": "Not A Valid Id"}}),
    );
    assert!(matches!(
        outcome,
        dp_editor::commands::CommandOutcome::Error { .. }
    ));
    assert!(state.snapshot().plugins.is_empty());
}
