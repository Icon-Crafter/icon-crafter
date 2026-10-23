//! CPU rasterizer: deterministic, reference implementation of the display list.
//!
//! tiny-skia renders into a premultiplied RGBA8 pixmap. The rasterizer handles
//! fills (solid/linear/radial), strokes, text glyphs, node opacity, blend modes
//! and accumulated rectangular clips. The same display list will feed a future
//! wgpu backend; the CPU path stays the reference for visual tests and export.

use std::sync::Arc;

use dp_geom::{FillRule as GeomFillRule, Mat2D, Point, Rect, VectorPath};
use dp_model::{BlendMode, Color};
use dp_text::TextEngine;
use tiny_skia::{
    BlendMode as TinyBlend, Color as TinyColor, FillRule as TinyFillRule, FilterQuality,
    GradientStop as TinyStop, IntSize, LineCap as TinyCap, LineJoin as TinyJoin, LinearGradient,
    Mask, Paint as TinyPaint, Path, PathBuilder, Pixmap, PixmapPaint, PixmapRef, RadialGradient,
    Shader, SpreadMode, Stroke as TinyStroke, StrokeDash, Transform,
};

use crate::scene::{DisplayList, SceneItem, ScenePaint, SceneShape, SceneStop, SceneStroke};
use crate::viewport::Viewport;

/// Largest supported render target edge.
pub const MAX_TARGET_EDGE: u32 = 16384;

/// Errors produced while rasterizing.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The requested target size is invalid or too large.
    #[error("invalid render target size {0}x{1}")]
    InvalidSize(u32, u32),
    /// PNG encoding failed.
    #[error("png encoding failed: {0}")]
    Png(String),
}

/// A rendered frame: premultiplied RGBA8 pixels.
#[derive(Debug, Clone)]
pub struct RenderTarget {
    width: u32,
    height: u32,
    pixmap: Pixmap,
}

impl RenderTarget {
    /// Renders a display list at the viewport's device resolution.
    pub fn render(
        list: &DisplayList,
        viewport: &Viewport,
        text: &mut TextEngine,
    ) -> Result<RenderTarget, RenderError> {
        let (width, height) = viewport.device_size();
        if width == 0 || height == 0 || width > MAX_TARGET_EDGE || height > MAX_TARGET_EDGE {
            return Err(RenderError::InvalidSize(width, height));
        }
        let mut pixmap =
            Pixmap::new(width, height).ok_or(RenderError::InvalidSize(width, height))?;

        if list.background.a > 0.0 {
            pixmap.fill(to_tiny_color(list.background, 1.0));
        }

        let viewport_transform = to_tiny_transform(viewport.transform());
        let device_rect = Rect::new(0.0, 0.0, width as f64, height as f64);

        for item in &list.items {
            let screen_bounds = viewport.transform().apply_rect(item.world_bounds);
            if !intersects(screen_bounds, device_rect) {
                continue;
            }

            // A zero-area clip means the item is fully clipped away.
            let clip_mask = match item.clip {
                Some(clip) => {
                    let screen = viewport.transform().apply_rect(clip);
                    if screen.w <= 0.0 || screen.h <= 0.0 {
                        continue;
                    }
                    clip_mask(&screen, width, height)
                }
                None => None,
            };

            draw_item(
                &mut pixmap,
                item,
                viewport_transform,
                clip_mask.as_ref(),
                text,
            );
        }

        Ok(RenderTarget {
            width,
            height,
            pixmap,
        })
    }

