//! Tauri IPC layer: the single boundary between the React shell and the Rust
//! engine.
//!
//! The UI is a presentation layer. Every command here validates its arguments,
//! mutates the transport-independent [`EditorState`], and returns the contract
//! snapshot defined by `docs/CONTRACT.md` and mirrored in
//! `apps/ui/src/engine/contract.ts`.
//!
//! # Errors
//!
//! Commands return `Err(IpcError)` for failures that prevent a state snapshot
//! (for example a missing document or an IO error) and *always* return a
//! snapshot on success. Recoverable command failures (unknown command,
//! validation error) are reported inside `Snapshot.errors` with a contract
//! error code so the UI can render them without losing engine state.

use std::path::PathBuf;
use std::sync::Mutex;

use dp_editor::commands::CommandOutcome;
use dp_editor::input::{KeyInput, KeyKind, Modifiers, PointerInput, PointerKind, WheelInput};
use dp_editor::{EditorError, EditorState, ToolId};
use dp_model::{DocKind, Document};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tauri::State;

/// Managed application state (one editor at a time; multi-window later).
pub struct IpcState {
    inner: Mutex<IpcInner>,
}

#[derive(Default)]
struct IpcInner {
    editor: Option<EditorState>,
    recent: Vec<RecentFile>,
    preferences: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFile {
    /// Absolute path on disk.
    pub path: String,
    /// Display name (document name).
    pub name: String,
}

/// Error shape returned to the frontend.
#[derive(Debug, Serialize)]
pub struct IpcError {
    /// Contract error code: validation | io | permission | unsupported | conflict | internal.
    pub code: String,
    /// Human readable message.
    pub message: String,
}

impl IpcError {
    fn new(code: &str, message: impl Into<String>) -> IpcError {
        IpcError {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> IpcError {
        IpcError::new("internal", message)
    }

    fn validation(message: impl Into<String>) -> IpcError {
        IpcError::new("validation", message)
    }

    fn no_document() -> IpcError {
        IpcError::new("validation", "no document is open")
    }
}

impl From<EditorError> for IpcError {
    fn from(error: EditorError) -> IpcError {
        let code = match &error {
            EditorError::Unsupported(_) => "unsupported",
            EditorError::InvalidArguments(_) | EditorError::NoPath => "validation",
            EditorError::Model(_) | EditorError::Page(_) => "validation",
            EditorError::Persist(_) | EditorError::Export(_) => "io",
            EditorError::Render(_) => "internal",
        };
        IpcError::new(code, error.to_string())
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(error: serde_json::Error) -> IpcError {
        IpcError::internal(format!("serialization failed: {error}"))
    }
}

impl IpcState {
    pub fn new() -> IpcState {
        IpcState {
            inner: Mutex::new(IpcInner::default()),
        }
    }
}

// ---------------------------------------------------------------------------
// Argument types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PointerInputArgs {
    pub kind: String,
    pub button: u8,
    pub screen_x: f64,
    pub screen_y: f64,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub meta: bool,
    pub pressure: Option<f64>,
    // The UI sends `performance.now()` which is fractional; accept a number and
    // truncate to milliseconds for the engine's monotonic timestamp.
    #[serde(default)]
    pub timestamp_ms: f64,
}

#[derive(Debug, Deserialize)]
pub struct WheelInputArgs {
    pub delta_x: f64,
    pub delta_y: f64,
    pub screen_x: f64,
    pub screen_y: f64,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub meta: bool,
}

#[derive(Debug, Deserialize)]
pub struct KeyInputArgs {
    pub kind: String,
    pub key: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub meta: bool,
    #[serde(default)]
    pub repeat: bool,
}

#[derive(Debug, Deserialize)]
pub struct ExportRasterArgs {
    #[serde(default)]
    pub node_ids: Option<Vec<String>>,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub transparent: bool,
    pub path: String,
}

fn default_scale() -> f64 {
    1.0
}

#[derive(Debug, Deserialize)]
pub struct ExportVectorArgs {
    #[serde(default)]
    pub node_ids: Option<Vec<String>>,
    pub path: String,
}

impl PointerInputArgs {
    fn to_engine(&self) -> Result<PointerInput, IpcError> {
        let kind = match self.kind.as_str() {
            "down" => PointerKind::Down,
            "move" => PointerKind::Move,
            "up" => PointerKind::Up,
            "double_click" => PointerKind::DoubleClick,
            "leave" => PointerKind::Leave,
            other => {
                return Err(IpcError::validation(format!(
                    "unknown pointer kind '{other}'"
                )));
            }
        };
        Ok(PointerInput {
            kind,
            button: self.button,
            screen_x: self.screen_x,
            screen_y: self.screen_y,
            modifiers: Modifiers {
                shift: self.shift,
                alt: self.alt,
                ctrl: self.ctrl,
                meta: self.meta,
            },
            pressure: self.pressure.unwrap_or(0.5),
            timestamp_ms: self.timestamp_ms.max(0.0) as u64,
        })
    }
}

impl WheelInputArgs {
    fn to_engine(&self) -> WheelInput {
        WheelInput {
            delta_x: self.delta_x,
            delta_y: self.delta_y,
            screen_x: self.screen_x,
            screen_y: self.screen_y,
            modifiers: Modifiers {
                shift: self.shift,
                alt: self.alt,
                ctrl: self.ctrl,
                meta: self.meta,
            },
        }
    }
}

impl KeyInputArgs {
    fn to_engine(&self) -> Result<KeyInput, IpcError> {
        let kind = match self.kind.as_str() {
            "down" => KeyKind::Down,
            "up" => KeyKind::Up,
            other => {
                return Err(IpcError::validation(format!("unknown key kind '{other}'")));
            }
        };
        Ok(KeyInput {
            kind,
            key: self.key.clone(),
            code: self.code.clone(),
            modifiers: Modifiers {
                shift: self.shift,
                alt: self.alt,
                ctrl: self.ctrl,
                meta: self.meta,
            },
            repeat: self.repeat,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn lock_error() -> IpcError {
    IpcError::internal("engine state lock was poisoned")
}

fn snapshot_json(editor: &mut EditorState) -> Result<Value, IpcError> {
    Ok(serde_json::to_value(editor.snapshot())?)
}

fn empty_snapshot_json() -> Value {
    serde_json::to_value(EditorState::empty_snapshot()).unwrap_or(Value::Null)
}

fn with_editor<R>(
    state: &IpcState,
    action: impl FnOnce(&mut EditorState) -> Result<R, IpcError>,
) -> Result<R, IpcError> {
    let mut inner = state.inner.lock().map_err(|_| lock_error())?;
    let editor = inner.editor.as_mut().ok_or_else(IpcError::no_document)?;
    action(editor)
}

fn parse_kind(kind: Option<&str>) -> Result<DocKind, IpcError> {
    match kind {
        None | Some("") | Some("design") => Ok(DocKind::Design),
        Some("whiteboard") => Ok(DocKind::Whiteboard),
        Some("slides") => Ok(DocKind::Slides),
        Some("site") => Ok(DocKind::Site),
        Some("illustration") => Ok(DocKind::Illustration),
        Some(other) => Err(IpcError::validation(format!(
            "unknown document kind '{other}'"
        ))),
    }
}

fn file_size(path: &PathBuf) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

fn remember_recent(inner: &mut IpcInner, editor: &EditorState, path: &PathBuf) {
    let name = editor.document.name.clone();
    inner
        .recent
        .retain(|item| item.path != path.display().to_string());
    inner.recent.insert(
        0,
        RecentFile {
            path: path.display().to_string(),
            name,
        },
    );
    inner.recent.truncate(20);
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn bootstrap(state: State<'_, IpcState>) -> Result<Value, IpcError> {
    let inner = state.inner.lock().map_err(|_| lock_error())?;
    Ok(json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "recent_files": inner.recent,
        "preferences": Value::Object(inner.preferences.clone()),
        "capabilities": {
            "ai": false,
            "plugins": false,
            "collab": false,
            "gpu": false,
            "dev_mode": true,
            "export_pdf": true,
            "export_scope": true,
            "import_svg": true,
            "import_image": false,
            "prototype": true,
            "canvas_text_edit": true,
        },
    }))
}

#[tauri::command]
pub fn new_document(state: State<'_, IpcState>, kind: Option<String>) -> Result<Value, IpcError> {
    let kind = parse_kind(kind.as_deref())?;
    let mut inner = state.inner.lock().map_err(|_| lock_error())?;
    inner.editor = Some(EditorState::new_empty(kind));
    let editor = inner.editor.as_mut().ok_or_else(IpcError::no_document)?;
    snapshot_json(editor)
}

#[tauri::command]
pub fn open_document(state: State<'_, IpcState>, path: String) -> Result<Value, IpcError> {
    let editor = EditorState::open(&path).map_err(IpcError::from)?;
    let mut inner = state.inner.lock().map_err(|_| lock_error())?;
    remember_recent(&mut inner, &editor, &PathBuf::from(&path));
    inner.editor = Some(editor);
    let editor = inner.editor.as_mut().ok_or_else(IpcError::no_document)?;
    snapshot_json(editor)
}

#[tauri::command]
pub fn close_document(state: State<'_, IpcState>) -> Result<Value, IpcError> {
    let mut inner = state.inner.lock().map_err(|_| lock_error())?;
    inner.editor = None;
    Ok(empty_snapshot_json())
}

#[tauri::command]
pub fn save_document(state: State<'_, IpcState>, path: Option<String>) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        let saved = match path {
            Some(path) => editor.save_as(path)?,
            None => editor.save()?,
        };
        let bytes = file_size(&saved);
        Ok(json!({
            "path": saved.display().to_string(),
            "revision": editor.document.revision,
            "bytes": bytes,
        }))
    })
}

#[tauri::command]
pub fn engine_state(state: State<'_, IpcState>) -> Result<Value, IpcError> {
    let mut inner = state.inner.lock().map_err(|_| lock_error())?;
    match inner.editor.as_mut() {
        Some(editor) => snapshot_json(editor),
        None => Ok(empty_snapshot_json()),
    }
}

#[tauri::command]
pub fn set_tool(state: State<'_, IpcState>, tool: String) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        let outcome = editor.command("set_tool", &json!({ "tool": tool }));
        command_snapshot(editor, outcome)
    })
}

#[tauri::command]
pub fn pointer(state: State<'_, IpcState>, input: PointerInputArgs) -> Result<Value, IpcError> {
    let engine_input = input.to_engine()?;
    with_editor(&state, |editor| {
        editor.pointer(&engine_input)?;
        snapshot_json(editor)
    })
}

#[tauri::command]
pub fn wheel(state: State<'_, IpcState>, input: WheelInputArgs) -> Result<Value, IpcError> {
    let engine_input = input.to_engine();
    with_editor(&state, |editor| {
        editor.wheel(&engine_input);
        snapshot_json(editor)
    })
}

#[tauri::command]
pub fn key(state: State<'_, IpcState>, input: KeyInputArgs) -> Result<Value, IpcError> {
    let engine_input = input.to_engine()?;
    with_editor(&state, |editor| {
        editor.key(&engine_input)?;
        snapshot_json(editor)
    })
}

#[tauri::command]
pub fn command(
    state: State<'_, IpcState>,
    name: String,
    args: Option<Value>,
) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        let args = args.unwrap_or(Value::Null);
        let outcome = editor.command(&name, &args);
        command_snapshot(editor, outcome)
    })
}

