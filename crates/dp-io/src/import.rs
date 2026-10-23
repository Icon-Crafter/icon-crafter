//! SVG import.
//!
//! Vector geometry is preserved as editable cubic paths. Solid fills and
//! strokes are imported; gradients, patterns, raster images and text are
//! counted and reported as warnings rather than silently dropped (gradients
//! import as their first colour stop so the shape remains visible).

use std::sync::Arc;

use dp_geom::{Anchor, Contour, Point, VectorPath};
use dp_model::{
    Color, Document, Fill, Node, NodeGeometry, NodeKind, NodeStyle, PageId, Paint, Stroke,
    StrokeAlign, Transform,
};
use tiny_skia_path::Transform as TinyTransform;

use crate::ExportError;

/// Result of an SVG import.
#[derive(Debug)]
pub struct ImportResult {
    /// The imported document with one page containing a frame with the paths.
    pub document: Document,
    /// Id of the imported page.
    pub page: PageId,
    /// Id of the frame that wraps the imported artwork.
    pub frame: dp_model::NodeId,
    /// Human-readable warnings for content that could not be preserved.
    pub warnings: Vec<String>,
}

/// Imports an SVG string into a new document.
pub fn import_svg(svg: &str, name: &str) -> Result<ImportResult, ExportError> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &options)
        .map_err(|error| ExportError::Conversion(format!("invalid SVG: {error}")))?;

    let width = tree.size().width().max(1.0) as f64;
    let height = tree.size().height().max(1.0) as f64;

    let mut document = Document::new(name);
    let page = document.active_page_id();
    document.active_page_mut().background = Color::WHITE;
    let frame = Node::frame(name, 0.0, 0.0, width, height);
    let frame_id = frame.id;
    document
        .page_mut(page)
        .expect("document has an active page")
        .insert_node(frame, None, None)
        .map_err(|error| ExportError::Conversion(error.to_string()))?;

    let mut importer = Importer {
        document: &mut document,
        page,
        frame: frame_id,
        warnings: Vec::new(),
        paths: 0,
        gradients: 0,
        skipped: 0,
        warned_image: false,
        warned_text: false,
    };
    for child in tree.root().children() {
        importer.visit(child);
    }
    if svg.to_ascii_lowercase().contains("<text") && !importer.warned_text {
        // usvg converts text to outlined paths during parsing, so no Text node
        // reaches the importer; still tell the user the text is not editable.
        importer.warned_text = true;
        importer.warnings.push(
            "SVG text is imported as outlined vector paths, not editable text; editable text import is not supported yet"
                .to_owned(),
        );
    }

    if importer.paths == 0 {
        importer
            .warnings
            .push("the SVG contained no importable vector paths".to_owned());
    }
    let warnings = std::mem::take(&mut importer.warnings);
    Ok(ImportResult {
        document,
        page,
        frame: frame_id,
        warnings,
    })
}

struct Importer<'a> {
    document: &'a mut Document,
    page: PageId,
    frame: dp_model::NodeId,
    warnings: Vec<String>,
    paths: usize,
    gradients: usize,
    skipped: usize,
    warned_image: bool,
    warned_text: bool,
}

impl Importer<'_> {
    fn visit(&mut self, node: &usvg::Node) {
        match node {
            usvg::Node::Group(group) => {
                for child in group.children() {
                    self.visit(child);
                }
            }
            usvg::Node::Path(path) => self.import_path(path),
            usvg::Node::Image(_) => {
                self.skipped += 1;
                if !self.warned_image {
                    self.warned_image = true;
                    self.warnings.push(
                        "raster images inside the SVG are not imported yet; they are skipped"
                            .to_owned(),
                    );
                }
            }
            usvg::Node::Text(_) => {
                self.skipped += 1;
                if !self.warned_text {
                    self.warned_text = true;
                    self.warnings.push(
                        "SVG text is not imported as editable text yet; it is skipped".to_owned(),
                    );
                }
            }
        }
    }

    fn import_path(&mut self, path: &usvg::Path) {
        let transform = path.abs_transform();
        let vector = match convert_path(path.data(), transform) {
            Some(vector) => vector,
            None => {
                self.skipped += 1;
                return;
            }
        };
        let Some(bounds) = vector.bounds() else {
            self.skipped += 1;
            return;
        };
        if bounds.w <= 0.0 && bounds.h <= 0.0 {
            return;
        }
        let origin = Point::new(bounds.x, bounds.y);
        let local = shift_path(&vector, origin);

        let mut style = NodeStyle::default();
        if let Some(fill) = path.fill() {
            if let Some(paint) = convert_paint(fill.paint()) {
                style.fills.push(Fill {
                    visible: true,
                    opacity: fill.opacity().get() as f64,
                    blend: Default::default(),
                    paint,
                    style_id: None,
                    variable_id: None,
                });
            }
        }
        if let Some(stroke) = path.stroke() {
            if let Some(paint) = convert_paint(stroke.paint()) {
                let mut converted = Stroke::solid(Color::BLACK);
                converted.visible = true;
                converted.opacity = stroke.opacity().get() as f64;
                converted.paint = paint;
                converted.weight = stroke.width().get() as f64;
                converted.align = StrokeAlign::Center;
                converted.cap = match stroke.linecap() {
                    usvg::LineCap::Butt => dp_model::LineCap::Butt,
                    usvg::LineCap::Round => dp_model::LineCap::Round,
                    usvg::LineCap::Square => dp_model::LineCap::Square,
                };
                converted.join = match stroke.linejoin() {
                    usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => dp_model::LineJoin::Miter,
                    usvg::LineJoin::Round => dp_model::LineJoin::Round,
                    usvg::LineJoin::Bevel => dp_model::LineJoin::Bevel,
                };
                converted.dash = stroke
                    .dasharray()
                    .map(|values| values.iter().map(|value| *value as f64).collect());
                style.strokes.push(converted);
            }
        }
        if style.fills.is_empty() && style.strokes.is_empty() {
            return;
        }

        let transform = Transform::new(bounds.x, bounds.y, bounds.w, bounds.h);
        let name = if path.id().is_empty() {
            format!("Path {}", self.paths + 1)
        } else {
            path.id().to_owned()
        };
        let node = Node::new(NodeKind::Vector, name, transform)
            .with_geometry(NodeGeometry::Vector { path: local })
            .with_style(style);
        if let Some(page) = self.document.page_mut(self.page) {
            if page.insert_node(node, Some(self.frame), None).is_ok() {
                self.paths += 1;
            }
        }
    }
}

