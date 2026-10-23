//! Boolean operations on filled paths.
//!
//! The exact boolean is computed on flattened polygons using `i_overlay`. The
//! result is therefore an editable polygon (curves are sampled at the requested
//! tolerance). This is an intentional, documented limitation: exact bezier
//! boolean operations are a hard problem and a polygonal result with a fine
//! tolerance is what production tools also produce for shape combination.
//!
//! The result preserves contour winding so that holes render correctly under
//! the non-zero fill rule.

use i_overlay::core::fill_rule::FillRule as IoFillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::path::{simplify_collinear, Anchor, Contour, FillRule, VectorPath};

/// Default flattening tolerance (world units) for boolean operations.
pub const DEFAULT_BOOLEAN_TOLERANCE: f64 = 0.02;

/// The four boolean operations exposed by the design editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BooleanOp {
    /// Combine both shapes.
    Union,
    /// Remove the second shape from the first one.
    Subtract,
    /// Keep the overlapping area only.
    Intersect,
    /// Keep the non-overlapping areas.
    Exclude,
}

/// Errors that can occur during boolean operations.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum BooleanError {
    /// Input geometry contained NaN or infinite coordinates.
    #[error("path contains non-finite coordinates")]
    NonFinite,
}

/// Performs a boolean operation with the default tolerance.
pub fn boolean_op(
    a: &VectorPath,
    b: &VectorPath,
    op: BooleanOp,
    fill_rule: FillRule,
) -> Result<VectorPath, BooleanError> {
    boolean_op_with_tolerance(a, b, op, fill_rule, DEFAULT_BOOLEAN_TOLERANCE)
}

/// Performs a boolean operation with an explicit flattening tolerance.
pub fn boolean_op_with_tolerance(
    a: &VectorPath,
    b: &VectorPath,
    op: BooleanOp,
    fill_rule: FillRule,
    tolerance: f64,
) -> Result<VectorPath, BooleanError> {
    let tolerance = tolerance.clamp(1e-4, 10.0);
    if !paths_are_finite(a) || !paths_are_finite(b) {
        return Err(BooleanError::NonFinite);
    }
    if a.is_empty() && b.is_empty() {
        return Ok(VectorPath::new());
    }
    let subject = to_contours(a, tolerance);
    let clip = to_contours(b, tolerance);
    if subject.is_empty() && clip.is_empty() {
        return Ok(VectorPath::new());
    }
    let rule = match op {
        BooleanOp::Union => OverlayRule::Union,
        BooleanOp::Subtract => OverlayRule::Difference,
        BooleanOp::Intersect => OverlayRule::Intersect,
        BooleanOp::Exclude => OverlayRule::Xor,
    };
    let io_fill = match fill_rule {
        FillRule::NonZero => IoFillRule::NonZero,
        FillRule::EvenOdd => IoFillRule::EvenOdd,
    };
    let shapes = if subject.is_empty() {
        // Boolean with an empty subject: union/intersection emit the clip while
        // subtract yields nothing; i_overlay handles empty inputs, so this is
        // only here to keep the call sites straightforward.
        let empty: Vec<Vec<[f64; 2]>> = Vec::new();
        empty.overlay(&clip, rule, io_fill)
    } else if clip.is_empty() {
        subject.overlay(&Vec::<Vec<[f64; 2]>>::new(), rule, io_fill)
    } else {
        subject.overlay(&clip, rule, io_fill)
    };
    Ok(from_io_shapes(shapes))
}

fn paths_are_finite(path: &VectorPath) -> bool {
    path.contours.iter().all(|c| {
        c.anchors.iter().all(|a| {
            a.point.is_finite() && a.in_handle.is_finite() && a.out_handle.is_finite()
        })
    })
}

