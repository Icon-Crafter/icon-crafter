//! Integration tests for the CPU reference renderer.
//!
//! These exercise the real pipeline: document model -> layout -> display list
//! -> tiny-skia raster -> pixels. They are the visual regression baseline.

use dp_geom::{Point, Size};
use dp_model::{
    Color, Document, Fill, FillRule as ModelFillRule, Node, NodeGeometry, NodeId, NodeKind,
    NodeStyle, PageId, Paint, Stroke, StyleId, Transform, VariableId,
};
use dp_render::{RenderTarget, SceneBuilder, Viewport, render_page_png};
use dp_text::TextEngine;

fn doc_and_page() -> (Document, PageId) {
    let mut document = Document::new("Render test");
    let page = document.active_page_id();
    // Transparent page background so pixel tests can distinguish painted from
    // unpainted areas; the PNG test overrides the background explicitly.
    document.active_page_mut().background = Color::TRANSPARENT;
    (document, page)
}

fn add(document: &mut Document, page: PageId, node: Node, parent: Option<NodeId>) -> NodeId {
    let id = node.id;
    document
        .page_mut(page)
        .expect("page exists")
        .insert_node(node, parent, None)
        .expect("insert succeeds");
    id
}

fn viewport(width: f64, height: f64) -> Viewport {
    Viewport::new(0.0, 0.0, 1.0, width, height, 1.0)
}

fn render(document: &Document, page: PageId, viewport: &Viewport) -> RenderTarget {
    let mut text = TextEngine::new();
    let list = SceneBuilder::new(document)
        .build(page, viewport.visible_world_rect(), &mut text)
        .expect("display list builds");
    RenderTarget::render(&list, viewport, &mut text).expect("render succeeds")
}

fn red() -> Color {
    Color::from_hex("#ff0000").unwrap()
}

#[test]
fn solid_rectangle_renders_inside_and_not_outside() {
    let (mut document, page) = doc_and_page();
    add(
        &mut document,
        page,
        Node::rectangle("Rect", 10.0, 10.0, 50.0, 50.0).with_style(NodeStyle::filled(red())),
        None,
    );

    let target = render(&document, page, &viewport(200.0, 100.0));

    let inside = target.pixel_straight(35, 35).expect("inside pixel");
    assert_eq!(&inside[..3], &[255, 0, 0]);
    assert_eq!(inside[3], 255);

    let outside = target.pixel_straight(5, 5).expect("outside pixel");
    assert_eq!(outside[3], 0);
}

#[test]
fn panning_moves_content_on_screen() {
    let (mut document, page) = doc_and_page();
    add(
        &mut document,
        page,
        Node::rectangle("Rect", 10.0, 10.0, 50.0, 50.0).with_style(NodeStyle::filled(red())),
        None,
    );

    let viewport = Viewport::new(10.0, 10.0, 1.0, 200.0, 100.0, 1.0);
    let target = render(&document, page, &viewport);

    // The rectangle now starts at device (0, 0).
    assert_eq!(target.pixel_straight(5, 5).unwrap()[3], 255);
    assert_eq!(target.pixel_straight(99, 99).unwrap()[3], 0);
}

#[test]
fn zoom_scales_content() {
    let (mut document, page) = doc_and_page();
    add(
        &mut document,
        page,
        Node::rectangle("Rect", 10.0, 10.0, 50.0, 50.0).with_style(NodeStyle::filled(red())),
        None,
    );

    let viewport = Viewport::new(0.0, 0.0, 2.0, 200.0, 100.0, 1.0);
    let target = render(&document, page, &viewport);

    // With the origin fixed, doc (10, 10) lands at device (20, 20).
    assert_eq!(target.pixel_straight(15, 15).unwrap()[3], 0);
    assert_eq!(target.pixel_straight(30, 30).unwrap()[3], 255);
}

#[test]
fn opacity_multiplies_alpha() {
    let (mut document, page) = doc_and_page();
    let mut node =
        Node::rectangle("Rect", 0.0, 0.0, 50.0, 50.0).with_style(NodeStyle::filled(red()));
    node.opacity = 0.5;
    add(&mut document, page, node, None);

    let target = render(&document, page, &viewport(100.0, 100.0));
    let alpha = target.pixel_straight(25, 25).unwrap()[3];
    assert!((120..=135).contains(&alpha), "alpha was {alpha}");
}