    /// Target width in device pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Target height in device pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Premultiplied RGBA8 pixels (as stored by tiny-skia).
    pub fn data_premultiplied(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Straight-alpha RGBA8 pixels (for the IPC contract and PNG export).
    pub fn data_straight_rgba8(&self) -> Vec<u8> {
        let src = self.pixmap.data();
        let mut out = Vec::with_capacity(src.len());
        for px in src.as_chunks::<4>().0 {
            let a = px[3] as u32;
            let un_premultiply = |channel: u8| -> u8 {
                (channel as u32 * 255 + a / 2)
                    .checked_div(a)
                    .unwrap_or(0)
                    .min(255) as u8
            };
            out.extend_from_slice(&[
                un_premultiply(px[0]),
                un_premultiply(px[1]),
                un_premultiply(px[2]),
                a as u8,
            ]);
        }
        out
    }

    /// Encodes the frame as PNG bytes.
    pub fn encode_png(&self) -> Result<Vec<u8>, RenderError> {
        self.pixmap
            .encode_png()
            .map_err(|err| RenderError::Png(err.to_string()))
    }

    /// Writes the frame to a PNG file.
    pub fn save_png(&self, path: impl AsRef<std::path::Path>) -> Result<(), RenderError> {
        let bytes = self.encode_png()?;
        std::fs::write(path, bytes).map_err(|err| RenderError::Png(err.to_string()))
    }

    /// The backing pixmap.
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// Premultiplied RGBA at a device pixel.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        let data = self.pixmap.data();
        Some([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]])
    }

    /// Straight-alpha RGBA at a device pixel.
    pub fn pixel_straight(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        let p = self.pixel(x, y)?;
        let a = p[3] as u32;
        if a == 0 {
            return Some([0, 0, 0, 0]);
        }
        Some([
            ((p[0] as u32 * 255 + a / 2) / a).min(255) as u8,
            ((p[1] as u32 * 255 + a / 2) / a).min(255) as u8,
            ((p[2] as u32 * 255 + a / 2) / a).min(255) as u8,
            a as u8,
        ])
    }
}

/// Renders a display list into a new target.
pub fn rasterize(
    list: &DisplayList,
    viewport: &Viewport,
    text: &mut TextEngine,
) -> Result<RenderTarget, RenderError> {
    RenderTarget::render(list, viewport, text)
}

fn draw_item(
    target: &mut Pixmap,
    item: &SceneItem,
    viewport_transform: Transform,
    mask: Option<&Mask>,
    text: &mut TextEngine,
) {
    let world = to_tiny_transform(item.matrix);
    // tiny-skia `pre_concat(a)` returns `viewport * a`, i.e. apply `a` first and
    // then the viewport, which is the world -> screen order we need.
    let transform = viewport_transform.pre_concat(world);
    let blend = to_tiny_blend(item.blend);

    if let SceneShape::Path(path) = &item.shape
        && let Some(tiny_path) = to_tiny_path(path)
    {
        for shadow in &item.shadows {
            draw_shadow(target, item, shadow, &tiny_path, viewport_transform, mask);
        }
        for fill in &item.fills {
            if let Some(shader) = to_shader(&fill.paint, fill.opacity) {
                let paint = TinyPaint {
                    shader,
                    blend_mode: blend,
                    anti_alias: true,
                    ..TinyPaint::default()
                };
                target.fill_path(
                    &tiny_path,
                    &paint,
                    to_tiny_fill_rule(fill.rule),
                    transform,
                    mask,
                );
            }
        }
        for stroke in &item.strokes {
            if let Some(shader) = to_shader(&stroke.paint, stroke.opacity) {
                let paint = TinyPaint {
                    shader,
                    blend_mode: blend,
                    anti_alias: true,
                    ..TinyPaint::default()
                };
                let tiny_stroke = to_tiny_stroke(stroke);
                target.stroke_path(&tiny_path, &paint, &tiny_stroke, transform, mask);
            }
        }
    }

    if let Some(text_draw) = &item.text {
        draw_text(target, item, text_draw, transform, blend, mask, text);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    target: &mut Pixmap,
    item: &SceneItem,
    text_draw: &crate::scene::SceneText,
    item_transform: Transform,
    blend: TinyBlend,
    mask: Option<&Mask>,
    text: &mut TextEngine,
) {
    for line in &text_draw.layout.lines {
        for glyph in &line.glyphs {
            let Some(bitmap) = text.rasterize_glyph(glyph) else {
                continue;
            };
            let Some(pixmap) = tint_bitmap(
                bitmap.width,
                bitmap.height,
                &bitmap.data,
                text_draw.color,
                item.opacity,
            ) else {
                continue;
            };
            let gx = (glyph.x + bitmap.left as f64) as f32;
            let gy = (glyph.y - bitmap.top as f64) as f32;
            let glyph_transform = item_transform.pre_concat(Transform::from_translate(gx, gy));
            let paint = PixmapPaint {
                opacity: 1.0,
                blend_mode: blend,
                quality: FilterQuality::Nearest,
            };
            target.draw_pixmap(
                0,
                0,
                PixmapRef::from_bytes(pixmap.data(), bitmap.width, bitmap.height)
                    .expect("bitmap dimensions are validated"),
                &paint,
                glyph_transform,
                mask,
            );
        }
    }
}

/// Tints a straight-alpha glyph RGBA bitmap into a premultiplied pixmap.
fn tint_bitmap(width: u32, height: u32, data: &[u8], color: Color, opacity: f64) -> Option<Pixmap> {
    let pixels = width as usize * height as usize;
    if width == 0 || height == 0 || data.len() < pixels * 4 {
        return None;
    }
    let opacity = opacity.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(pixels * 4);
    for px in data[..pixels * 4].as_chunks::<4>().0 {
        let coverage = px[3] as f64 / 255.0;
        let alpha = (color.a * opacity * coverage).clamp(0.0, 1.0);
        let premul_r = color.r * alpha;
        let premul_g = color.g * alpha;
        let premul_b = color.b * alpha;
        out.extend_from_slice(&[
            (premul_r * 255.0).round() as u8,
            (premul_g * 255.0).round() as u8,
            (premul_b * 255.0).round() as u8,
            (alpha * 255.0).round() as u8,
        ]);
    }
    let size = IntSize::from_wh(width, height)?;
    Pixmap::from_vec(out, size)
}

/// Converts a vector path into a tiny-skia path, preserving cubic segments.
pub fn to_tiny_path(path: &VectorPath) -> Option<Path> {
    if path.contours.is_empty() {
        return None;
    }
    let mut builder = PathBuilder::new();
    let mut any = false;
    for contour in &path.contours {
        let anchors = &contour.anchors;
        if anchors.is_empty() {
            continue;
        }
        let first = &anchors[0];
        builder.move_to(first.point.x as f32, first.point.y as f32);
        any = true;
        let count = anchors.len();
        let segments = if contour.closed {
            count
        } else {
            count.saturating_sub(1)
        };
        for i in 0..segments {
            let from = &anchors[i];
            let to = &anchors[(i + 1) % count];
            builder.cubic_to(
                from.out_handle.x as f32,
                from.out_handle.y as f32,
                to.in_handle.x as f32,
                to.in_handle.y as f32,
                to.point.x as f32,
                to.point.y as f32,
            );
        }
        if contour.closed {
            builder.close();
        }
    }
    if any { builder.finish() } else { None }
}

fn to_shader(paint: &ScenePaint, opacity: f64) -> Option<Shader<'static>> {
    match paint {
        ScenePaint::Solid(color) => Some(Shader::SolidColor(to_tiny_color(*color, opacity))),
        ScenePaint::Linear { from, to, stops } => LinearGradient::new(
            tiny_point(*from),
            tiny_point(*to),
            tiny_stops(stops, opacity),
            SpreadMode::Pad,
            Transform::identity(),
        ),
        ScenePaint::Radial {
            center,
            radius,
            stops,
        } => RadialGradient::new(
            tiny_point(*center),
            0.0,
            tiny_point(*center),
            radius.max(0.0) as f32,
            tiny_stops(stops, opacity),
            SpreadMode::Pad,
            Transform::identity(),
        ),
    }
}