fn to_contours(path: &VectorPath, tolerance: f64) -> Vec<Vec<[f64; 2]>> {
    let mut out = Vec::with_capacity(path.contours.len());
    for contour in &path.contours {
        let mut points = contour.flatten(tolerance);
        // Remove a duplicated closing point (i_overlay closes contours itself).
        if points.len() >= 2 {
            let first = points[0];
            let last = points[points.len() - 1];
            if first.distance_sq(last) <= 1e-12 {
                points.pop();
            }
        }
        if points.len() < 3 {
            continue;
        }
        let ring: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
        out.push(ring);
    }
    out
}

fn from_io_shapes(shapes: Vec<Vec<Vec<[f64; 2]>>>) -> VectorPath {
    let mut contours = Vec::new();
    for shape in shapes {
        for ring in shape {
            let points: Vec<crate::types::Point> = ring
                .iter()
                .map(|p| crate::types::Point::new(p[0], p[1]))
                .collect();
            let simplified = simplify_collinear(points, 1e-5);
            if simplified.len() < 3 {
                continue;
            }
            let anchors = simplified.into_iter().map(Anchor::corner).collect();
            contours.push(Contour::closed(anchors));
        }
    }
    VectorPath { contours }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{ellipse_path, rectangle_path};
    use crate::types::{CornerRadii, Mat2D, Point, Size};

    fn rect(x: f64, y: f64, w: f64, h: f64) -> VectorPath {
        let mut path = rectangle_path(Size::new(w, h), CornerRadii::ZERO);
        path.transform(&Mat2D::translation(x, y));
        path
    }

    #[test]
    fn union_of_overlapping_rectangles() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 10.0, 10.0);
        let result = boolean_op(&a, &b, BooleanOp::Union, FillRule::NonZero).unwrap();
        assert!((result.area() - 150.0).abs() < 1e-6, "area {}", result.area());
        assert_eq!(result.bounds().unwrap(), crate::types::Rect::new(0.0, 0.0, 15.0, 10.0));
    }

    #[test]
    fn subtract_smaller_from_larger() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(2.5, 2.5, 5.0, 5.0);
        let result = boolean_op(&a, &b, BooleanOp::Subtract, FillRule::NonZero).unwrap();
        assert!((result.area() - 75.0).abs() < 1e-6, "area {}", result.area());
        assert_eq!(result.contours.len(), 2, "outer contour plus hole");
        assert!(!result.contains(Point::new(5.0, 5.0), FillRule::NonZero));
        assert!(result.contains(Point::new(1.0, 1.0), FillRule::NonZero));
    }

    #[test]
    fn intersect_and_exclude() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(5.0, 0.0, 10.0, 10.0);
        let inter = boolean_op(&a, &b, BooleanOp::Intersect, FillRule::NonZero).unwrap();
        assert!((inter.area() - 50.0).abs() < 1e-6);
        let exclude = boolean_op(&a, &b, BooleanOp::Exclude, FillRule::NonZero).unwrap();
        assert!((exclude.area() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn booleans_with_curves_are_approximated() {
        let ellipse = ellipse_path(Size::new(100.0, 100.0));
        let square = rect(50.0, 0.0, 100.0, 100.0);
        let result = boolean_op(&ellipse, &square, BooleanOp::Intersect, FillRule::NonZero).unwrap();
        let area = result.area();
        // Half a circle of radius 50 is 3926.99; flattening tolerance 0.02 is
        // accurate to a fraction of a unit.
        assert!((area - 3926.99).abs() < 2.0, "unexpected area {area}");
        assert!(result.anchor_count() > 20, "curve should be sampled");
    }

    #[test]
    fn subtract_everything_yields_empty() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        let b = rect(-5.0, -5.0, 30.0, 30.0);
        let result = boolean_op(&a, &b, BooleanOp::Subtract, FillRule::NonZero).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn non_finite_geometry_is_rejected() {
        let mut a = rect(0.0, 0.0, 10.0, 10.0);
        a.contours[0].anchors[0].point.x = f64::NAN;
        let b = rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            boolean_op(&a, &b, BooleanOp::Union, FillRule::NonZero),
            Err(BooleanError::NonFinite)
        );
    }
}