fn command_snapshot(editor: &mut EditorState, outcome: CommandOutcome) -> Result<Value, IpcError> {
    let mut snapshot = editor.snapshot();
    match outcome {
        CommandOutcome::Unsupported { message } => {
            snapshot.errors.push(dp_editor::snapshot::SnapshotError {
                code: "unsupported".to_owned(),
                message,
                severity: "error".to_owned(),
            })
        }
        CommandOutcome::Error { code, message } => {
            let contract_code = match code.as_str() {
                "validation" | "io" | "permission" | "unsupported" | "conflict" | "internal" => {
                    code
                }
                _ => "internal".to_owned(),
            };
            snapshot.errors.push(dp_editor::snapshot::SnapshotError {
                code: contract_code,
                message,
                severity: "error".to_owned(),
            })
        }
        CommandOutcome::Ok { .. } => {}
    }
    Ok(serde_json::to_value(snapshot)?)
}

#[tauri::command]
pub fn render_frame(
    state: State<'_, IpcState>,
    // The contract types `quality` as a number; accept strings too so older
    // clients do not break. The CPU reference renderer currently renders at the
    // viewport's device resolution for every tier (tiers land with the tile
    // cache / GPU backend).
    quality: Option<Value>,
) -> Result<tauri::ipc::Response, IpcError> {
    let _tier = match quality.as_ref() {
        None | Some(Value::Null) => "high".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => format!("level-{number}"),
        Some(other) => {
            return Err(IpcError::validation(format!(
                "invalid render quality {other}"
            )));
        }
    };
    with_editor(&state, |editor| {
        let frame = editor.render()?;
        // The frame id travels with the state snapshot; the contract's
        // render_frame returns raw RGBA8 bytes (straight alpha).
        Ok(tauri::ipc::Response::new(frame.rgba))
    })
}