fn tiny_stops(stops: &[SceneStop], opacity: f64) -> Vec<TinyStop> {
    if stops.is_empty() {
        return vec![
            TinyStop::new(0.0, TinyColor::BLACK),
            TinyStop::new(1.0, TinyColor::BLACK),
        ];
    }
    stops
        .iter()
        .map(|stop| TinyStop::new(stop.position as f32, to_tiny_color(stop.color, opacity)))
        .collect()
}

fn to_tiny_stroke(stroke: &SceneStroke) -> TinyStroke {
    TinyStroke {
        width: stroke.weight.max(0.0) as f32,
        miter_limit: stroke.miter_limit.max(1.0) as f32,
        line_cap: match stroke.cap {
            dp_model::LineCap::Butt => TinyCap::Butt,
            dp_model::LineCap::Round => TinyCap::Round,
            dp_model::LineCap::Square => TinyCap::Square,
        },
        line_join: match stroke.join {
            dp_model::LineJoin::Miter => TinyJoin::Miter,
            dp_model::LineJoin::Round => TinyJoin::Round,
            dp_model::LineJoin::Bevel => TinyJoin::Bevel,
        },
        dash: stroke.dash.as_ref().and_then(|dash| {
            let values: Vec<f32> = dash.iter().map(|value| *value as f32).collect();
            StrokeDash::new(values, 0.0)
        }),
    }
}

