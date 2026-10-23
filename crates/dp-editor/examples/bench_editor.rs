//! Representative performance benchmark for the editor engine.
//!
//! Run with: `cargo run -p dp-editor --example bench_editor`
//! (dev profile: opt-level 1 for the editor, opt-level 2 for dependencies).
//!
//! The document is a 500-frame grid with 20 children each (10,500 nodes total),
//! which is a realistic mid-size design file. The benchmark measures the work
//! the UI performs per interaction, not synthetic micro-operations.

use std::time::Instant;

use dp_editor::input::{Modifiers, PointerInput, PointerKind};
use dp_editor::{EditorState, ToolId};
use dp_model::{Document, Node};

fn frame_count() -> usize {
    std::env::var("BENCH_FRAMES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(500)
}

fn build_document() -> (Document, Vec<dp_model::NodeId>) {
    let frames = frame_count();
    let mut document = Document::new("Benchmark");
    let page_id = document.active_page_id();
    let mut ids = Vec::with_capacity(frames * 21);
    {
        let page = document.page_mut(page_id).expect("active page");
        for index in 0..frames {
            let x = (index % 25) as f64 * 420.0;
            let y = (index / 25) as f64 * 700.0;
            let mut frame = Node::frame(format!("Screen {index}"), x, y, 390.0, 640.0);
            frame.style.corner_radii = dp_geom::CornerRadii::uniform(6.0);
            let frame_id = frame.id;
            page.insert_node(frame, None, None).expect("insert frame");
            ids.push(frame_id);
            for child in 0..20 {
                let node = Node::rectangle(
                    format!("Card {index}-{child}"),
                    16.0 + (child % 4) as f64 * 88.0,
                    24.0 + (child / 4) as f64 * 112.0,
                    72.0,
                    96.0,
                );
                let id = node.id;
                page.insert_node(node, Some(frame_id), None).expect("insert child");
                ids.push(id);
            }
        }
    }
    (document, ids)
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

fn main() {
    let (document, ids) = build_document();
    let node_count = ids.len();
    let mut state = EditorState::new_empty(dp_model::DocKind::Design);
    state.document = document;
    state.viewport = dp_render::Viewport::new(-40.0, -40.0, 0.6, 1440.0, 900.0, 1.0);
    state.set_tool(ToolId::Move);

    println!("== Icon Crafter editor benchmark (dev profile) ==");
    println!("nodes: {node_count}");

    let start = Instant::now();
    let _ = state.layout();
    println!("cold layout:        {:>8.1} ms", start.elapsed().as_secs_f64() * 1000.0);

    let start = Instant::now();
    for _ in 0..10 {
        state.invalidate_layout();
        let _ = state.layout();
    }
    println!(
        "layout (x10):       {:>8.1} ms total",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let sample: Vec<dp_model::NodeId> = ids.iter().take(1000).copied().collect();
    let start = Instant::now();
    let mut found = 0;
    for id in &sample {
        if state.node_bounds(*id).is_some() {
            found += 1;
        }
    }
    let elapsed = start.elapsed();
    println!(
        "bounds x1000:       {:>8.1} ms ({} found)",
        elapsed.as_secs_f64() * 1000.0,
        found
    );

    let start = Instant::now();
    let mut frame_bytes = 0;
    for _ in 0..3 {
        let frame = state.render().expect("render");
        frame_bytes = frame.rgba.len();
    }
    println!(
        "render 1440x900 x3: {:>8.1} ms ({frame_bytes} bytes/frame)",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let start = Instant::now();
    let mut hits = 0;
    for index in 0..200 {
        let x = 40.0 + (index % 20) as f64 * 60.0;
        let y = 60.0 + (index / 20) as f64 * 40.0;
        state.pointer(&pointer(PointerKind::Down, x, y)).ok();
        let selected = state.selection.len();
        state.pointer(&pointer(PointerKind::Up, x, y)).ok();
        if selected > 0 {
            hits += 1;
        }
    }
    println!(
        "pointer click x200: {:>8.1} ms ({hits} hits)",
        start.elapsed().as_secs_f64() * 1000.0
    );

    let start = Instant::now();
    let json = state.document.to_json_string().expect("serialize");
    println!(
        "serialize:          {:>8.1} ms ({} KiB)",
        start.elapsed().as_secs_f64() * 1000.0,
        json.len() / 1024
    );
    let start = Instant::now();
    let reloaded = Document::from_json_str(&json).expect("deserialize");
    println!(
        "deserialize:        {:>8.1} ms ({} nodes)",
        start.elapsed().as_secs_f64() * 1000.0,
        reloaded.pages.iter().map(|page| page.nodes.len()).sum::<usize>()
    );

    let targets: Vec<dp_model::NodeId> = ids.iter().skip(1).take(50).copied().collect();
    state.select_many(&targets);
    let start = Instant::now();
    state.command("delete", &serde_json::json!({}));
    println!(
        "delete 50 + undo:   {:>8.1} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    state.undo().expect("undo");

    assert!(node_count >= 10_000, "benchmark needs a large document");
}