#[tauri::command]
pub fn node_details(state: State<'_, IpcState>, ids: Vec<String>) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        let mut parsed = Vec::with_capacity(ids.len());
        for id in ids {
            parsed.push(
                id.parse()
                    .map_err(|_| IpcError::validation(format!("invalid node id '{id}'")))?,
            );
        }
        let details = editor.details_snapshot(&parsed);
        Ok(serde_json::to_value(details)?)
    })
}

#[tauri::command]
pub fn export_png(state: State<'_, IpcState>, args: ExportRasterArgs) -> Result<Value, IpcError> {
    let ids = parse_node_ids(args.node_ids)?;
    with_editor(&state, |editor| {
        let path = PathBuf::from(&args.path);
        let saved = editor.export_png_scoped(&path, args.scale, args.transparent, &ids)?;
        Ok(json!({
            "path": saved.display().to_string(),
            "bytes": file_size(&saved),
            "format": "png",
        }))
    })
}

#[tauri::command]
pub fn export_svg(state: State<'_, IpcState>, args: ExportVectorArgs) -> Result<Value, IpcError> {
    let ids = parse_node_ids(args.node_ids)?;
    with_editor(&state, |editor| {
        let path = PathBuf::from(&args.path);
        let saved = editor.export_svg_scoped(&path, &ids)?;
        Ok(json!({
            "path": saved.display().to_string(),
            "bytes": file_size(&saved),
            "format": "svg",
        }))
    })
}

