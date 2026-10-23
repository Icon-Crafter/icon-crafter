//! SVG export.
//!
//! The exporter serializes the display list: one `<g>` per node with its world
//! matrix, paths with cubic Béziers, solid/gradient fills, strokes, opacity and
//! text as real `<text>` elements (selectable and editable in vector tools).
//! Font metrics inside SVG are resolved by the client, so text placement can
//! differ slightly from the canvas; that limitation is documented in the README
//! and docs/FEATURE-CHECKLIST.md.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use dp_geom::{FillRule, Point, Rect, VectorPath};
use dp_model::{Color, Document, PageId};
use dp_render::{DisplayList, SceneItem, ScenePaint, SceneShape, SceneStroke, SceneText};
use dp_text::TextEngine;

use crate::{ExportError, ExportOptions};

/// Formats a number with at most `precision` decimals and no trailing zeros.
pub fn number(value: f64, precision: u8) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let mut text = format!("{:.*}", precision as usize, value);
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    text
}

/// Escapes text for XML content and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// A paint as written into SVG.
#[derive(Debug, Clone)]
enum SvgPaint {
    Solid(String),
    Linear {
        id: String,
        from: Point,
        to: Point,
        stops: Vec<(f64, Color)>,
    },
    Radial {
        id: String,
        center: Point,
        radius: f64,
        stops: Vec<(f64, Color)>,
    },
}

impl SvgPaint {
    fn reference(&self) -> String {
        match self {
            SvgPaint::Solid(hex) => hex.clone(),
            SvgPaint::Linear { id, .. } | SvgPaint::Radial { id, .. } => format!("url(#{id})"),
        }
    }
}

fn color_hex(color: Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        to_byte(color.r),
        to_byte(color.g),
        to_byte(color.b)
    )
}

fn to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Bundles gradient definitions with their SVG ids.
#[derive(Default)]
struct Defs {
    paints: BTreeMap<String, SvgPaint>,
    next_id: usize,
}

impl Defs {
    fn register(&mut self, paint: &ScenePaint, stops: Vec<(f64, Color)>) -> SvgPaint {
        match paint {
            ScenePaint::Solid(color) => SvgPaint::Solid(color_hex(*color)),
            ScenePaint::Linear { from, to, .. } => {
                self.next_id += 1;
                let id = format!("grad-{}", self.next_id);
                let value = SvgPaint::Linear {
                    id: id.clone(),
                    from: *from,
                    to: *to,
                    stops,
                };
                self.paints.insert(id, value.clone());
                value
            }
            ScenePaint::Radial { center, radius, .. } => {
                self.next_id += 1;
                let id = format!("grad-{}", self.next_id);
                let value = SvgPaint::Radial {
                    id: id.clone(),
                    center: *center,
                    radius: *radius,
                    stops,
                };
                self.paints.insert(id, value.clone());
                value
            }
        }
    }
}

