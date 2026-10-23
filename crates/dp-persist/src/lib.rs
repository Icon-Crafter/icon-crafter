//! Persistence for Icon Crafter: native `.icraft` project files, schema
//! validation on open, atomic saves and crash recovery.
//!
//! # Container format
//!
//! An `.icraft` file is a ZIP archive containing:
//!
//! * `meta.json` — container metadata (format marker, container version, app
//!   version, save timestamp, document schema version);
//! * `document.json` — the canonical document JSON produced by
//!   [`dp_model::Document::to_json_value`];
//! * `assets/images/<id>.<ext>` and `assets/fonts/<id>.<ext>` — optional binary
//!   assets. Asset embedding is planned; the reader tolerates their absence and
//!   reports them, and the writer currently emits no assets because the model
//!   does not store bytes yet (documented limitation, not silent loss: the
//!   document references assets by id and the caller keeps the files).
//!
//! Opening validates the container, rejects newer container/schema versions
//! with a typed error, parses the document through the model's migration path
//! and reports validation errors instead of loading a corrupt document.
//!
//! Saves are atomic: bytes are written to a sibling temp file, flushed and
//! synced, then renamed over the destination. A crash leaves either the old
//! file or the new one, never a truncated project.
//!
//! # Recovery
//!
//! [`write_recovery`] writes the same container to a sibling
//! `<name>.icraft.recovery.icraft`. [`recovery_info`] lets the desktop app offer
//! to reopen it after a crash; [`recover`] loads it and [`clear_recovery`]
//! deletes it after the user accepts or rejects it.

use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use dp_model::{Document, ModelError};

/// Version of the persistence API surface.
pub const PERSIST_VERSION: &str = "0.1.0";
/// Container version written by this build.
pub const CONTAINER_VERSION: u32 = 1;
/// Archive entry name of the document payload.
pub const ARCHIVE_DOCUMENT: &str = "document.json";
/// Archive entry name of the container metadata.
pub const ARCHIVE_META: &str = "meta.json";
/// Upper bound for a single decompressed archive entry (256 MiB).
pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
/// Upper bound for the number of archive entries.
pub const MAX_ENTRIES: usize = 4096;
/// Suffix used for the crash-recovery sibling file.
pub const RECOVERY_SUFFIX: &str = ".recovery.icraft";
/// Temporary suffix for atomic writes.
const TEMP_SUFFIX: &str = ".writing.tmp";
/// Format marker stored in `meta.json`.
const FORMAT_MARKER: &str = "iconcrafter-project";

/// Metadata stored next to the document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectMeta {
    /// Always `"iconcrafter-project"`.
    pub format: String,
    /// Container layout version.
    pub container_version: u32,
    /// Document schema version (mirrors `document.schema`).
    pub document_schema: u32,
    /// Application version that wrote the file.
    pub app_version: String,
    /// RFC 3339 save timestamp.
    pub saved_at: String,
}

impl ProjectMeta {
    /// Builds metadata for a document save.
    pub fn for_document(document: &Document) -> ProjectMeta {
        ProjectMeta {
            format: FORMAT_MARKER.to_owned(),
            container_version: CONTAINER_VERSION,
            document_schema: document.schema,
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            saved_at: Utc::now().to_rfc3339(),
        }
    }
}

/// A loaded project.
#[derive(Debug, Clone)]
pub struct LoadedProject {
    /// Parsed, validated document.
    pub document: Document,
    /// Container metadata; `None` for archives written without it.
    pub meta: Option<ProjectMeta>,
}

/// Result of a successful save.
#[derive(Debug, Clone)]
pub struct SaveReport {
    /// Destination path.
    pub path: PathBuf,
    /// Size of the written archive in bytes.
    pub bytes_written: u64,
    /// Number of pages saved.
    pub page_count: usize,
    /// Number of nodes saved.
    pub node_count: usize,
}

/// Information about a recovery file found next to a project.
#[derive(Debug, Clone)]
pub struct RecoveryInfo {
    /// Path of the recovery archive.
    pub path: PathBuf,
    /// Last modification time.
    pub modified_at: DateTime<Utc>,
}

/// Errors raised by persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// Filesystem error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// ZIP container error.
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// Document model error (schema, migrations, malformed model).
    #[error("document error: {0}")]
    Model(#[from] ModelError),
    /// The archive is not a valid Icon Crafter project.
    #[error("invalid project archive: {0}")]
    InvalidArchive(String),
    /// The container version is newer than this build supports.
    #[error("unsupported container version {found} (this build supports up to {supported})")]
    UnsupportedContainer {
        /// Version found in the file.
        found: u32,
        /// Highest version this build understands.
        supported: u32,
    },
    /// The document failed validation.
    #[error("document failed validation: {0}")]
    InvalidDocument(String),
    /// An archive entry is larger than allowed.
    #[error("archive entry too large: {0} bytes (limit {MAX_ENTRY_BYTES})")]
    EntryTooLarge(u64),
}

