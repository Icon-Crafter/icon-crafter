//! Image import integration tests: real PNG bytes in, editable image layers
//! out, with undo/redo and save/reopen persistence.

use dp_editor::EditorState;
use dp_model::{DocKind, ImageFit, NodeKind, Paint};

fn write_png(directory: &std::path::Path) -> std::path::PathBuf {
    let pixels = image::RgbaImage::from_pixel(4, 4, image::Rgba([30, 140, 220, 255]));
    let path = directory.join("sample.png");
    image::DynamicImage::ImageRgba8(pixels)
        .save(&path)
        .expect("png writes");
    path
}

#[test]
fn imports_png_as_editable_image_layer() {
    let directory = tempfile::tempdir().expect("temp dir");
    let png = write_png(directory.path());
    let mut editor = EditorState::new_empty(DocKind::Design);

    let (node_id, warnings) = editor.import_image(&png).expect("image imports");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

    let detail = editor
        .node_details(&[node_id])
        .into_iter()
        .next()
        .expect("imported node detail");
    assert_eq!(detail.kind, NodeKind::Rectangle);
    assert_eq!(detail.transform.w, 4.0);
    assert_eq!(detail.transform.h, 4.0);
    let fill = detail.style.fills.first().expect("image fill");
    match &fill.paint {
        Paint::Image { asset_id, fit, .. } => {
            assert_eq!(*fit, ImageFit::Fill);
            assert!(editor.document.assets.images.contains_key(asset_id));
        }
        other => panic!("expected image paint, got {other:?}"),
    }
    assert_eq!(editor.selection, vec![node_id]);
    assert!(editor.history.undo_len() > 0);
}

#[test]
fn import_is_undoable_and_redoable() {
    let directory = tempfile::tempdir().expect("temp dir");
    let png = write_png(directory.path());
    let mut editor = EditorState::new_empty(DocKind::Design);
    let before_nodes = editor.document.active_page().nodes.len();
    let before_assets = editor.document.assets.images.len();

    let (node_id, _) = editor.import_image(&png).expect("image imports");
    assert_eq!(editor.document.active_page().nodes.len(), before_nodes + 1);

    editor.undo().expect("undo succeeds");
    assert_eq!(editor.document.active_page().nodes.len(), before_nodes);
    assert_eq!(editor.document.assets.images.len(), before_assets);

    editor.redo().expect("redo succeeds");
    assert_eq!(editor.document.active_page().nodes.len(), before_nodes + 1);
    assert!(editor.document.node(node_id).is_some());
    assert_eq!(editor.document.assets.images.len(), before_assets + 1);
}

#[test]
fn image_layer_survives_save_and_reopen() {
    let directory = tempfile::tempdir().expect("temp dir");
    let png = write_png(directory.path());
    let project = directory.path().join("image.icraft");
    let mut editor = EditorState::new_empty(DocKind::Design);
    let (node_id, _) = editor.import_image(&png).expect("image imports");
    editor.save_as(&project).expect("project saves");
    assert!(project.exists(), "project file must exist");

    let reopened = EditorState::open(&project).expect("project reopens");
    let node = reopened
        .document
        .node(node_id)
        .expect("image node persists");
    assert_eq!(node.kind, NodeKind::Rectangle);
    let fill = node.style.fills.first().expect("image fill persists");
    let asset_id = match &fill.paint {
        Paint::Image { asset_id, .. } => *asset_id,
        other => panic!("expected image paint, got {other:?}"),
    };
    let asset = reopened
        .document
        .assets
        .images
        .get(&asset_id)
        .expect("asset metadata persists");
    assert!(
        asset
            .data_base64
            .as_deref()
            .is_some_and(|data| !data.is_empty()),
        "embedded image bytes must survive save/reopen"
    );
    assert!(!reopened.dirty);
}
