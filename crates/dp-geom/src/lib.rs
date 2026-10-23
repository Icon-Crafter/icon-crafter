//! Geometry kernel for Icon Crafter.
//!
//! This crate is intentionally dependency-light and free of document, UI or
//! rendering concerns. Everything here operates on plain values:
//!
//! - [`types`]: points, rectangles, affine transforms, corner radii;
//! - [`path`]: cubic bezier contours, path editing, flattening, hit tests;
//! - [`shapes`]: path constructors for the primitive shape nodes;
//! - [`boolean`]: boolean operations on filled paths (via `i_overlay`);
//! - [`spatial`]: a uniform grid for cheap area queries;
//! - [`snap`]: snapping and alignment-guide computation.
//!
//! All public geometry uses `f64` coordinates; the renderer converts to `f32`
//! at its own boundary.

pub mod boolean;
pub mod path;
pub mod shapes;
pub mod snap;
pub mod spatial;
pub mod types;

pub use boolean::{boolean_op, boolean_op_with_tolerance, BooleanError, BooleanOp};
pub use path::{
    polygon_crossings, polygon_winding, signed_polygon_area, simplify_collinear, Anchor, Contour,
    FillRule, PathSegment, VectorPath,
};
pub use shapes::{ellipse_path, line_path, polygon_path, rectangle_path, star_path};
pub use snap::{
    snap_point, snap_rect, Axis, Guide, GridSpec, SnapKind, SnapLine, SnapOptions, SnapResult,
};
pub use spatial::SpatialGrid;
pub use types::{
    CornerRadii, Insets, Mat2D, Point, Rect, Size, EPSILON,
};
