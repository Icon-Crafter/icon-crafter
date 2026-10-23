//! Raster export (PNG/JPEG) through the deterministic CPU renderer.

use dp_model::Document;
use dp_model::PageId;
use dp_render::{RenderTarget, SceneBuilder, Viewport};
use dp_text::TextEngine;
use image::{ImageEncoder, RgbaImage};

use crate::{ExportError, ExportFormat, ExportOptions};

/// Renders a page and encodes it as PNG or JPEG.
pub fn export_raster(
    document: &Document,
    page: PageId,
    format: ExportFormat,
    options: &ExportOptions,
) -> Result<Vec<u8>, ExportError> {
    if document.page(page).is_none() {
        return Err(ExportError::MissingPage(page));
    }
    let mut text = TextEngine::new();
    let huge = dp_geom::Rect::new(-1.0e9, -1.0e9, 2.0e9, 2.0e9);
    let mut list = SceneBuilder::new(document)
        .build(page, huge, &mut text)
        .map_err(ExportError::from)?;
    let content = crate::svg::content_bounds(&list);
    if content.w <= 0.0 || content.h <= 0.0 {
        return Err(ExportError::EmptyDocument);
    }
    let bounds = options.bounds.or(options.clip).unwrap_or(content);

    let scale = options.scale.clamp(0.01, 64.0);
    let background = options.background;
    if let Some(color) = background {
        list.background = color;
    }

    // Export scale is expressed through the device pixel ratio so the target
    // size is `bounds * scale` while the world-to-device transform stays
    // `(world - bounds.origin) * scale`.
    let viewport = Viewport::new(bounds.x, bounds.y, 1.0, bounds.w, bounds.h, scale);
    let target = RenderTarget::render(&list, &viewport, &mut text)
        .map_err(|error| ExportError::Render(error.to_string()))?;

    match format {
        ExportFormat::Png => target
            .encode_png()
            .map_err(|error| ExportError::Encoding(error.to_string())),
        ExportFormat::Jpeg => encode_jpeg(&target, background, options.jpeg_quality),
        _ => Err(ExportError::Encoding(format!(
            "raster export does not support {format:?}"
        ))),
    }
}

fn encode_jpeg(
    target: &RenderTarget,
    background: Option<dp_model::Color>,
    quality: u8,
) -> Result<Vec<u8>, ExportError> {
    let width = target.width();
    let height = target.height();
    let straight = target.data_straight_rgba8();
    let rgba = RgbaImage::from_raw(width, height, straight)
        .ok_or_else(|| ExportError::Encoding("invalid raster buffer".to_owned()))?;

    let bg = background.unwrap_or(dp_model::Color::WHITE);
    let bg_bytes = [
        (bg.r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (bg.g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (bg.b.clamp(0.0, 1.0) * 255.0).round() as u8,
    ];
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for pixel in rgba.pixels() {
        let alpha = pixel[3] as f64 / 255.0;
        for (channel, background_channel) in bg_bytes.iter().enumerate() {
            let value = pixel[channel] as f64 * alpha + *background_channel as f64 * (1.0 - alpha);
            rgb.push(value.round().clamp(0.0, 255.0) as u8);
        }
    }

    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        std::io::Cursor::new(&mut out),
        quality.clamp(1, 100),
    );
    encoder
        .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| ExportError::Encoding(error.to_string()))?;
    Ok(out)
}

/// Renders a page to a straight-alpha RGBA8 buffer (for the IPC canvas).
pub fn render_rgba(
    document: &Document,
    page: PageId,
    viewport: &Viewport,
) -> Result<(u32, u32, Vec<u8>), ExportError> {
    let mut text = TextEngine::new();
    let list = SceneBuilder::new(document)
        .build(page, viewport.visible_world_rect(), &mut text)
        .map_err(ExportError::from)?;
    let target = RenderTarget::render(&list, viewport, &mut text)
        .map_err(|error| ExportError::Render(error.to_string()))?;
    Ok((
        target.width(),
        target.height(),
        target.data_straight_rgba8(),
    ))
}

/// Convenience helper: encodes a straight RGBA8 buffer as PNG.
pub fn rgba_to_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, ExportError> {
    let image = RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| ExportError::Encoding("invalid rgba buffer".to_owned()))?;
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|error| ExportError::Encoding(error.to_string()))?;
    Ok(out)
}