fn clip_mask(screen: &Rect, width: u32, height: u32) -> Option<Mask> {
    let rect = tiny_skia::Rect::from_xywh(
        screen.x as f32,
        screen.y as f32,
        screen.w as f32,
        screen.h as f32,
    )?;
    let mut mask = Mask::new(width, height)?;
    mask.fill_path(
        &PathBuilder::from_rect(rect),
        TinyFillRule::Winding,
        true,
        Transform::identity(),
    );
    Some(mask)
}

/// Draws one drop shadow for an item: an offset, optionally blurred copy of the
/// outline filled with the shadow colour. Spread is approximated with an outline
/// stroke; text shadows and inner/background blurs are not implemented yet and
/// are warned by the builder.
#[allow(clippy::too_many_arguments)]
fn draw_shadow(
    target: &mut Pixmap,
    item: &SceneItem,
    shadow: &crate::scene::SceneShadow,
    path: &Path,
    viewport_transform: Transform,
    mask: Option<&Mask>,
) {
    let full = viewport_transform.pre_concat(to_tiny_transform(item.matrix));
    let scale = (full.sx * full.sx + full.ky * full.ky).sqrt().max(0.001);
    let expand = shadow.blur + shadow.spread + 2.0;
    let world = Rect::new(
        item.world_bounds.x + shadow.offset.x - expand,
        item.world_bounds.y + shadow.offset.y - expand,
        item.world_bounds.w + expand * 2.0,
        item.world_bounds.h + expand * 2.0,
    );
    let (min_x, min_y, max_x, max_y) = map_rect(viewport_transform, world);
    let left = min_x.floor().max(0.0);
    let top = min_y.floor().max(0.0);
    let right = max_x.ceil().min(target.width() as f32);
    let bottom = max_y.ceil().min(target.height() as f32);
    let width = (right - left).max(0.0) as u32;
    let height = (bottom - top).max(0.0) as u32;
    if width == 0 || height == 0 || (width as u64 * height as u64) > 16_000_000 {
        return;
    }
    let Some(mut shadow_pixmap) = Pixmap::new(width, height) else {
        return;
    };
    let offset = Transform::from_translate(
        (shadow.offset.x * scale as f64) as f32,
        (shadow.offset.y * scale as f64) as f32,
    );
    let local = Transform::from_translate(-left, -top)
        .pre_concat(offset)
        .pre_concat(full);
    let paint = TinyPaint {
        shader: Shader::SolidColor(to_tiny_color(shadow.color, item.opacity)),
        anti_alias: true,
        ..TinyPaint::default()
    };
    shadow_pixmap.fill_path(path, &paint, TinyFillRule::Winding, local, None);
    if shadow.spread > 0.5 {
        let spread_stroke = TinyStroke {
            width: (shadow.spread * scale as f64) as f32 * 2.0,
            ..TinyStroke::default()
        };
        shadow_pixmap.stroke_path(path, &paint, &spread_stroke, local, None);
    }
    let blur_radius = (shadow.blur as f32 * scale).round() as u32;
    if blur_radius >= 1 {
        blur_rgba(
            shadow_pixmap.data_mut(),
            width as usize,
            height as usize,
            blur_radius.min(128),
        );
    }
    let Some(source) = PixmapRef::from_bytes(shadow_pixmap.data(), width, height) else {
        return;
    };
    let pixmap_paint = PixmapPaint {
        opacity: 1.0,
        blend_mode: to_tiny_blend(shadow.blend),
        quality: FilterQuality::Bilinear,
    };
    target.draw_pixmap(
        left as i32,
        top as i32,
        source,
        &pixmap_paint,
        Transform::identity(),
        mask,
    );
}