#[test]
fn linear_gradient_varies_across_shape() {
    let (mut document, page) = doc_and_page();
    let style = NodeStyle {
        fills: vec![Fill {
            visible: true,
            opacity: 1.0,
            blend: Default::default(),
            paint: Paint::Linear {
                stops: vec![
                    dp_model::GradientStop::new(0.0, Color::from_hex("#000000").unwrap()),
                    dp_model::GradientStop::new(1.0, Color::from_hex("#ffffff").unwrap()),
                ],
                from: dp_geom::Point::new(0.0, 0.0),
                to: dp_geom::Point::new(1.0, 0.0),
                transform: None,
            },
            style_id: None::<StyleId>,
            variable_id: None::<VariableId>,
        }],
        ..NodeStyle::default()
    };
    add(
        &mut document,
        page,
        Node::rectangle("Gradient", 0.0, 0.0, 100.0, 40.0).with_style(style),
        None,
    );

    let target = render(&document, page, &viewport(120.0, 60.0));
    let left = target.pixel_straight(2, 20).unwrap();
    let right = target.pixel_straight(96, 20).unwrap();
    assert!(left[0] < 40, "left should be dark, got {left:?}");
    assert!(right[0] > 215, "right should be light, got {right:?}");
}

#[test]
fn strokes_paint_the_outline() {
    let (mut document, page) = doc_and_page();
    let mut style = NodeStyle::filled(Color::WHITE);
    let mut stroke = Stroke::solid(Color::BLACK);
    stroke.weight = 4.0;
    style.strokes.push(stroke);
    add(
        &mut document,
        page,
        Node::rectangle("Outlined", 10.0, 10.0, 60.0, 40.0).with_style(style),
        None,
    );

    let target = render(&document, page, &viewport(100.0, 100.0));
    // Border pixel is dark, interior stays white.
    let border = target.pixel_straight(10, 30).unwrap();
    assert!(border[0] < 60, "border should be dark, got {border:?}");
    let interior = target.pixel_straight(40, 30).unwrap();
    assert!(
        interior[0] > 200,
        "interior should be white, got {interior:?}"
    );
}

#[test]
fn frames_clip_overflowing_children() {
    let (mut document, page) = doc_and_page();
    let frame = add(
        &mut document,
        page,
        Node::frame("Frame", 0.0, 0.0, 100.0, 100.0),
        None,
    );
    add(
        &mut document,
        page,
        Node::rectangle("Child", 50.0, 50.0, 200.0, 200.0).with_style(NodeStyle::filled(red())),
        Some(frame),
    );

    let target = render(&document, page, &viewport(200.0, 100.0));
    // Inside the frame the child is visible, beyond the frame edge it is
    // clipped even though the device buffer is large enough.
    assert_eq!(target.pixel_straight(60, 60).unwrap()[3], 255);
    assert_eq!(target.pixel_straight(150, 60).unwrap()[3], 0);
}

#[test]
fn text_renders_pixels() {
    let (mut document, page) = doc_and_page();
    let mut node =
        Node::text("Label", "Icon Crafter", 10.0, 20.0).with_style(NodeStyle::filled(Color::BLACK));
    if let Some(text) = node.text_mut() {
        text.auto_size = dp_model::AutoSize::WidthAndHeight;
    }
    add(&mut document, page, node, None);

    let target = render(&document, page, &viewport(300.0, 100.0));
    let mut painted = 0;
    for y in 0..target.height() {
        for x in 0..target.width() {
            if target.pixel_straight(x, y).unwrap()[3] > 0 {
                painted += 1;
            }
        }
    }
    assert!(painted > 50, "expected glyph pixels, got {painted}");
}

#[test]
fn culling_drops_offscreen_nodes() {
    let (mut document, page) = doc_and_page();
    add(
        &mut document,
        page,
        Node::rectangle("Far", 5000.0, 5000.0, 10.0, 10.0).with_style(NodeStyle::filled(red())),
        None,
    );

    let mut text = TextEngine::new();
    let viewport = viewport(200.0, 100.0);
    let list = SceneBuilder::new(&document)
        .build(page, viewport.visible_world_rect(), &mut text)
        .unwrap();
    assert_eq!(list.len(), 0);
    assert_eq!(list.culled, 1);
}

