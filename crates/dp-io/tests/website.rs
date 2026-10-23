//! Website export integration tests: real HTML/CSS should come out of a real
//! document, hotspots and vector SVG included, with honest warnings.

use dp_io::ExportError;
use dp_io::website::{WebsiteOptions, export_website, write_website};
use dp_model::{
    Color, Document, Fill, InteractionId, Node, NodeKind, PageId, PrototypeNode,
    prototype::{Action, Interaction, Trigger},
};

fn document() -> Document {
    let mut document = Document::new("Website Test");
    let page_id = document.active_page_id();
    let page = document.page_mut(page_id).expect("page");

    let mut hero = Node::frame("Hero", 0.0, 0.0, 640.0, 360.0);
    hero.style.fills = vec![Fill::solid(Color::from_hex("#101318").unwrap())];
    let hero_id = hero.id;
    page.insert_node(hero, None, None).unwrap();

    let mut title = Node::text("Title", "Hello Icon Crafter", 40.0, 64.0);
    title.style.fills = vec![Fill::solid(Color::WHITE)];
    page.insert_node(title, Some(hero_id), None).unwrap();

    let mut card = Node::rectangle("Card", 40.0, 200.0, 240.0, 120.0);
    card.style.fills = vec![Fill::solid(Color::from_hex("#ff6b4a").unwrap())];
    card.style.corner_radii = dp_geom::CornerRadii::uniform(12.0);
    let card_id = card.id;
    page.insert_node(card, Some(hero_id), None).unwrap();

    let mut sparkle = Node::new(
        NodeKind::Star,
        "Sparkle",
        dp_model::Transform::new(460.0, 60.0, 80.0, 80.0),
    );
    sparkle.geometry = Some(dp_model::NodeGeometry::Star {
        point_count: 4,
        inner_ratio: 0.35,
    });
    sparkle.style.fills = vec![Fill::solid(Color::from_hex("#2dd4bf").unwrap())];
    page.insert_node(sparkle, Some(hero_id), None).unwrap();

    let mut about = Node::frame("About", 0.0, 400.0, 640.0, 240.0);
    about.style.fills = vec![Fill::solid(Color::WHITE)];
    let about_id = about.id;
    page.insert_node(about, None, None).unwrap();

    // A click hotspot on the card navigating to the About frame.
    let mut card_copy = page.nodes.get(&card_id).unwrap().clone();
    card_copy.prototype = Some(PrototypeNode {
        interactions: vec![Interaction {
            id: InteractionId::new_v4(),
            trigger: Trigger::click(),
            actions: vec![Action::NavigateTo {
                frame_id: about_id,
                transition: None,
            }],
        }],
    });
    let mut page = document.page_mut(page_id).expect("page");
    page.remove_node(card_id);
    page.insert_node(card_copy, Some(hero_id), None).unwrap();
    document
}

fn page_id(document: &Document) -> PageId {
    document.active_page_id()
}

#[test]
fn exports_sections_text_shapes_and_css() {
    let document = document();
    let bundle = export_website(&document, page_id(&document), &WebsiteOptions::default())
        .expect("website export");
    assert!(bundle.html.starts_with("<!DOCTYPE html>"));
    assert!(bundle.html.contains("<section"));
    assert!(bundle.html.contains("Hello Icon Crafter"), "text must be real text");
    assert!(bundle.html.contains("Hero"));
    assert!(
        bundle.css.contains("width: 640px"),
        "section size should be in CSS: {}",
        bundle.css
    );
    assert!(bundle.css.contains("#101318"));
    assert!(bundle.html.contains("<svg"), "star should be inline SVG");
    assert!(bundle.sections >= 2);
    assert_eq!(bundle.elements, bundle.sections + 3 + 1); // children + hotspot anchor
}

#[test]
fn exports_navigation_hotspots() {
    let document = document();
    let bundle = export_website(&document, page_id(&document), &WebsiteOptions::default())
        .expect("website export");
    assert!(
        bundle.html.contains("ic-hotspot"),
        "hotspot anchor missing: {}",
        bundle.html
    );
    assert!(bundle.html.contains("href=\"#about\""), "{}", bundle.html);
}

#[test]
fn images_and_missing_geometry_are_warned() {
    let mut document = document();
    let page_id = document.active_page_id();
    let hero = document
        .active_page()
        .nodes
        .values()
        .find(|node| node.kind == NodeKind::Frame && node.name == "Hero")
        .map(|node| node.id)
        .expect("hero frame");
    let page = document.page_mut(page_id).expect("page");
    let image = Node::new(
        NodeKind::Image,
        "Photo",
        dp_model::Transform::new(20.0, 120.0, 120.0, 80.0),
    );
    page.insert_node(image, Some(hero), None).unwrap();
    let bundle = export_website(&document, page_id, &WebsiteOptions::default()).unwrap();
    assert!(
        bundle
            .warnings
            .iter()
            .any(|warning| warning.contains("image layers")),
        "{:?}",
        bundle.warnings
    );
    assert!(bundle.html.contains("ic-image"), "{}", bundle.html);
}

#[test]
fn writes_index_and_stylesheet_to_disk() {
    let document = document();
    let dir = tempfile::tempdir().unwrap();
    let index = write_website(
        &document,
        page_id(&document),
        &WebsiteOptions {
            title: "Portfolio".to_owned(),
            ..Default::default()
        },
        dir.path(),
    )
    .expect("write website");
    assert!(index.ends_with("index.html"));
    let html = std::fs::read_to_string(&index).unwrap();
    let css = std::fs::read_to_string(dir.path().join("styles.css")).unwrap();
    assert!(html.contains("<title>Portfolio</title>"));
    assert!(html.contains("ic-section"));
    assert!(css.contains(".ic-node"));
}

#[test]
fn empty_page_is_an_error() {
    let document = Document::new("Empty");
    let error = export_website(&document, page_id(&document), &WebsiteOptions::default())
        .expect_err("empty document must fail");
    assert!(matches!(error, ExportError::EmptyDocument));
}

#[test]
fn root_filter_limits_sections() {
    let document = document();
    let hero = document
        .active_page()
        .nodes
        .values()
        .find(|node| node.kind == NodeKind::Frame && node.name == "Hero")
        .map(|node| node.id)
        .expect("hero");
    let bundle = export_website(
        &document,
        page_id(&document),
        &WebsiteOptions {
            root_ids: vec![hero],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bundle.sections, 1);
    assert!(!bundle.html.contains(">About<"));
}