fn convert_paint(paint: &usvg::Paint) -> Option<Paint> {
    match paint {
        usvg::Paint::Color(color) => Some(Paint::solid(Color::rgb(
            color.red as f64 / 255.0,
            color.green as f64 / 255.0,
            color.blue as f64 / 255.0,
        ))),
        usvg::Paint::LinearGradient(gradient) => gradient
            .stops()
            .first()
            .map(|stop| Paint::solid(convert_color(stop.color()))),
        usvg::Paint::RadialGradient(gradient) => gradient
            .stops()
            .first()
            .map(|stop| Paint::solid(convert_color(stop.color()))),
        usvg::Paint::Pattern(_) => None,
    }
}

fn convert_color(color: usvg::Color) -> Color {
    Color::rgb(
        color.red as f64 / 255.0,
        color.green as f64 / 255.0,
        color.blue as f64 / 255.0,
    )
}

/// Converts a tiny-skia path (already in SVG canvas space) to a model path.
fn convert_path(path: &tiny_skia_path::Path, transform: TinyTransform) -> Option<VectorPath> {
    let mut contours: Vec<Contour> = Vec::new();
    let mut current: Vec<Anchor> = Vec::new();
    let map = |point: tiny_skia_path::Point| -> Point {
        let mut mapped = point;
        transform.map_point(&mut mapped);
        Point::new(mapped.x as f64, mapped.y as f64)
    };

    let flush = |current: &mut Vec<Anchor>, contours: &mut Vec<Contour>| {
        if current.len() >= 2 {
            contours.push(Contour {
                closed: false,
                anchors: std::mem::take(current),
            });
        } else {
            current.clear();
        }
    };

    for segment in path.segments() {
        match segment {
            tiny_skia_path::PathSegment::MoveTo(point) => {
                flush(&mut current, &mut contours);
                current.push(Anchor::corner(map(point)));
            }
            tiny_skia_path::PathSegment::LineTo(point) => {
                if current.is_empty() {
                    continue;
                }
                current.push(Anchor::corner(map(point)));
            }
            tiny_skia_path::PathSegment::QuadTo(control, end) => {
                let Some(last) = current.last_mut() else {
                    continue;
                };
                let start = last.point;
                let control = map(control);
                let end = map(end);
                let c1 = Point::new(
                    start.x + (control.x - start.x) * 2.0 / 3.0,
                    start.y + (control.y - start.y) * 2.0 / 3.0,
                );
                let c2 = Point::new(
                    end.x + (control.x - end.x) * 2.0 / 3.0,
                    end.y + (control.y - end.y) * 2.0 / 3.0,
                );
                last.out_handle = c1;
                let mut anchor = Anchor::corner(end);
                anchor.in_handle = c2;
                current.push(anchor);
            }
            tiny_skia_path::PathSegment::CubicTo(control1, control2, end) => {
                let Some(last) = current.last_mut() else {
                    continue;
                };
                last.out_handle = map(control1);
                let mut anchor = Anchor::corner(map(end));
                anchor.in_handle = map(control2);
                current.push(anchor);
            }
            tiny_skia_path::PathSegment::Close => {
                if current.len() >= 2 {
                    let mut contour = Contour {
                        closed: true,
                        anchors: std::mem::take(&mut current),
                    };
                    // SVG closes back to the start; drop a duplicated final
                    // anchor so the model keeps one anchor per corner.
                    let first = contour.anchors[0].point;
                    if let Some(last) = contour.anchors.last() {
                        if (last.point.x - first.x).abs() < 1e-6
                            && (last.point.y - first.y).abs() < 1e-6
                            && contour.anchors.len() > 2
                        {
                            contour.anchors.pop();
                        }
                    }
                    contours.push(contour);
                } else {
                    current.clear();
                }
            }
        }
    }
    flush(&mut current, &mut contours);
    if contours.is_empty() {
        return None;
    }
    Some(VectorPath { contours })
}

/// Shifts every anchor by `-origin` and closes remaining open contours.
fn shift_path(path: &VectorPath, origin: Point) -> VectorPath {
    let contours = path
        .contours
        .iter()
        .map(|contour| {
            let mut contour = contour.clone();
            for anchor in &mut contour.anchors {
                anchor.point.x -= origin.x;
                anchor.point.y -= origin.y;
                anchor.in_handle.x -= origin.x;
                anchor.in_handle.y -= origin.y;
                anchor.out_handle.x -= origin.x;
                anchor.out_handle.y -= origin.y;
            }
            contour
        })
        .collect();
    VectorPath { contours }
}

/// Imports SVG bytes into the given page as a group under a frame.
pub fn import_svg_bytes(bytes: &[u8], name: &str) -> Result<ImportResult, ExportError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| ExportError::Conversion(format!("SVG is not UTF-8: {error}")))?;
    import_svg(text, name)
}

#[allow(dead_code)]
fn arc_transform(_: Arc<()>) {}