#[test]
fn png_export_has_signature_and_size() {
    let (mut document, page) = doc_and_page();
    add(
        &mut document,
        page,
        Node::rectangle("Rect", 0.0, 0.0, 100.0, 50.0).with_style(NodeStyle::filled(red())),
        None,
    );

    let png = render_page_png(&document, page, &viewport(100.0, 50.0), Some(Color::WHITE))
        .expect("png renders");

    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    assert!(png.len() > 50);
}

#[test]
fn layout_map_sizes_text_from_content() {
    let (mut document, page) = doc_and_page();
    let mut text_node = Node::text("Label", "Auto layout text", 0.0, 0.0);
    if let Some(text) = text_node.text_mut() {
        text.auto_size = dp_model::AutoSize::WidthAndHeight;
    }
    add(&mut document, page, text_node, None);

    let mut engine = dp_layout::LayoutEngine::new();
    let page_ref = document.page(page).unwrap();
    let layout = engine.compute_page(page_ref);
    let node_id = document.page(page).unwrap().roots()[0];
    let measured = layout.get(node_id).expect("layout exists");
    assert!(measured.size.w > 0.0);
    assert!(measured.size.h > 0.0);
    assert_eq!(measured.size, Size::new(measured.size.w, measured.size.h));
}

fn boolean_document(operation: dp_geom::BooleanOp) -> (Document, PageId) {
    let (mut document, page) = doc_and_page();
    let mut node = Node::new(
        NodeKind::Boolean,
        "Boolean",
        Transform::new(0.0, 0.0, 100.0, 100.0),
    );
    node.style = NodeStyle::filled(red());
    node.geometry = Some(NodeGeometry::Boolean {
        operation,
        fill_rule: ModelFillRule::NonZero,
    });
    let boolean_id = add(&mut document, page, node, None);
    add(
        &mut document,
        page,
        Node::rectangle("A", 0.0, 0.0, 60.0, 60.0),
        Some(boolean_id),
    );
    add(
        &mut document,
        page,
        Node::rectangle("B", 40.0, 0.0, 60.0, 60.0),
        Some(boolean_id),
    );
    (document, page)
}

#[test]
fn boolean_subtract_renders_evaluated_geometry() {
    let (document, page) = boolean_document(dp_geom::BooleanOp::Subtract);
    let target = render(&document, page, &viewport(200.0, 100.0));
    // A minus B: the left part of A survives, the overlap and B are gone.
    let kept = target.pixel_straight(20, 30).expect("pixel inside A");
    assert_eq!(&kept[..3], &[255, 0, 0]);
    let removed = target.pixel_straight(70, 30).expect("pixel inside B");
    assert_eq!(removed[3], 0);
    let below = target.pixel_straight(20, 80).expect("pixel below A");
    assert_eq!(below[3], 0);
}

#[test]
fn boolean_union_merges_operand_outlines() {
    let (document, page) = boolean_document(dp_geom::BooleanOp::Union);
    let target = render(&document, page, &viewport(200.0, 100.0));
    let merged = target
        .pixel_straight(70, 30)
        .expect("pixel inside the union");
    assert_eq!(&merged[..3], &[255, 0, 0]);
    assert_eq!(merged[3], 255);
}

#[test]
fn drop_shadows_are_rasterized_behind_shapes() {
    let (mut document, page) = doc_and_page();
    let mut style = NodeStyle::filled(red());
    style.effects = vec![dp_model::Effect::drop_shadow(
        Color::BLACK,
        Point::new(10.0, 10.0),
        6.0,
    )];
    add(
        &mut document,
        page,
        Node::rectangle("Card", 20.0, 20.0, 60.0, 60.0).with_style(style),
        None,
    );
    let target = render(&document, page, &viewport(200.0, 150.0));

    let body = target.pixel_straight(50, 50).expect("shape pixel");
    assert_eq!(&body[..3], &[255, 0, 0]);
    assert_eq!(body[3], 255);

    // Offset shadow: just outside the bottom-right edge there must be visible
    // darkness, and far away the canvas must stay untouched.
    let shadow = target.pixel_straight(85, 85).expect("shadow pixel");
    assert!(shadow[3] > 0, "shadow alpha was {}", shadow[3]);
    assert!(shadow[0] < 200, "shadow was not dark: {shadow:?}");

    let untouched = target.pixel_straight(5, 5).expect("corner pixel");
    assert_eq!(untouched[3], 0);
}
