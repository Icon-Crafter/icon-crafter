//! Path constructors for the primitive shape node kinds.
//!
//! Every primitive is expressed as editable bezier contours so that the pen
//! tools, boolean operations and the vector editor all operate on one model.
//! Primitive geometry is stored in node-local coordinates with the origin at
//! the node's top-left corner and no rotation applied (rotation lives in the
//! node transform).

use std::f64::consts::TAU;

use crate::path::{Anchor, Contour, VectorPath};
use crate::types::{CornerRadii, Point, Size};

/// Control point length of a cubic approximation of a quarter circle.
pub const KAPPA: f64 = 0.552_284_749_830_793_6;

/// Path of an axis aligned rectangle with per-corner radii.
pub fn rectangle_path(size: Size, radii: CornerRadii) -> VectorPath {
    let w = size.w.max(0.0);
    let h = size.h.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return VectorPath::new();
    }
    let r = radii.clamp_for(size);
    if r.is_zero() {
        return VectorPath::polygon([
            Point::new(0.0, 0.0),
            Point::new(w, 0.0),
            Point::new(w, h),
            Point::new(0.0, h),
        ]);
    }

    let k = KAPPA;
    let mut anchors = Vec::with_capacity(8);
    // Top-left corner end / top edge start.
    push_corner(
        &mut anchors,
        Point::new(r.tl, 0.0),
        Point::new(r.tl - k * r.tl, 0.0),
        Point::new(r.tl + k * r.tl, 0.0),
    );
    push_corner(
        &mut anchors,
        Point::new(w - r.tr, 0.0),
        Point::new(w - r.tr - k * r.tr, 0.0),
        Point::new(w - r.tr + k * r.tr, 0.0),
    );
    push_corner(
        &mut anchors,
        Point::new(w, r.tr),
        Point::new(w, r.tr - k * r.tr),
        Point::new(w, r.tr + k * r.tr),
    );
    push_corner(
        &mut anchors,
        Point::new(w, h - r.br),
        Point::new(w, h - r.br - k * r.br),
        Point::new(w, h - r.br + k * r.br),
    );
    push_corner(
        &mut anchors,
        Point::new(w - r.br, h),
        Point::new(w - r.br + k * r.br, h),
        Point::new(w - r.br - k * r.br, h),
    );
    push_corner(
        &mut anchors,
        Point::new(r.bl, h),
        Point::new(r.bl + k * r.bl, h),
        Point::new(r.bl - k * r.bl, h),
    );
    push_corner(
        &mut anchors,
        Point::new(0.0, h - r.bl),
        Point::new(0.0, h - r.bl + k * r.bl),
        Point::new(0.0, h - r.bl - k * r.bl),
    );
    push_corner(
        &mut anchors,
        Point::new(0.0, r.tl),
        Point::new(0.0, r.tl + k * r.tl),
        Point::new(0.0, r.tl - k * r.tl),
    );

    // Corners whose radius is zero produce duplicate anchors; drop them.
    let mut deduped: Vec<Anchor> = Vec::with_capacity(anchors.len());
    for anchor in anchors {
        match deduped.last() {
            Some(last) if last.point.distance_sq(anchor.point) <= 1e-14 => {
                // Merge handles so the remaining anchor keeps a valid corner.
                let last = deduped.last_mut().expect("checked above");
                last.out_handle = anchor.out_handle;
                if last.in_handle == last.point {
                    last.in_handle = anchor.in_handle;
                }
            }
            _ => deduped.push(anchor),
        }
    }
    if deduped.len() >= 2 {
        let first = deduped[0];
        let last = deduped.last().expect("non-empty");
        if first.point.distance_sq(last.point) <= 1e-14 {
            let last = deduped.pop().expect("non-empty");
            deduped[0].in_handle = last.in_handle;
        }
    }
    VectorPath::from_contour(Contour::closed(deduped))
}

fn push_corner(anchors: &mut Vec<Anchor>, point: Point, in_handle: Point, out_handle: Point) {
    anchors.push(Anchor {
        point,
        in_handle,
        out_handle,
        smooth: false,
    });
}

/// Path of an ellipse inscribed in the given size.
pub fn ellipse_path(size: Size) -> VectorPath {
    let w = size.w.max(0.0);
    let h = size.h.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return VectorPath::new();
    }
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (rx, ry) = (w / 2.0, h / 2.0);
    let k = KAPPA;
    // Clockwise: right -> bottom -> left -> top.
    let anchors = vec![
        Anchor {
            point: Point::new(cx + rx, cy),
            in_handle: Point::new(cx + rx, cy - k * ry),
            out_handle: Point::new(cx + rx, cy + k * ry),
            smooth: true,
        },
        Anchor {
            point: Point::new(cx, cy + ry),
            in_handle: Point::new(cx + k * rx, cy + ry),
            out_handle: Point::new(cx - k * rx, cy + ry),
            smooth: true,
        },
        Anchor {
            point: Point::new(cx - rx, cy),
            in_handle: Point::new(cx - rx, cy + k * ry),
            out_handle: Point::new(cx - rx, cy - k * ry),
            smooth: true,
        },
        Anchor {
            point: Point::new(cx, cy - ry),
            in_handle: Point::new(cx - k * rx, cy - ry),
            out_handle: Point::new(cx + k * rx, cy - ry),
            smooth: true,
        },
    ];
    VectorPath::from_contour(Contour::closed(anchors))
}