#[tauri::command]
pub fn export_pdf(state: State<'_, IpcState>, args: ExportVectorArgs) -> Result<Value, IpcError> {
    let ids = parse_node_ids(args.node_ids)?;
    with_editor(&state, |editor| {
        let path = PathBuf::from(&args.path);
        let saved = editor.export_pdf_scoped(&path, &ids)?;
        Ok(json!({
            "path": saved.display().to_string(),
            "bytes": file_size(&saved),
            "format": "pdf",
        }))
    })
}

#[tauri::command]
pub fn import_svg(
    state: State<'_, IpcState>,
    path: String,
    page_id: Option<String>,
    parent_id: Option<String>,
) -> Result<Value, IpcError> {
    let page = parse_optional_id(page_id, "page id")?;
    let parent = parse_optional_id(parent_id, "parent id")?;
    with_editor(&state, |editor| {
        editor.import_svg_at(&path, page, parent)?;
        snapshot_json(editor)
    })
}

#[tauri::command]
pub fn import_image(state: State<'_, IpcState>, path: String) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        editor.import_image(&path)?;
        snapshot_json(editor)
    })
}

#[tauri::command]
pub fn set_preference(
    state: State<'_, IpcState>,
    key: String,
    value: Value,
) -> Result<Value, IpcError> {
    let mut inner = state.inner.lock().map_err(|_| lock_error())?;
    inner.preferences.insert(key, value);
    Ok(Value::Object(inner.preferences.clone()))
}

// ---------------------------------------------------------------------------
// Dev Mode (inspection, code generation, handoff, revision compare)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn set_dev_mode(state: State<'_, IpcState>, enabled: bool) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        editor.set_dev_mode(enabled);
        snapshot_json(editor)
    })
}

#[tauri::command]
pub fn dev_inspect(state: State<'_, IpcState>) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        let inspection = editor.inspect_selection();
        serde_json::to_value(inspection).map_err(|error| IpcError::internal(error.to_string()))
    })
}

#[tauri::command]
pub fn dev_codegen(
    state: State<'_, IpcState>,
    target: String,
    precision: Option<u32>,
) -> Result<Value, IpcError> {
    let target = parse_code_target(&target)?;
    with_editor(&state, |editor| {
        let generated = editor.generate_code(target, precision.unwrap_or(2))?;
        serde_json::to_value(generated).map_err(|error| IpcError::internal(error.to_string()))
    })
}

#[tauri::command]
pub fn dev_handoff(state: State<'_, IpcState>) -> Result<Value, IpcError> {
    with_editor(&state, |editor| {
        serde_json::to_value(&editor.handoff).map_err(|error| IpcError::internal(error.to_string()))
    })
}

