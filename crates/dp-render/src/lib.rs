//! Rendering engine for Icon Crafter.
//!
//! The engine resolves a document page into a flat display list
//! ([`scene::DisplayList`]) and rasterizes it. Two backends are planned:
//!
//! * the CPU rasterizer (tiny-skia) in [`rasterize`], which is implemented,
//!   deterministic and used for tests, export and the current desktop
//!   viewport;
//! * a wgpu backend for GPU-accelerated compositing, declared behind the `gpu`
//!   feature and **not implemented yet**. The feature is off by default until a
//!   real pipeline with pixel parity exists; we do not ship a stub that claims
//!   GPU acceleration.
//!
//! Geometry, text, layout and paint resolution live in the model crates; this
//! crate only transforms them into draw calls.

pub mod builder;
pub mod rasterize;
pub mod scene;
pub mod viewport;

pub use builder::{BuildError, SceneBuilder};
pub use rasterize::{MAX_TARGET_EDGE, RenderError, RenderTarget, rasterize};
pub use scene::{
    DisplayList, SceneFill, SceneItem, ScenePaint, SceneShadow, SceneShape, SceneStop, SceneStroke,
    SceneText,
};
pub use viewport::{MAX_ZOOM, MIN_ZOOM, Viewport};

/// Version of the rendering engine API surface.
pub const RENDER_VERSION: &str = "0.1.0";

/// Renders one page into a PNG byte buffer, computing layout and scene first.
///
/// This is the convenience path used by thumbnail jobs and tests; interactive
/// rendering uses [`SceneBuilder::with_layout`] to reuse a cached layout.
pub fn render_page_png(
    document: &dp_model::Document,
    page_id: dp_model::PageId,
    viewport: &Viewport,
    background: Option<dp_model::Color>,
) -> Result<Vec<u8>, RenderError> {
    let mut text = dp_text::TextEngine::new();
    let builder = SceneBuilder::new(document);
    let mut list = builder
        .build(page_id, viewport.visible_world_rect(), &mut text)
        .map_err(|err| RenderError::Png(err.to_string()))?;
    if let Some(color) = background {
        list.background = color;
    }
    let target = RenderTarget::render(&list, viewport, &mut text)?;
    target.encode_png()
}
