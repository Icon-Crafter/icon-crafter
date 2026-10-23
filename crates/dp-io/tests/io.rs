//! Integration tests for import/export.

use dp_geom::Rect;
use dp_io::{
    ExportError, ExportFormat, ExportOptions, export_page, export_page_to_file, import_svg,
};
use dp_model::{Color, Document, Node, NodeKind, NodeStyle};

fn sample_document() -> (Document, dp_model::PageId) {
    let mut document = Document::new("Export Test");
    let page = document.active_page_id();
    document.active_page_mut().background = Color::TRANSPARENT;

    let mut frame = Node::frame("Frame", 0.0, 0.0, 200.0, 120.0);
    frame.style = NodeStyle::filled(Color::from_hex("#101318").unwrap());
    let frame_id = frame.id;
    document
        .active_page_mut()
        .insert_node(frame, None, None)
        .unwrap();

    let mut heading = Node::text("Heading", "Hello", 24.0, 24.0);
    heading.transform.w = 160.0;
    heading.transform.h = 32.0;
    if let Some(text) = heading.text_mut() {
        text.style.font_size = 24.0;
        text.auto_size = dp_model::AutoSize::None;
        text.style.fills = vec![dp_model::Fill::solid(Color::WHITE)];
    }
    document
        .active_page_mut()
        .insert_node(heading, Some(frame_id), None)
        .unwrap();

    let mut box_node = Node::rectangle("Block", 24.0, 80.0, 60.0, 20.0);
    box_node.style = NodeStyle::filled(Color::from_hex("#ff6b4a").unwrap());
    document
        .active_page_mut()
        .insert_node(box_node, Some(frame_id), None)
        .unwrap();

    (document, page)
}

#[test]
fn svg_export_contains_editable_geometry_and_text() {
    let (document, page) = sample_document();
    let options = ExportOptions::default();
    let svg = dp_io::export_svg(&document, page, &options).unwrap();
    assert!(svg.starts_with("<svg"), "{svg}");
    assert!(svg.contains("viewBox="), "{svg}");
    assert!(svg.contains("<text"), "text must stay editable: {svg}");
    assert!(svg.contains("Hello"), "{svg}");
    assert!(svg.contains("<path") || svg.contains("<rect"), "{svg}");
    assert!(svg.contains("matrix("), "{svg}");
}

#[test]
fn png_export_produces_requested_scale() {
    let (document, page) = sample_document();
    let mut options = ExportOptions::default();
    options.bounds = Some(Rect::new(0.0, 0.0, 200.0, 120.0));
    let file = export_page(&document, page, ExportFormat::Png, &options).unwrap();
    assert_eq!(&file.bytes[..8], b"\x89PNG\r\n\x1a\n");
    let image = image::load_from_memory(&file.bytes).unwrap();
    assert_eq!((image.width(), image.height()), (200, 120));

    options.scale = 2.0;
    let scaled = export_page(&document, page, ExportFormat::Png, &options).unwrap();
    let image = image::load_from_memory(&scaled.bytes).unwrap();
    assert_eq!((image.width(), image.height()), (400, 240));
}

#[test]
fn jpeg_export_has_jpeg_magic() {
    let (document, page) = sample_document();
    let mut options = ExportOptions::default();
    options.bounds = Some(Rect::new(0.0, 0.0, 200.0, 120.0));
    options.background = Some(Color::WHITE);
    let file = export_page(&document, page, ExportFormat::Jpeg, &options).unwrap();
    assert_eq!(&file.bytes[..2], b"\xff\xd8");
}

#[test]
fn pdf_export_produces_pdf_document() {
    let (document, page) = sample_document();
    let mut options = ExportOptions::default();
    options.bounds = Some(Rect::new(0.0, 0.0, 200.0, 120.0));
    let file = export_page(&document, page, ExportFormat::Pdf, &options).unwrap();
    assert!(
        file.bytes.starts_with(b"%PDF"),
        "{:?}",
        &file.bytes[..8.min(file.bytes.len())]
    );
    assert!(file.bytes.len() > 200);
}

#[test]
fn export_to_file_writes_bytes() {
    let (document, page) = sample_document();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("frame.svg");
    let options = ExportOptions::default();
    export_page_to_file(&document, page, ExportFormat::Svg, &options, &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("<svg"));
}

#[test]
fn svg_import_creates_editable_path() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60" viewBox="0 0 100 60">
        <path d="M10 10 L90 10 L90 50 L10 50 Z" fill="#ff6b4a"/>
    </svg>"##;
    let result = import_svg(svg, "Imported").unwrap();
    let page = result.document.active_page();
    assert_eq!(
        page.roots.len(),
        1,
        "imported content is grouped under one root"
    );
    let vector = page
        .nodes
        .values()
        .find(|node| node.kind == NodeKind::Vector)
        .expect("one imported vector path");
    assert!(vector.geometry.is_some());
    assert!(
        (vector.transform.w - 80.0).abs() < 0.01,
        "width {}",
        vector.transform.w
    );
    assert!(
        (vector.transform.h - 40.0).abs() < 0.01,
        "height {}",
        vector.transform.h
    );
    assert!(!vector.style.fills.is_empty(), "the fill is imported");
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
}

#[test]
fn svg_import_reports_skipped_text_instead_of_dropping_silently() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40">
        <text x="10" y="20">hello</text>
    </svg>"##;
    let result = import_svg(svg, "Text Only").unwrap();
    assert!(!result.warnings.is_empty(), "unsupported text must warn");
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.to_lowercase().contains("text"))
    );
}

#[test]
fn svg_import_rejects_malformed_input() {
    let error = import_svg("<svg><path d=", "Broken").unwrap_err();
    assert!(matches!(error, ExportError::Conversion(_)), "{error:?}");
}

#[test]
fn empty_page_export_is_a_typed_error() {
    let mut document = Document::new("Empty");
    document.active_page_mut().background = Color::TRANSPARENT;
    let page = document.active_page_id();
    let error = export_page(
        &document,
        page,
        ExportFormat::Svg,
        &ExportOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error, ExportError::EmptyDocument), "{error:?}");
}