#[tauri::command]
pub fn dev_add_annotation(
    state: State<'_, IpcState>,
    node_id: String,
    body: String,
    author: String,
    created_at: String,
    kind: Option<String>,
    link: Option<String>,
) -> Result<Value, IpcError> {
    let node = parse_optional_id::<dp_model::NodeId>(Some(node_id), "node id")?
        .ok_or_else(|| IpcError::validation("node id is required"))?;
    let kind = match kind.as_deref() {
        None | Some("") | Some("note") => dp_devmode::AnnotationKind::Note,
        Some("implementation") => dp_devmode::AnnotationKind::Implementation,
        Some("measurement") => dp_devmode::AnnotationKind::Measurement,
        Some(other) => {
            return Err(IpcError::validation(format!(
                "unknown annotation kind '{other}'"
            )))
        }
    };
    with_editor(&state, |editor| {
        editor.dev_add_annotation(node, body.clone(), author.clone(), created_at.clone(), kind, link.clone())?;
        serde_json::to_value(&editor.handoff).map_err(|error| IpcError::internal(error.to_string()))
    })
}

#[tauri::command]
pub fn dev_mark_ready(
    state: State<'_, IpcState>,
    status: String,
    by: String,
    at: String,
) -> Result<Value, IpcError> {
    let status = parse_handoff_status(&status)?;
    with_editor(&state, |editor| {
        editor.dev_mark_ready(status, by.clone(), at.clone());
        serde_json::to_value(&editor.handoff).map_err(|error| IpcError::internal(error.to_string()))
    })
}

#[tauri::command]
pub fn dev_compare(
    state: State<'_, IpcState>,
    document_json: String,
) -> Result<Value, IpcError> {
    let other = Document::from_json_str(&document_json)
        .map_err(EditorError::from)
        .map_err(IpcError::from)?;
    with_editor(&state, |editor| {
        let diff = editor.compare_revision(&other);
        serde_json::to_value(diff).map_err(|error| IpcError::internal(error.to_string()))
    })
}

/// Returns the current document as canonical JSON string (Dev Mode revision capture).
#[tauri::command]
pub fn dev_snapshot(state: State<'_, IpcState>) -> Result<String, IpcError> {
    with_editor(&state, |editor| editor.document_json().map_err(IpcError::from))
}

fn parse_code_target(target: &str) -> Result<dp_devmode::CodeTarget, IpcError> {
    match target {
        "css" => Ok(dp_devmode::CodeTarget::Css),
        "react" | "react_typescript" | "tsx" => Ok(dp_devmode::CodeTarget::ReactTypeScript),
        "swiftui" | "swift" => Ok(dp_devmode::CodeTarget::SwiftUi),
        "compose" | "kotlin" => Ok(dp_devmode::CodeTarget::Compose),
        other => Err(IpcError::validation(format!(
            "unknown code target '{other}'"
        ))),
    }
}

fn parse_handoff_status(status: &str) -> Result<dp_devmode::HandoffStatus, IpcError> {
    match status {
        "draft" => Ok(dp_devmode::HandoffStatus::Draft),
        "ready_for_review" | "review" => Ok(dp_devmode::HandoffStatus::ReadyForReview),
        "ready_for_development" | "ready" => {
            Ok(dp_devmode::HandoffStatus::ReadyForDevelopment)
        }
        "implemented" => Ok(dp_devmode::HandoffStatus::Implemented),
        other => Err(IpcError::validation(format!(
            "unknown handoff status '{other}'"
        ))),
    }
}

fn parse_node_ids(ids: Option<Vec<String>>) -> Result<Vec<dp_model::NodeId>, IpcError> {
    let mut parsed = Vec::new();
    for id in ids.unwrap_or_default() {
        parsed.push(
            id.parse()
                .map_err(|_| IpcError::validation(format!("invalid node id '{id}'")))?,
        );
    }
    Ok(parsed)
}

fn parse_optional_id<T: std::str::FromStr>(
    value: Option<String>,
    label: &str,
) -> Result<Option<T>, IpcError> {
    match value {
        Some(text) => text
            .parse()
            .map(Some)
            .map_err(|_| IpcError::validation(format!("invalid {label} '{text}'"))),
        None => Ok(None),
    }
}

/// Tool ids accepted by the engine, for diagnostics and tests.
pub fn tool_ids() -> Vec<&'static str> {
    vec![
        "move",
        "hand",
        "frame",
        "slice",
        "rectangle",
        "ellipse",
        "line",
        "polygon",
        "star",
        "pen",
        "node",
        "text",
        "image",
        "comment",
        "connector",
    ]
}

/// Parses a tool id exactly like the `set_tool` command does.
pub fn parse_tool(tool: &str) -> Result<ToolId, IpcError> {
    serde_json::from_value(Value::String(tool.to_owned()))
        .map_err(|_| IpcError::validation(format!("unknown tool '{tool}'")))
}