/// Exports a page as an SVG string.
pub fn export_svg(
    document: &Document,
    page: PageId,
    options: &ExportOptions,
) -> Result<String, ExportError> {
    if document.page(page).is_none() {
        return Err(ExportError::MissingPage(page));
    }
    let mut text = TextEngine::new();
    let mut list = dp_render::SceneBuilder::new(document)
        .build(page, huge_visible(), &mut text)
        .map_err(ExportError::from)?;
    if let Some(background) = options.background {
        list.background = background;
    }

    let content = content_bounds(&list);
    if content.w <= 0.0 || content.h <= 0.0 {
        return Err(ExportError::EmptyDocument);
    }
    let bounds = options.bounds.or(options.clip).unwrap_or(content);
    let precision = options.precision.min(6);

    let mut defs = Defs::default();
    let mut body = String::new();
    for item in &list.items {
        write_item(&mut body, item, &mut defs, precision);
    }

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"{x} {y} {w} {h}\">",
        w = number(bounds.w, precision),
        h = number(bounds.h, precision),
        x = number(bounds.x, precision),
        y = number(bounds.y, precision),
    );
    if !defs.paints.is_empty() {
        let _ = writeln!(svg, "  <defs>");
        for paint in defs.paints.values() {
            match paint {
                SvgPaint::Solid(_) => {}
                SvgPaint::Linear {
                    id,
                    from,
                    to,
                    stops,
                    ..
                } => {
                    let _ = writeln!(
                        svg,
                        "    <linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\">",
                        x1 = number(from.x, precision),
                        y1 = number(from.y, precision),
                        x2 = number(to.x, precision),
                        y2 = number(to.y, precision),
                    );
                    write_stops(&mut svg, stops, precision);
                    let _ = writeln!(svg, "    </linearGradient>");
                }
                SvgPaint::Radial {
                    id,
                    center,
                    radius,
                    stops,
                    ..
                } => {
                    let _ = writeln!(
                        svg,
                        "    <radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\">",
                        cx = number(center.x, precision),
                        cy = number(center.y, precision),
                        r = number(*radius, precision),
                    );
                    write_stops(&mut svg, stops, precision);
                    let _ = writeln!(svg, "    </radialGradient>");
                }
            }
        }
        let _ = writeln!(svg, "  </defs>");
    }

    if let Some(background) = options.background
        && background.a > 0.0
    {
        let _ = writeln!(
            svg,
            "  <rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{fill}\" fill-opacity=\"{alpha}\"/>",
            x = number(bounds.x, precision),
            y = number(bounds.y, precision),
            w = number(bounds.w, precision),
            h = number(bounds.h, precision),
            fill = color_hex(background),
            alpha = number(background.a, 3),
        );
    }

    svg.push_str(&body);
    let _ = writeln!(svg, "</svg>");
    Ok(svg)
}

fn write_stops(svg: &mut String, stops: &[(f64, Color)], precision: u8) {
    for (position, color) in stops {
        let _ = writeln!(
            svg,
            "      <stop offset=\"{p}\" stop-color=\"{c}\" stop-opacity=\"{o}\"/>",
            p = number(*position, precision),
            c = color_hex(*color),
            o = number(color.a, 3),
        );
    }
}

fn write_item(body: &mut String, item: &SceneItem, defs: &mut Defs, precision: u8) {
    let matrix = item.matrix;
    let _ = writeln!(
        body,
        "  <g id=\"node-{id}\" transform=\"matrix({m11} {m12} {m21} {m22} {tx} {ty})\" opacity=\"{opacity}\">",
        id = item.node_id,
        m11 = number(matrix.m11, precision),
        m12 = number(matrix.m12, precision),
        m21 = number(matrix.m21, precision),
        m22 = number(matrix.m22, precision),
        tx = number(matrix.tx, precision),
        ty = number(matrix.ty, precision),
        opacity = number(item.opacity, 3),
    );

    if let SceneShape::Path(path) = &item.shape {
        let data = path_to_svg(path, precision);
        if !data.is_empty() {
            for fill in &item.fills {
                let paint = defs.register(&fill.paint, scene_stops_from_scene(&fill.paint));
                let _ = writeln!(
                    body,
                    "    <path d=\"{d}\" fill=\"{fill}\" fill-rule=\"{rule}\" fill-opacity=\"{opacity}\"/>",
                    d = data,
                    fill = paint.reference(),
                    rule = if fill.rule == FillRule::EvenOdd {
                        "evenodd"
                    } else {
                        "nonzero"
                    },
                    opacity = number(fill.opacity, 3),
                );
            }
            for stroke in &item.strokes {
                let paint = defs.register(&stroke.paint, scene_stops_from_scene(&stroke.paint));
                let _ = writeln!(
                    body,
                    "    <path d=\"{d}\" fill=\"none\" stroke=\"{stroke}\" stroke-opacity=\"{opacity}\" stroke-width=\"{width}\" stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\"{dash}/>",
                    d = data,
                    stroke = paint.reference(),
                    opacity = number(stroke.opacity, 3),
                    width = number(stroke.weight, precision),
                    cap = cap_name(stroke),
                    join = join_name(stroke),
                    dash = dash_attribute(stroke, precision),
                );
            }
        }
    }

    if let Some(text) = &item.text {
        write_text(body, text, precision);
    }

    let _ = writeln!(body, "  </g>");
}