/// Axis-aligned device rectangle (min_x, min_y, max_x, max_y) for a world rect.
fn map_rect(transform: Transform, rect: Rect) -> (f32, f32, f32, f32) {
    let corners = [
        (rect.x, rect.y),
        (rect.right(), rect.y),
        (rect.right(), rect.bottom()),
        (rect.x, rect.bottom()),
    ];
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in corners {
        let mut point = tiny_skia::Point::from_xy(x as f32, y as f32);
        transform.map_point(&mut point);
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    (min_x, min_y, max_x, max_y)
}

/// Separable triple box blur over premultiplied RGBA8 (approximates a gaussian).
fn blur_rgba(data: &mut [u8], width: usize, height: usize, radius: u32) {
    if width < 2 || height < 2 || radius == 0 {
        return;
    }
    let radius = radius as usize;
    let mut scratch = vec![0u8; data.len()];
    for _ in 0..3 {
        box_blur_horizontal(data, &mut scratch, width, height, radius);
        box_blur_vertical(&scratch, data, width, height, radius);
    }
}

fn box_blur_horizontal(
    source: &[u8],
    destination: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
) {
    let window = (radius * 2 + 1) as u32;
    for y in 0..height {
        let row = y * width * 4;
        let mut sums = [0u32; 4];
        for x in 0..=(radius.min(width - 1)) {
            let idx = row + x * 4;
            for c in 0..4 {
                sums[c] += source[idx + c] as u32;
            }
        }
        for x in 0..width {
            let out = row + x * 4;
            for c in 0..4 {
                destination[out + c] = (sums[c] / window) as u8;
            }
            let add_x = (x + radius + 1).min(width - 1);
            let remove_x = x.saturating_sub(radius);
            let add = row + add_x * 4;
            let remove = row + remove_x * 4;
            for c in 0..4 {
                sums[c] = sums[c] + source[add + c] as u32 - source[remove + c] as u32;
            }
        }
    }
}

fn box_blur_vertical(
    source: &[u8],
    destination: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
) {
    let window = (radius * 2 + 1) as u32;
    for x in 0..width {
        let column = x * 4;
        let mut sums = [0u32; 4];
        for y in 0..=(radius.min(height - 1)) {
            let idx = y * width * 4 + column;
            for c in 0..4 {
                sums[c] += source[idx + c] as u32;
            }
        }
        for y in 0..height {
            let out = y * width * 4 + column;
            for c in 0..4 {
                destination[out + c] = (sums[c] / window) as u8;
            }
            let add_y = (y + radius + 1).min(height - 1);
            let remove_y = y.saturating_sub(radius);
            let add = add_y * width * 4 + column;
            let remove = remove_y * width * 4 + column;
            for c in 0..4 {
                sums[c] = sums[c] + source[add + c] as u32 - source[remove + c] as u32;
            }
        }
    }
}

fn to_tiny_transform(matrix: Mat2D) -> Transform {
    Transform::from_row(
        matrix.m11 as f32,
        matrix.m12 as f32,
        matrix.m21 as f32,
        matrix.m22 as f32,
        matrix.tx as f32,
        matrix.ty as f32,
    )
}

fn tiny_point(point: Point) -> tiny_skia::Point {
    tiny_skia::Point::from_xy(point.x as f32, point.y as f32)
}

fn to_tiny_color(color: Color, opacity: f64) -> TinyColor {
    let alpha = (color.a * opacity).clamp(0.0, 1.0);
    TinyColor::from_rgba8(
        to_byte(color.r),
        to_byte(color.g),
        to_byte(color.b),
        to_byte(alpha),
    )
}

fn to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn to_tiny_fill_rule(rule: GeomFillRule) -> TinyFillRule {
    match rule {
        GeomFillRule::NonZero => TinyFillRule::Winding,
        GeomFillRule::EvenOdd => TinyFillRule::EvenOdd,
    }
}

fn to_tiny_blend(blend: BlendMode) -> TinyBlend {
    match blend {
        BlendMode::Normal | BlendMode::PassThrough => TinyBlend::SourceOver,
        BlendMode::Multiply => TinyBlend::Multiply,
        BlendMode::Screen => TinyBlend::Screen,
        BlendMode::Overlay => TinyBlend::Overlay,
        BlendMode::Darken => TinyBlend::Darken,
        BlendMode::Lighten => TinyBlend::Lighten,
        BlendMode::ColorDodge => TinyBlend::ColorDodge,
        BlendMode::ColorBurn => TinyBlend::ColorBurn,
        BlendMode::HardLight => TinyBlend::HardLight,
        BlendMode::SoftLight => TinyBlend::SoftLight,
        BlendMode::Difference => TinyBlend::Difference,
        BlendMode::Exclusion => TinyBlend::Exclusion,
        BlendMode::Hue => TinyBlend::Hue,
        BlendMode::Saturation => TinyBlend::Saturation,
        BlendMode::Color => TinyBlend::Color,
        BlendMode::Luminosity => TinyBlend::Luminosity,
    }
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
}

/// Ensures the `Arc` import is exercised for callers that wrap layouts.
#[allow(dead_code)]
fn layout_arc(layout: dp_text::TextLayout) -> Arc<dp_text::TextLayout> {
    Arc::new(layout)
}