/// Path of a line. The segment runs from the top-left to the bottom-right corner
/// of the node's box; resize the box to change its angle and length.
pub fn line_path(size: Size) -> VectorPath {
    let w = size.w.max(0.0);
    let h = size.h.max(0.0);
    if w <= 0.0 && h <= 0.0 {
        return VectorPath::new();
    }
    VectorPath::from_contour(Contour::open(vec![
        Anchor::corner(Point::new(0.0, 0.0)),
        Anchor::corner(Point::new(w, h)),
    ]))
}

/// Path of a regular polygon inscribed in the given size, first vertex on top.
pub fn polygon_path(size: Size, sides: u32) -> VectorPath {
    let w = size.w.max(0.0);
    let h = size.h.max(0.0);
    let count = sides.clamp(3, 512) as usize;
    if w <= 0.0 || h <= 0.0 {
        return VectorPath::new();
    }
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (rx, ry) = (w / 2.0, h / 2.0);
    let step = TAU / count as f64;
    let points = (0..count).map(|i| {
        let angle = -TAU / 4.0 + step * i as f64;
        Point::new(cx + rx * angle.cos(), cy + ry * angle.sin())
    });
    VectorPath::polygon(points)
}

/// Path of a star with `points` alternating outer and inner vertices.
pub fn star_path(size: Size, points: u32, inner_ratio: f64) -> VectorPath {
    let w = size.w.max(0.0);
    let h = size.h.max(0.0);
    let count = points.clamp(3, 256) as usize;
    if w <= 0.0 || h <= 0.0 {
        return VectorPath::new();
    }
    let ratio = inner_ratio.clamp(0.05, 0.95);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let (rx, ry) = (w / 2.0, h / 2.0);
    let step = TAU / (count * 2) as f64;
    let vertices = (0..count * 2).map(|i| {
        let angle = -TAU / 4.0 + step * i as f64;
        let scale = if i % 2 == 0 { 1.0 } else { ratio };
        Point::new(cx + rx * scale * angle.cos(), cy + ry * scale * angle.sin())
    });
    VectorPath::polygon(vertices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Mat2D, Rect};

    fn assert_close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() <= tolerance, "{a} != {b} (tolerance {tolerance})");
    }

    #[test]
    fn rectangle_without_radii() {
        let path = rectangle_path(Size::new(100.0, 50.0), CornerRadii::ZERO);
        assert_eq!(path.anchor_count(), 4);
        assert_eq!(path.bounds().unwrap(), Rect::new(0.0, 0.0, 100.0, 50.0));
        assert_close(path.area(), 5000.0, 1e-6);
    }

    #[test]
    fn rounded_rectangle_bounds_and_area() {
        let path = rectangle_path(Size::new(100.0, 50.0), CornerRadii::uniform(10.0));
        let bounds = path.bounds().unwrap();
        assert_close(bounds.w, 100.0, 1e-6);
        assert_close(bounds.h, 50.0, 1e-6);
        // Area is smaller than the full rectangle but close to it.
        let area = path.area();
        assert!(area > 4000.0 && area < 5000.0, "unexpected area {area}");
        // Rounded corners leave the node's corner empty.
        assert!(!path.contains(Point::new(1.0, 1.0), crate::path::FillRule::NonZero));
        assert!(path.contains(Point::new(50.0, 25.0), crate::path::FillRule::NonZero));
    }

    #[test]
    fn ellipse_approximates_circle() {
        let path = ellipse_path(Size::new(100.0, 100.0));
        let area = path.area();
        let expected = std::f64::consts::PI * 50.0 * 50.0;
        // Polygonal flattening at 0.1 world units loses a few units of area;
        // 0.5% is far above the numerical error and far below a real shape bug.
        assert_close(area, expected, expected * 0.005);
        let bounds = path.bounds().unwrap();
        assert_close(bounds.w, 100.0, 1e-6);
        assert_close(bounds.h, 100.0, 1e-6);
        assert!(path.contains(Point::new(50.0, 50.0), crate::path::FillRule::NonZero));
        assert!(!path.contains(Point::new(5.0, 5.0), crate::path::FillRule::NonZero));
    }

    #[test]
    fn line_is_open() {
        let path = line_path(Size::new(100.0, 50.0));
        assert_eq!(path.contours.len(), 1);
        assert!(!path.contours[0].closed);
        assert_eq!(path.anchor_count(), 2);
        assert_eq!(path.contours[0].segments().len(), 1);
    }

    #[test]
    fn polygon_geometry() {
        let path = polygon_path(Size::new(100.0, 100.0), 6);
        assert_eq!(path.anchor_count(), 6);
        let bounds = path.bounds().unwrap();
        // A hexagon with vertices at the top and bottom is 100 units tall and
        // 86.6 units wide.
        assert_close(bounds.h, 100.0, 0.01);
        assert_close(bounds.w, 100.0 * (3.0f64).sqrt() / 2.0, 0.01);
        let area = path.area();
        let expected = 3.0 * (3.0f64).sqrt() / 2.0 * 50.0 * 50.0;
        assert_close(area, expected, expected * 0.001);
    }

    #[test]
    fn star_has_alternating_vertices() {
        let path = star_path(Size::new(100.0, 100.0), 5, 0.5);
        assert_eq!(path.anchor_count(), 10);
        let area = path.area();
        assert!(area > 0.0 && area < 10000.0);
    }

    #[test]
    fn transformed_shape() {
        let mut path = rectangle_path(Size::new(10.0, 10.0), CornerRadii::ZERO);
        path.transform(&Mat2D::translation(100.0, 200.0));
        assert_eq!(path.bounds().unwrap(), Rect::new(100.0, 200.0, 10.0, 10.0));
    }
}