fn scene_stops_from_scene(paint: &ScenePaint) -> Vec<(f64, Color)> {
    match paint {
        ScenePaint::Solid(_) => Vec::new(),
        ScenePaint::Linear { stops, .. } | ScenePaint::Radial { stops, .. } => {
            stops.iter().map(|s| (s.position, s.color)).collect()
        }
    }
}

fn cap_name(stroke: &SceneStroke) -> &'static str {
    match stroke.cap {
        dp_model::LineCap::Butt => "butt",
        dp_model::LineCap::Round => "round",
        dp_model::LineCap::Square => "square",
    }
}

fn join_name(stroke: &SceneStroke) -> &'static str {
    match stroke.join {
        dp_model::LineJoin::Miter => "miter",
        dp_model::LineJoin::Round => "round",
        dp_model::LineJoin::Bevel => "bevel",
    }
}

fn dash_attribute(stroke: &SceneStroke, precision: u8) -> String {
    match &stroke.dash {
        Some(values) if !values.is_empty() => format!(
            " stroke-dasharray=\"{}\"",
            values
                .iter()
                .map(|value| number(*value, precision))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        _ => String::new(),
    }
}

fn write_text(body: &mut String, text: &SceneText, precision: u8) {
    for line in &text.layout.lines {
        // SVG text is placed by its baseline; alignment is encoded with
        // `text-anchor` where possible.
        let _ = writeln!(
            body,
            "    <text x=\"{x}\" y=\"{y}\" font-family=\"{family}\" font-size=\"{size}\" font-weight=\"{weight}\" font-style=\"{style}\" letter-spacing=\"{spacing}\" fill=\"{fill}\" fill-opacity=\"{opacity}\" xml:space=\"preserve\">{content}</text>",
            x = number(0.0, precision),
            y = number(line.baseline, precision),
            family = escape(
                text.layout
                    .used_family
                    .as_deref()
                    .unwrap_or(&text.layout.requested_family)
            ),
            size = number(text.layout.font_size as f64, precision),
            weight = text.layout.font_weight,
            style = if text.layout.italic {
                "italic"
            } else {
                "normal"
            },
            spacing = number(text.layout.letter_spacing, precision),
            fill = color_hex(text.color),
            opacity = number(text.color.a, 3),
            content = escape(&line.text),
        );
    }
}

/// Converts a vector path into an SVG path data string.
pub fn path_to_svg(path: &VectorPath, precision: u8) -> String {
    let mut data = String::new();
    for contour in &path.contours {
        if contour.anchors.is_empty() {
            continue;
        }
        let first = &contour.anchors[0];
        let _ = write!(
            data,
            "M {} {} ",
            number(first.point.x, precision),
            number(first.point.y, precision)
        );
        let count = contour.anchors.len();
        let segments = if contour.closed {
            count
        } else {
            count.saturating_sub(1)
        };
        for index in 0..segments {
            let from = &contour.anchors[index];
            let to = &contour.anchors[(index + 1) % count];
            let _ = write!(
                data,
                "C {} {} {} {} {} {} ",
                number(from.out_handle.x, precision),
                number(from.out_handle.y, precision),
                number(to.in_handle.x, precision),
                number(to.in_handle.y, precision),
                number(to.point.x, precision),
                number(to.point.y, precision),
            );
        }
        if contour.closed {
            data.push('Z');
            data.push(' ');
        }
    }
    data.trim_end().to_owned()
}

/// Union of every display-list item's world bounds.
pub fn content_bounds(list: &DisplayList) -> Rect {
    let mut result: Option<Rect> = None;
    for item in &list.items {
        if item.world_bounds.w <= 0.0 || item.world_bounds.h <= 0.0 {
            continue;
        }
        result = Some(match result {
            Some(current) => current.union(item.world_bounds),
            None => item.world_bounds,
        });
    }
    result.unwrap_or_else(|| Rect::new(0.0, 0.0, 0.0, 0.0))
}

/// Visible rectangle that disables culling for export.
fn huge_visible() -> Rect {
    Rect::new(-1.0e9, -1.0e9, 2.0e9, 2.0e9)
}
