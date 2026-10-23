//! Primitive geometry types shared by the whole engine.
//!
//! All coordinates are stored as `f64` in the document's coordinate space
//! (CSS-like logical pixels). Conversions to `f32` happen only at the render
//! boundary. The types in this module are deliberately small, `Copy` where it
//! makes sense and `serde`-friendly so they can be persisted verbatim.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// Tolerance used for floating point comparisons in geometric predicates.
pub const EPSILON: f64 = 1e-9;

// ---------------------------------------------------------------------------
// Point
// ---------------------------------------------------------------------------

/// A two dimensional point (also used as a vector).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Point {
    /// The origin.
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    /// Creates a point from raw coordinates.
    #[inline]
    pub const fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    /// The origin point.
    #[inline]
    pub const fn zero() -> Point {
        Point::ZERO
    }

    /// Creates a point from polar coordinates (radians, counter-clockwise in
    /// the standard math sense).
    pub fn from_polar(radius: f64, angle: f64) -> Point {
        Point::new(radius * angle.cos(), radius * angle.sin())
    }

    /// Squared euclidean distance.
    #[inline]
    pub fn distance_sq(self, other: Point) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dx * dx + dy * dy
    }

    /// Euclidean distance.
    #[inline]
    pub fn distance(self, other: Point) -> f64 {
        self.distance_sq(other).sqrt()
    }

    /// Squared length when the point is interpreted as a vector.
    #[inline]
    pub fn length_sq(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Length when the point is interpreted as a vector.
    #[inline]
    pub fn length(self) -> f64 {
        self.length_sq().sqrt()
    }

    /// Returns a unit vector pointing in the same direction, or `None` for the
    /// zero vector.
    pub fn normalized(self) -> Option<Point> {
        let len = self.length();
        if len > EPSILON {
            Some(Point::new(self.x / len, self.y / len))
        } else {
            None
        }
    }

    /// Dot product.
    #[inline]
    pub fn dot(self, other: Point) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// 2D cross product (z component of the 3D cross product).
    #[inline]
    pub fn cross(self, other: Point) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// Component-wise minimum.
    #[inline]
    pub fn min(self, other: Point) -> Point {
        Point::new(self.x.min(other.x), self.y.min(other.y))
    }

    /// Component-wise maximum.
    #[inline]
    pub fn max(self, other: Point) -> Point {
        Point::new(self.x.max(other.x), self.y.max(other.y))
    }

    /// Component-wise absolute value.
    #[inline]
    pub fn abs(self) -> Point {
        Point::new(self.x.abs(), self.y.abs())
    }

    /// Linear interpolation at `t` (`0` yields self, `1` yields `other`).
    #[inline]
    pub fn lerp(self, other: Point, t: f64) -> Point {
        Point::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    /// Rotates the point around `center` by `radians`.
    pub fn rotate_about(self, center: Point, radians: f64) -> Point {
        let (sin, cos) = radians.sin_cos();
        let dx = self.x - center.x;
        let dy = self.y - center.y;
        Point::new(
            center.x + dx * cos - dy * sin,
            center.y + dx * sin + dy * cos,
        )
    }

    /// Whether both coordinates are finite.
    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Distance from this point to the segment `a..b`.
    pub fn distance_to_segment(self, a: Point, b: Point) -> f64 {
        let ab = b - a;
        let len_sq = ab.length_sq();
        if len_sq <= EPSILON * EPSILON {
            return self.distance(a);
        }
        let t = ((self - a).dot(ab) / len_sq).clamp(0.0, 1.0);
        let closest = a + ab * t;
        self.distance(closest)
    }

    /// Converts to `kurbo::Point`.
    #[inline]
    pub fn to_kurbo(self) -> kurbo::Point {
        kurbo::Point::new(self.x, self.y)
    }

    /// Converts from `kurbo::Point`.
    #[inline]
    pub fn from_kurbo(p: kurbo::Point) -> Point {
        Point::new(p.x, p.y)
    }
}

impl Add for Point {
    type Output = Point;
    #[inline]
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Point {
    #[inline]
    fn add_assign(&mut self, rhs: Point) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Point {
    type Output = Point;
    #[inline]
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Point {
    #[inline]
    fn sub_assign(&mut self, rhs: Point) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl Mul<f64> for Point {
    type Output = Point;
    #[inline]
    fn mul(self, rhs: f64) -> Point {
        Point::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<f64> for Point {
    type Output = Point;
    #[inline]
    fn div(self, rhs: f64) -> Point {
        Point::new(self.x / rhs, self.y / rhs)
    }
}

impl Neg for Point {
    type Output = Point;
    #[inline]
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}

impl From<(f64, f64)> for Point {
    #[inline]
    fn from((x, y): (f64, f64)) -> Point {
        Point::new(x, y)
    }
}

// ---------------------------------------------------------------------------
// Size
// ---------------------------------------------------------------------------

/// A width/height pair.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Size {
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

impl Size {
    /// Zero size.
    pub const ZERO: Size = Size { w: 0.0, h: 0.0 };

    /// Creates a size.
    #[inline]
    pub const fn new(w: f64, h: f64) -> Size {
        Size { w, h }
    }

    /// Zero size.
    #[inline]
    pub const fn zero() -> Size {
        Size::ZERO
    }

    /// Whether width or height is zero.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// Component-wise minimum.
    #[inline]
    pub fn min(self, other: Size) -> Size {
        Size::new(self.w.min(other.w), self.h.min(other.h))
    }

    /// Component-wise maximum.
    #[inline]
    pub fn max(self, other: Size) -> Size {
        Size::new(self.w.max(other.w), self.h.max(other.h))
    }

    /// The size of a rectangle anchored at the origin.
    #[inline]
    pub fn to_rect(self) -> Rect {
        Rect::new(0.0, 0.0, self.w, self.h)
    }

    /// Whether both components are finite and non-negative.
    #[inline]
    pub fn is_valid(self) -> bool {
        self.w.is_finite() && self.h.is_finite() && self.w >= 0.0 && self.h >= 0.0
    }
}

// ---------------------------------------------------------------------------
// Insets (padding, margins)
// ---------------------------------------------------------------------------

/// Per-side insets.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Insets {
    /// Top inset.
    pub top: f64,
    /// Right inset.
    pub right: f64,
    /// Bottom inset.
    pub bottom: f64,
    /// Left inset.
    pub left: f64,
}

impl Insets {
    /// Equal insets on all sides.
    #[inline]
    pub const fn uniform(value: f64) -> Insets {
        Insets {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Horizontal inset applied to both left and right.
    #[inline]
    pub const fn symmetric(horizontal: f64, vertical: f64) -> Insets {
        Insets {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Total horizontal inset.
    #[inline]
    pub fn horizontal(self) -> f64 {
        self.left + self.right
    }

    /// Total vertical inset.
    #[inline]
    pub fn vertical(self) -> f64 {
        self.top + self.bottom
    }
}

impl Add for Insets {
    type Output = Insets;
    #[inline]
    fn add(self, rhs: Insets) -> Insets {
        Insets {
            top: self.top + rhs.top,
            right: self.right + rhs.right,
            bottom: self.bottom + rhs.bottom,
            left: self.left + rhs.left,
        }
    }
}

// ---------------------------------------------------------------------------
// Rect
// ---------------------------------------------------------------------------

/// An axis aligned rectangle described by its origin and size.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    /// Left coordinate.
    pub x: f64,
    /// Top coordinate.
    pub y: f64,
    /// Width (always stored non-negative by constructors that normalize).
    pub w: f64,
    /// Height (always stored non-negative by constructors that normalize).
    pub h: f64,
}

impl Rect {
    /// Empty rectangle at the origin.
    pub const ZERO: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    };

    /// Creates a rectangle without normalizing the size.
    #[inline]
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    /// Creates a rectangle from left/top/right/bottom edges, normalizing order.
    pub fn from_ltrb(left: f64, top: f64, right: f64, bottom: f64) -> Rect {
        Rect::new(
            left.min(right),
            top.min(bottom),
            (right - left).abs(),
            (bottom - top).abs(),
        )
    }

    /// Smallest rectangle containing both points.
    pub fn from_points(a: Point, b: Point) -> Rect {
        Rect::from_ltrb(a.x, a.y, b.x, b.y)
    }

    /// Rectangle covering two rectangles.
    pub fn from_rects(a: Rect, b: Rect) -> Rect {
        Rect::from_ltrb(
            a.left().min(b.left()),
            a.top().min(b.top()),
            a.right().max(b.right()),
            a.bottom().max(b.bottom()),
        )
    }

    /// Returns a copy with non-negative width and height.
    #[inline]
    pub fn normalized(self) -> Rect {
        Rect::from_ltrb(self.x, self.y, self.right(), self.bottom())
    }

    /// Left edge.
    #[inline]
    pub fn left(self) -> f64 {
        self.x
    }

    /// Top edge.
    #[inline]
    pub fn top(self) -> f64 {
        self.y
    }

    /// Right edge.
    #[inline]
    pub fn right(self) -> f64 {
        self.x + self.w
    }

    /// Bottom edge.
    #[inline]
    pub fn bottom(self) -> f64 {
        self.y + self.h
    }

    /// Center point.
    #[inline]
    pub fn center(self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// Horizontal center.
    #[inline]
    pub fn center_x(self) -> f64 {
        self.x + self.w / 2.0
    }

    /// Vertical center.
    #[inline]
    pub fn center_y(self) -> f64 {
        self.y + self.h / 2.0
    }

    /// Origin point.
    #[inline]
    pub fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Size of the rectangle.
    #[inline]
    pub fn size(self) -> Size {
        Size::new(self.w, self.h)
    }

    /// Area (negative sizes are not normalized; call [`Rect::normalized`] first).
    #[inline]
    pub fn area(self) -> f64 {
        self.w * self.h
    }

    /// Whether the rectangle has no area.
    #[inline]
    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// Whether all coordinates are finite.
    #[inline]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.w.is_finite() && self.h.is_finite()
    }

    /// Whether the point lies inside (boundary inclusive).
    pub fn contains_point(self, p: Point) -> bool {
        p.x >= self.left() && p.x <= self.right() && p.y >= self.top() && p.y <= self.bottom()
    }

    /// Whether the inner rectangle is fully contained in this rectangle.
    pub fn contains_rect(self, other: Rect) -> bool {
        other.left() >= self.left()
            && other.right() <= self.right()
            && other.top() >= self.top()
            && other.bottom() <= self.bottom()
    }

    /// Whether two rectangles overlap (touching edges do not count).
    pub fn overlaps(self, other: Rect) -> bool {
        self.left() < other.right()
            && other.left() < self.right()
            && self.top() < other.bottom()
            && other.top() < self.bottom()
    }

    /// Intersection, or `None` when the rectangles do not overlap.
    pub fn intersect(self, other: Rect) -> Option<Rect> {
        let left = self.left().max(other.left());
        let top = self.top().max(other.top());
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right > left && bottom > top {
            Some(Rect::new(left, top, right - left, bottom - top))
        } else {
            None
        }
    }

    /// Smallest rectangle containing both rectangles.
    pub fn union(self, other: Rect) -> Rect {
        Rect::from_rects(self, other)
    }

    /// Expands the rectangle by `amount` on every side.
    pub fn expand(self, amount: f64) -> Rect {
        Rect::new(
            self.x - amount,
            self.y - amount,
            self.w + amount * 2.0,
            self.h + amount * 2.0,
        )
    }

    /// Shrinks the rectangle by the given insets.
    pub fn inset(self, insets: Insets) -> Rect {
        Rect::new(
            self.x + insets.left,
            self.y + insets.top,
            self.w - insets.horizontal(),
            self.h - insets.vertical(),
        )
    }

    /// Translates the rectangle.
    pub fn translate(self, delta: Point) -> Rect {
        Rect::new(self.x + delta.x, self.y + delta.y, self.w, self.h)
    }

    /// The four corners in clockwise order starting top-left.
    pub fn corners(self) -> [Point; 4] {
        [
            Point::new(self.left(), self.top()),
            Point::new(self.right(), self.top()),
            Point::new(self.right(), self.bottom()),
            Point::new(self.left(), self.bottom()),
        ]
    }

    /// Converts to `kurbo::Rect`.
    #[inline]
    pub fn to_kurbo(self) -> kurbo::Rect {
        kurbo::Rect::new(self.x, self.y, self.right(), self.bottom())
    }

    /// Converts from `kurbo::Rect`, normalizing.
    #[inline]
    pub fn from_kurbo(r: kurbo::Rect) -> Rect {
        Rect::from_ltrb(r.x0, r.y0, r.x1, r.y1)
    }

    /// Whether the segment `a..b` intersects the rectangle (Liang-Barsky).
    pub fn intersects_segment(self, a: Point, b: Point) -> bool {
        let mut t0 = 0.0f64;
        let mut t1 = 1.0f64;
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let checks = [
            (-dx, a.x - self.left()),
            (dx, self.right() - a.x),
            (-dy, a.y - self.top()),
            (dy, self.bottom() - a.y),
        ];
        for (p, q) in checks {
            if p.abs() <= EPSILON {
                if q < 0.0 {
                    return false;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    if r > t1 {
                        return false;
                    }
                    if r > t0 {
                        t0 = r;
                    }
                } else {
                    if r < t0 {
                        return false;
                    }
                    if r < t1 {
                        t1 = r;
                    }
                }
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Corner radii
// ---------------------------------------------------------------------------

/// Per-corner radii of a rounded rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct CornerRadii {
    /// Top-left radius.
    pub tl: f64,
    /// Top-right radius.
    pub tr: f64,
    /// Bottom-right radius.
    pub br: f64,
    /// Bottom-left radius.
    pub bl: f64,
}

impl CornerRadii {
    /// No rounding.
    pub const ZERO: CornerRadii = CornerRadii {
        tl: 0.0,
        tr: 0.0,
        br: 0.0,
        bl: 0.0,
    };

    /// Equal radius on all corners.
    #[inline]
    pub const fn uniform(radius: f64) -> CornerRadii {
        CornerRadii {
            tl: radius,
            tr: radius,
            br: radius,
            bl: radius,
        }
    }

    /// Whether every radius is (approximately) zero.
    pub fn is_zero(self) -> bool {
        self.tl.abs() <= EPSILON
            && self.tr.abs() <= EPSILON
            && self.br.abs() <= EPSILON
            && self.bl.abs() <= EPSILON
    }

    /// Clamps radii to non-negative values and proportionally reduces them when
    /// they would overlap (CSS border-radius behaviour).
    pub fn clamp_for(self, size: Size) -> CornerRadii {
        let mut r = CornerRadii {
            tl: self.tl.max(0.0),
            tr: self.tr.max(0.0),
            br: self.br.max(0.0),
            bl: self.bl.max(0.0),
        };
        if size.w <= 0.0 || size.h <= 0.0 {
            return CornerRadii::ZERO;
        }
        let mut factor = 1.0f64;
        if r.tl + r.tr > size.w {
            factor = factor.min(size.w / (r.tl + r.tr));
        }
        if r.bl + r.br > size.w {
            factor = factor.min(size.w / (r.bl + r.br));
        }
        if r.tl + r.bl > size.h {
            factor = factor.min(size.h / (r.tl + r.bl));
        }
        if r.tr + r.br > size.h {
            factor = factor.min(size.h / (r.tr + r.br));
        }
        if factor < 1.0 {
            r.tl *= factor;
            r.tr *= factor;
            r.br *= factor;
            r.bl *= factor;
        }
        r
    }
}

// ---------------------------------------------------------------------------
// Affine transform
// ---------------------------------------------------------------------------

/// A 2D affine transform stored as
/// `[[m11, m21, tx], [m12, m22, ty], [0, 0, 1]]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mat2D {
    /// X scale / rotation component.
    pub m11: f64,
    /// Y skew / rotation component.
    pub m12: f64,
    /// X skew / rotation component.
    pub m21: f64,
    /// Y scale / rotation component.
    pub m22: f64,
    /// X translation.
    pub tx: f64,
    /// Y translation.
    pub ty: f64,
}

impl Default for Mat2D {
    fn default() -> Mat2D {
        Mat2D::IDENTITY
    }
}

impl Mat2D {
    /// Identity transform.
    pub const IDENTITY: Mat2D = Mat2D {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Translation.
    #[inline]
    pub const fn translation(tx: f64, ty: f64) -> Mat2D {
        Mat2D {
            tx,
            ty,
            ..Mat2D::IDENTITY
        }
    }

    /// Non-uniform scale about the origin.
    #[inline]
    pub const fn scale(sx: f64, sy: f64) -> Mat2D {
        Mat2D {
            m11: sx,
            m22: sy,
            ..Mat2D::IDENTITY
        }
    }

    /// Rotation about the origin (radians, clockwise in screen space because
    /// the Y axis points down).
    pub fn rotation(radians: f64) -> Mat2D {
        let (sin, cos) = radians.sin_cos();
        Mat2D {
            m11: cos,
            m12: sin,
            m21: -sin,
            m22: cos,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Rotation about `center`.
    pub fn rotation_about(center: Point, radians: f64) -> Mat2D {
        Mat2D::translation(center.x, center.y)
            .mul(Mat2D::rotation(radians))
            .mul(Mat2D::translation(-center.x, -center.y))
    }

    /// Scale about `center`.
    pub fn scale_about(center: Point, sx: f64, sy: f64) -> Mat2D {
        Mat2D::translation(center.x, center.y)
            .mul(Mat2D::scale(sx, sy))
            .mul(Mat2D::translation(-center.x, -center.y))
    }

    /// Returns `self * rhs` (equivalent to applying `rhs` first, then `self`).
    pub fn mul(self, rhs: Mat2D) -> Mat2D {
        Mat2D {
            m11: self.m11 * rhs.m11 + self.m21 * rhs.m12,
            m21: self.m11 * rhs.m21 + self.m21 * rhs.m22,
            tx: self.m11 * rhs.tx + self.m21 * rhs.ty + self.tx,
            m12: self.m12 * rhs.m11 + self.m22 * rhs.m12,
            m22: self.m12 * rhs.m21 + self.m22 * rhs.m22,
            ty: self.m12 * rhs.tx + self.m22 * rhs.ty + self.ty,
        }
    }

    /// Applies `self` first and then `next`.
    pub fn then(self, next: Mat2D) -> Mat2D {
        next.mul(self)
    }

    /// Variant of [`Mat2D::then`] that consumes `self` (kept for symmetry).
    pub fn precompose(self, earlier: Mat2D) -> Mat2D {
        self.mul(earlier)
    }

    /// Transforms a point including translation.
    #[inline]
    pub fn apply_point(self, p: Point) -> Point {
        Point::new(
            self.m11 * p.x + self.m21 * p.y + self.tx,
            self.m12 * p.x + self.m22 * p.y + self.ty,
        )
    }

    /// Transforms a vector, ignoring translation.
    #[inline]
    pub fn apply_vector(self, v: Point) -> Point {
        Point::new(
            self.m11 * v.x + self.m21 * v.y,
            self.m12 * v.x + self.m22 * v.y,
        )
    }

    /// Axis aligned bounding box of the transformed rectangle.
    pub fn apply_rect(self, rect: Rect) -> Rect {
        let corners = rect.corners().map(|p| self.apply_point(p));
        let mut min = corners[0];
        let mut max = corners[0];
        for p in corners.iter().skip(1) {
            min = min.min(*p);
            max = max.max(*p);
        }
        Rect::from_ltrb(min.x, min.y, max.x, max.y)
    }

    /// Determinant of the linear part.
    #[inline]
    pub fn determinant(self) -> f64 {
        self.m11 * self.m22 - self.m12 * self.m21
    }

    /// Inverse transform, or `None` when the linear part is singular.
    pub fn inverse(self) -> Option<Mat2D> {
        let det = self.determinant();
        if det.abs() <= 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        Some(Mat2D {
            m11: self.m22 * inv,
            m12: -self.m12 * inv,
            m21: -self.m21 * inv,
            m22: self.m11 * inv,
            tx: (self.m21 * self.ty - self.m22 * self.tx) * inv,
            ty: (self.m12 * self.tx - self.m11 * self.ty) * inv,
        })
    }

    /// Returns the average absolute scale factor (useful for tolerance scaling).
    pub fn scale_factor(self) -> f64 {
        let sx = (self.m11 * self.m11 + self.m12 * self.m12).sqrt();
        let sy = (self.m21 * self.m21 + self.m22 * self.m22).sqrt();
        ((sx + sy) / 2.0).max(EPSILON)
    }

    /// Whether the transform is the identity within `EPSILON`.
    pub fn is_identity(self) -> bool {
        (self.m11 - 1.0).abs() < EPSILON
            && self.m12.abs() < EPSILON
            && self.m21.abs() < EPSILON
            && (self.m22 - 1.0).abs() < EPSILON
            && self.tx.abs() < EPSILON
            && self.ty.abs() < EPSILON
    }
}

impl Mul for Mat2D {
    type Output = Mat2D;
    #[inline]
    fn mul(self, rhs: Mat2D) -> Mat2D {
        Mat2D::mul(self, rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    fn approx_point(a: Point, b: Point) {
        approx(a.x, b.x);
        approx(a.y, b.y);
    }

    #[test]
    fn point_math() {
        let p = Point::new(3.0, 4.0);
        approx(p.length(), 5.0);
        approx_point(p.normalized().unwrap(), Point::new(0.6, 0.8));
        assert_eq!(Point::ZERO.normalized(), None);
        approx(Point::new(1.0, 0.0).dot(Point::new(0.0, 1.0)), 0.0);
        approx(Point::new(1.0, 0.0).cross(Point::new(0.0, 1.0)), 1.0);
        approx_point(
            Point::new(0.0, 0.0).lerp(Point::new(10.0, 20.0), 0.5),
            Point::new(5.0, 10.0),
        );
        approx_point(
            Point::new(1.0, 0.0).rotate_about(Point::ZERO, std::f64::consts::FRAC_PI_2),
            Point::new(0.0, 1.0),
        );
        approx(
            Point::new(5.0, 5.0).distance_to_segment(Point::new(0.0, 0.0), Point::new(10.0, 0.0)),
            5.0,
        );
    }

    #[test]
    fn rect_math() {
        let r = Rect::new(10.0, 20.0, 30.0, 40.0);
        assert_eq!(r.right(), 40.0);
        assert_eq!(r.bottom(), 60.0);
        assert_eq!(r.center(), Point::new(25.0, 40.0));
        assert!(r.contains_point(Point::new(25.0, 40.0)));
        assert!(!r.contains_point(Point::new(41.0, 40.0)));
        let other = Rect::new(35.0, 50.0, 20.0, 20.0);
        let inter = r.intersect(other).unwrap();
        assert_eq!(inter, Rect::new(35.0, 50.0, 5.0, 10.0));
        assert_eq!(r.union(other), Rect::from_ltrb(10.0, 20.0, 55.0, 70.0));
        assert!(r.overlaps(other));
        assert!(!r.overlaps(Rect::new(100.0, 100.0, 1.0, 1.0)));
        assert_eq!(
            Rect::from_points(Point::new(5.0, 5.0), Point::new(1.0, 2.0)).size(),
            Size::new(4.0, 3.0)
        );
        assert!(r.intersects_segment(Point::new(0.0, 0.0), Point::new(100.0, 100.0)));
        assert!(!r.intersects_segment(Point::new(0.0, 0.0), Point::new(0.0, 10.0)));
    }

    #[test]
    fn corner_radii_clamp() {
        let radio = CornerRadii::uniform(30.0);
        let clamped = radio.clamp_for(Size::new(40.0, 40.0));
        approx(clamped.tl, 20.0);
    }

    #[test]
    fn mat2d_composition_and_inverse() {
        let m = Mat2D::translation(10.0, 20.0)
            .then(Mat2D::scale(2.0, 3.0))
            .then(Mat2D::rotation(std::f64::consts::FRAC_PI_2));
        let p = Point::new(1.0, 1.0);
        let got = m.apply_point(p);
        // Apply translation, then scale, then rotation.
        let expected = Mat2D::rotation(std::f64::consts::FRAC_PI_2).apply_point(
            Mat2D::scale(2.0, 3.0).apply_point(Mat2D::translation(10.0, 20.0).apply_point(p)),
        );
        approx_point(got, expected);
        let inv = m.inverse().unwrap();
        approx_point(inv.apply_point(got), p);
        approx(m.determinant(), 6.0);
        assert!(Mat2D::scale(0.0, 1.0).inverse().is_none());
        assert!(Mat2D::IDENTITY.is_identity());
    }

    #[test]
    fn mat2d_rect_transform() {
        let m = Mat2D::rotation_about(Point::new(5.0, 5.0), std::f64::consts::FRAC_PI_4);
        let r = m.apply_rect(Rect::new(0.0, 0.0, 10.0, 10.0));
        // A 10x10 square rotated by 45 degrees has a bounding box of 10*sqrt(2).
        approx(r.w, 10.0 * std::f64::consts::SQRT_2);
        approx(r.h, 10.0 * std::f64::consts::SQRT_2);
    }
}