/// Whether a path looks like a native project file (extension check only).
pub fn is_project_file(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("icraft"))
        .unwrap_or(false)
}

/// Suggested file name for a document (sanitized, `.icraft` extension).
pub fn suggested_file_name(document: &Document) -> String {
    let mut name = String::new();
    for ch in document.name.chars() {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
            name.push('_');
        } else {
            name.push(ch);
        }
        if name.len() >= 80 {
            break;
        }
    }
    let trimmed = name.trim().trim_end_matches('.');
    let base = if trimmed.is_empty() {
        "Untitled"
    } else {
        trimmed
    };
    format!("{base}.icraft")
}

/// Serializes a document into `.icraft` archive bytes.
pub fn encode_project(document: &Document) -> Result<Vec<u8>, PersistError> {
    let mut cursor = Cursor::new(Vec::new());
    write_archive(&mut cursor, document)?;
    Ok(cursor.into_inner())
}

/// Saves a document atomically to `path`.
pub fn save_project(
    path: impl AsRef<Path>,
    document: &Document,
) -> Result<SaveReport, PersistError> {
    let path = path.as_ref();
    let magic = check_document(document)?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let temp = temp_path(path);
    let result = (|| -> Result<SaveReport, PersistError> {
        let mut file = File::create(&temp)?;
        write_archive(&mut file, document)?;
        file.sync_all()?;
        drop(file);
        let bytes = fs::metadata(&temp)?.len();
        fs::rename(&temp, path)?;
        Ok(SaveReport {
            path: path.to_path_buf(),
            bytes_written: bytes,
            page_count: document.pages.len(),
            node_count: magic,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Opens a project file, validating container and document.
pub fn open_project(path: impl AsRef<Path>) -> Result<LoadedProject, PersistError> {
    let file = File::open(path)?;
    read_archive(file)
}

/// Opens a project from owned bytes.
pub fn open_project_bytes(bytes: Vec<u8>) -> Result<LoadedProject, PersistError> {
    read_archive(Cursor::new(bytes))
}

/// Path of the recovery sibling file for a project.
pub fn recovery_path(project: impl AsRef<Path>) -> PathBuf {
    let project = project.as_ref();
    let mut name = project
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("Untitled.icraft"));
    name.push(RECOVERY_SUFFIX);
    project.with_file_name(name)
}

/// Writes a crash-recovery snapshot next to `project`.
pub fn write_recovery(
    project: impl AsRef<Path>,
    document: &Document,
) -> Result<PathBuf, PersistError> {
    let path = recovery_path(project);
    save_project(&path, document)?;
    Ok(path)
}

/// Returns info about an existing recovery snapshot.
pub fn recovery_info(project: impl AsRef<Path>) -> Option<RecoveryInfo> {
    let path = recovery_path(project);
    let metadata = fs::metadata(&path).ok()?;
    let modified = metadata.modified().ok()?;
    let modified_at: DateTime<Utc> = modified.into();
    Some(RecoveryInfo { path, modified_at })
}

/// Loads a recovery snapshot for `project`.
pub fn recover(project: impl AsRef<Path>) -> Result<LoadedProject, PersistError> {
    open_project(recovery_path(project))
}

/// Deletes a recovery snapshot; returns whether one existed.
pub fn clear_recovery(project: impl AsRef<Path>) -> Result<bool, PersistError> {
    let path = recovery_path(project);
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("project.icraft"));
    name.push(TEMP_SUFFIX);
    path.with_file_name(name)
}

fn check_document(document: &Document) -> Result<usize, PersistError> {
    let issues = document.validate();
    let errors: Vec<String> = issues
        .iter()
        .filter(|issue| issue.severity == dp_model::ValidationSeverity::Error)
        .map(|issue| format!("{}: {} ({})", issue.code, issue.message, issue.path))
        .collect();
    if !errors.is_empty() {
        return Err(PersistError::InvalidDocument(errors.join("; ")));
    }
    Ok(node_count(document))
}

/// Total number of nodes across all pages.
pub fn node_count(document: &Document) -> usize {
    document.pages.iter().map(|page| page.nodes.len()).sum()
}

fn write_archive<W: Write + Seek>(writer: W, document: &Document) -> Result<W, PersistError> {
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let meta = ProjectMeta::for_document(document);
    zip.start_file(ARCHIVE_META, options)?;
    zip.write_all(&serde_json::to_vec_pretty(&meta)?)?;

    let value = document.to_json_value()?;
    let document_json = serde_json::to_vec(&value)?;
    zip.start_file(ARCHIVE_DOCUMENT, options)?;
    zip.write_all(&document_json)?;

    Ok(zip.finish()?)
}

fn read_archive<R: Read + Seek>(reader: R) -> Result<LoadedProject, PersistError> {
    let mut archive = ZipArchive::new(reader)?;
    if archive.len() > MAX_ENTRIES {
        return Err(PersistError::InvalidArchive(format!(
            "{} entries (limit {MAX_ENTRIES})",
            archive.len()
        )));
    }

    let meta = match archive.by_name(ARCHIVE_META) {
        Ok(mut entry) => {
            ensure_entry_size(entry.size())?;
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            let meta: ProjectMeta = serde_json::from_str(&text)?;
            if meta.format != FORMAT_MARKER {
                return Err(PersistError::InvalidArchive(format!(
                    "format marker {:?} is not {:?}",
                    meta.format, FORMAT_MARKER
                )));
            }
            if meta.container_version > CONTAINER_VERSION {
                return Err(PersistError::UnsupportedContainer {
                    found: meta.container_version,
                    supported: CONTAINER_VERSION,
                });
            }
            Some(meta)
        }
        Err(zip::result::ZipError::FileNotFound) => None,
        Err(error) => return Err(error.into()),
    };

    let mut entry = archive
        .by_name(ARCHIVE_DOCUMENT)
        .map_err(|_| PersistError::InvalidArchive("missing document.json".to_owned()))?;
    ensure_entry_size(entry.size())?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    drop(entry);

    let document = Document::from_json_str(&text)?;
    let issues = document.validate();
    let errors: Vec<String> = issues
        .iter()
        .filter(|issue| issue.severity == dp_model::ValidationSeverity::Error)
        .map(|issue| format!("{}: {} ({})", issue.code, issue.message, issue.path))
        .collect();
    if !errors.is_empty() {
        return Err(PersistError::InvalidDocument(errors.join("; ")));
    }

    Ok(LoadedProject { document, meta })
}

fn ensure_entry_size(size: u64) -> Result<(), PersistError> {
    if size > MAX_ENTRY_BYTES {
        Err(PersistError::EntryTooLarge(size))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dp_model::{Color, Node, NodeId, PageId, Transform};

    fn sample_document() -> (Document, NodeId) {
        let mut document = Document::new("Sample");
        let mut frame = Node::frame("Frame", 0.0, 0.0, 100.0, 80.0);
        frame.transform.x = 40.0;
        frame.transform.y = 20.0;
        let frame_id = frame.id;
        document
            .active_page_mut()
            .insert_node(frame, None, None)
            .unwrap();
        let mut child = Node::rectangle("Child", 10.0, 10.0, 30.0, 20.0);
        let child_id = child.id;
        child.style = dp_model::NodeStyle::filled(Color::BLACK);
        document
            .active_page_mut()
            .insert_node(child, Some(frame_id), None)
            .unwrap();
        (document, child_id)
    }

    #[test]
    fn roundtrip_preserves_nodes_and_hierarchy() {
        let (document, child_id) = sample_document();
        let bytes = encode_project(&document).unwrap();
        let loaded = open_project_bytes(bytes).unwrap();
        assert_eq!(loaded.document.name, "Sample");
        assert_eq!(loaded.document.schema, 1);
        assert_eq!(node_count(&loaded.document), node_count(&document));
        let node = loaded.document.node(child_id).expect("child node survives");
        assert_eq!(node.name, "Child");
        assert!(node.parent.is_some());
        assert_eq!(loaded.document.active_page().roots.len(), 1);
        assert!(loaded.meta.is_some());
        assert_eq!(loaded.meta.unwrap().container_version, CONTAINER_VERSION);
    }

    #[test]
    fn roundtrip_is_document_stable() {
        let (document, _) = sample_document();
        let loaded = open_project_bytes(encode_project(&document).unwrap()).unwrap();
        assert_eq!(
            loaded.document.to_json_string().unwrap(),
            document.to_json_string().unwrap()
        );
    }

    #[test]
    fn rejects_non_zip_bytes() {
        let error = open_project_bytes(b"this is not a project".to_vec()).unwrap_err();
        assert!(matches!(error, PersistError::Zip(_)), "{error:?}");
    }

    #[test]
    fn rejects_archive_without_document() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            zip.start_file("other.txt", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"hi").unwrap();
            zip.finish().unwrap();
        }
        let error = open_project_bytes(cursor.into_inner()).unwrap_err();
        assert!(
            matches!(error, PersistError::InvalidArchive(_)),
            "{error:?}"
        );
    }

    #[test]
    fn rejects_wrong_format_marker() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            zip.start_file(ARCHIVE_META, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(br#"{"format":"other","container_version":1,"document_schema":1,"app_version":"0","saved_at":"x"}"#)
                .unwrap();
            zip.finish().unwrap();
        }
        let error = open_project_bytes(cursor.into_inner()).unwrap_err();
        assert!(
            matches!(error, PersistError::InvalidArchive(_)),
            "{error:?}"
        );
    }

    #[test]
    fn save_is_atomic_and_opens_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.icraft");
        let (document, _) = sample_document();
        let report = save_project(&path, &document).unwrap();
        assert!(report.bytes_written > 0);
        assert_eq!(report.page_count, document.pages.len());
        assert!(path.exists());
        assert!(!temp_path(&path).exists(), "temp file must be gone");
        assert!(is_project_file(&path));
        let loaded = open_project(&path).unwrap();
        assert_eq!(loaded.document.name, document.name);
    }

    #[test]
    fn saving_over_existing_project_replaces_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.icraft");
        let (mut document, _) = sample_document();
        save_project(&path, &document).unwrap();
        document.name = "Renamed".into();
        save_project(&path, &document).unwrap();
        assert_eq!(open_project(&path).unwrap().document.name, "Renamed");
    }

    #[test]
    fn recovery_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Design.icraft");
        let (document, child_id) = sample_document();
        assert!(recovery_info(&project).is_none());
        let recovery = write_recovery(&project, &document).unwrap();
        assert_eq!(recovery, recovery_path(&project));
        let info = recovery_info(&project).expect("recovery exists");
        assert_eq!(info.path, recovery);
        let loaded = recover(&project).unwrap();
        assert!(loaded.document.node(child_id).is_some());
        assert!(clear_recovery(&project).unwrap());
        assert!(!clear_recovery(&project).unwrap());
        assert!(recovery_info(&project).is_none());
        assert!(
            !project.exists(),
            "recovery must not touch the project path"
        );
    }

    #[test]
    fn invalid_documents_are_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.icraft");
        let (mut document, child_id) = sample_document();
        // Remove a child from the map but keep it referenced from its parent.
        let page = document.active_page_mut();
        page.nodes.shift_remove(&child_id);
        let error = save_project(&path, &document).unwrap_err();
        assert!(
            matches!(error, PersistError::InvalidDocument(_)),
            "{error:?}"
        );
        assert!(!path.exists());
    }

    #[test]
    fn suggested_names_are_safe() {
        let mut document = Document::new("Q3 / Launch: \"Hero\"?");
        assert_eq!(
            suggested_file_name(&document),
            "Q3 _ Launch_ _Hero__.icraft"
        );
        document.name = "   ".into();
        assert_eq!(suggested_file_name(&document), "Untitled.icraft");
    }

    #[test]
    fn container_version_is_bounded() {
        let (document, _) = sample_document();
        let bytes = encode_project(&document).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut meta: ProjectMeta = {
            let mut entry = archive.by_name(ARCHIVE_META).unwrap();
            let mut text = String::new();
            entry.read_to_string(&mut text).unwrap();
            serde_json::from_str(&text).unwrap()
        };
        meta.container_version = CONTAINER_VERSION + 1;
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            zip.start_file(ARCHIVE_META, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&serde_json::to_vec(&meta).unwrap()).unwrap();
            zip.start_file(ARCHIVE_DOCUMENT, SimpleFileOptions::default())
                .unwrap();
            let value = document.to_json_value().unwrap();
            zip.write_all(&serde_json::to_vec(&value).unwrap()).unwrap();
            zip.finish().unwrap();
        }
        let error = open_project_bytes(cursor.into_inner()).unwrap_err();
        match error {
            PersistError::UnsupportedContainer { found, supported } => {
                assert_eq!(found, CONTAINER_VERSION + 1);
                assert_eq!(supported, CONTAINER_VERSION);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn page_ids_survive_roundtrip() {
        let (document, _) = sample_document();
        let page: PageId = document.active_page_id();
        let loaded = open_project_bytes(encode_project(&document).unwrap()).unwrap();
        assert!(
            loaded
                .document
                .pages
                .iter()
                .any(|candidate| candidate.id == page)
        );
    }

    #[test]
    fn transform_attributes_survive_roundtrip() {
        let (mut document, _) = sample_document();
        let node_id = document.active_page().roots[0];
        {
            let node = document.active_page_mut().nodes.get_mut(&node_id).unwrap();
            node.transform = Transform::new(12.5, -8.25, 320.0, 240.0);
            node.opacity = 0.4;
        }
        let loaded = open_project_bytes(encode_project(&document).unwrap()).unwrap();
        let node = loaded.document.node(node_id).unwrap();
        assert_eq!(node.transform.x, 12.5);
        assert_eq!(node.transform.y, -8.25);
        assert_eq!(node.transform.w, 320.0);
        assert_eq!(node.opacity, 0.4);
    }
}
