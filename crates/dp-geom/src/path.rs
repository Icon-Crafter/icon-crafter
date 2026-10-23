//! Vector path model: anchors with cubic handles, contours and the
//! path-editing operations the editor builds on.
//!
//! A path is a list of contours; each contour is a list of anchors. Every pair
//! of consecutive anchors is connected by a cubic bezier segment whose control
//! points are stored on the anchors themselves (`out_handle` of the first
//! anchor and `in_handle` of the second one). Straight segments are represented
//! by handles that coincide with their anchor, which keeps the model uniform
//! and makes handle dragging work for every segment.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::types::{EPSILON, Mat2D, Point, Rect};

/// The fill rule used to decide whether a point is inside a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillRule {
    /// Non-zero winding rule (default for design tools).
    #[default]
    NonZero,
    /// Even-odd rule.
    EvenOdd,
}

// ---------------------------------------------------------------------------
// Anchor
// ---------------------------------------------------------------------------

/// A single anchor point of a contour.
///
/// Deserialization accepts anchors that only carry `p`; the handles then
/// default to the anchor position (a hard corner).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Anchor {
    /// Anchor position in the path's local coordinate space.
    #[serde(rename = "p")]
    pub point: Point,
    /// Incoming control point (cubic handle of the segment arriving here).
    #[serde(rename = "in")]
    pub in_handle: Point,
    /// Outgoing control point (cubic handle of the segment leaving here).
    #[serde(rename = "out")]
    pub out_handle: Point,
    /// Whether the two handles are mirrored through the anchor.
    #[serde(default, skip_serializing_if = "is_false")]
    pub smooth: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Default for Anchor {
    fn default() -> Anchor {
        Anchor {
            point: Point::ZERO,
            in_handle: Point::ZERO,
            out_handle: Point::ZERO,
            smooth: false,
        }
    }
}

impl Anchor {
    /// A corner anchor (both handles coincide with the point).
    pub fn corner(point: Point) -> Anchor {
        Anchor {
            point,
            in_handle: point,
            out_handle: point,
            smooth: false,
        }
    }

    /// An anchor with explicit handles.
    pub fn with_handles(point: Point, in_handle: Point, out_handle: Point) -> Anchor {
        Anchor {
            point,
            in_handle,
            out_handle,
            smooth: true,
        }
    }

    /// Whether this anchor has no bezier handles (a hard corner).
    pub fn is_sharp(&self) -> bool {
        self.in_handle == self.point && self.out_handle == self.point
    }

    /// Moves the anchor and both handles by `delta`.
    pub fn translate(&mut self, delta: Point) {
        self.point += delta;
        self.in_handle += delta;
        self.out_handle += delta;
    }

    /// Applies an affine transform to the anchor and its handles.
    pub fn transform(&mut self, matrix: &Mat2D) {
        self.point = matrix.apply_point(self.point);
        self.in_handle = matrix.apply_point(self.in_handle);
        self.out_handle = matrix.apply_point(self.out_handle);
    }

    /// Mirrors the outgoing handle through the anchor based on the incoming one.
    pub fn mirror_out_from_in(&mut self) {
        self.out_handle = self.point * 2.0 - self.in_handle;
    }

    /// Mirrors the incoming handle through the anchor based on the outgoing one.
    pub fn mirror_in_from_out(&mut self) {
        self.in_handle = self.point * 2.0 - self.out_handle;
    }

    /// Makes the handles collinear while preserving their lengths (smooth node).
    pub fn make_smooth(&mut self) {
        let in_len = self.point.distance(self.in_handle);
        let out_len = self.point.distance(self.out_handle);
        if in_len <= EPSILON && out_len <= EPSILON {
            self.smooth = true;
            return;
        }
        let dir = if out_len > EPSILON {
            (self.out_handle - self.point) / out_len
        } else {
            -(self.in_handle - self.point) / in_len
        };
        self.in_handle = self.point - dir * in_len;
        self.out_handle = self.point + dir * out_len;
        self.smooth = true;
    }
}

impl<'de> Deserialize<'de> for Anchor {
    fn deserialize<D>(deserializer: D) -> Result<Anchor, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(rename = "p")]
            point: Point,
            #[serde(rename = "in", default)]
            in_handle: Option<Point>,
            #[serde(rename = "out", default)]
            out_handle: Option<Point>,
            #[serde(default)]
            smooth: bool,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Anchor {
            point: raw.point,
            in_handle: raw.in_handle.unwrap_or(raw.point),
            out_handle: raw.out_handle.unwrap_or(raw.point),
            smooth: raw.smooth,
        })
    }
}

// ---------------------------------------------------------------------------
// Contour
// ---------------------------------------------------------------------------

/// A single contour (sub-path) made of anchors.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Contour {
    /// Whether the contour is closed (last anchor connects back to the first).
    #[serde(default)]
    pub closed: bool,
    /// Anchors in drawing order.
    #[serde(default)]
    pub anchors: Vec<Anchor>,
}

impl Contour {
    /// An open contour.
    pub fn open(anchors: Vec<Anchor>) -> Contour {
        Contour {
            closed: false,
            anchors,
        }
    }

    /// A closed contour.
    pub fn closed(anchors: Vec<Anchor>) -> Contour {
        Contour {
            closed: true,
            anchors,
        }
    }

    /// A closed polygon from plain points (no handles).
    pub fn polygon(points: impl IntoIterator<Item = Point>) -> Contour {
        Contour::closed(points.into_iter().map(Anchor::corner).collect())
    }

    /// Number of anchors.
    pub fn len(&self) -> usize {
        self.anchors.len()
    }

    /// Whether the contour has no anchors.
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    /// Number of segments (edges) in this contour.
    pub fn segment_count(&self) -> usize {
        match self.anchors.len() {
            0 | 1 => 0,
            n if self.closed => n,
            n => n - 1,
        }
    }

    /// Creates the segment leaving anchor `i` (wrapping for closed contours).
    pub fn segment(&self, index: usize) -> Option<PathSegment> {
        let count = self.segment_count();
        if index >= count {
            return None;
        }
        let n = self.anchors.len();
        let i = index;
        let j = (index + 1) % n;
        let a = self.anchors[i];
        let b = self.anchors[j];
        let is_line = a.out_handle == a.point && b.in_handle == b.point;
        Some(if is_line {
            PathSegment::line(a.point, b.point)
        } else {
            PathSegment::cubic(a.point, a.out_handle, b.in_handle, b.point)
        })
    }

    /// All segments in drawing order.
    pub fn segments(&self) -> Vec<PathSegment> {
        (0..self.segment_count())
            .filter_map(|i| self.segment(i))
            .collect()
    }

    /// Applies an affine transform.
    pub fn transform(&mut self, matrix: &Mat2D) {
        for anchor in &mut self.anchors {
            anchor.transform(matrix);
        }
    }

    /// Translates the contour.
    pub fn translate(&mut self, delta: Point) {
        for anchor in &mut self.anchors {
            anchor.translate(delta);
        }
    }

    /// Approximate bounds of the contour (exact for cubics; control points are
    /// included via the derivative roots).
    pub fn bounds(&self) -> Option<Rect> {
        if self.anchors.is_empty() {
            return None;
        }
        let mut bounds: Option<Rect> = None;
        for anchor in &self.anchors {
            let r = Rect::new(anchor.point.x, anchor.point.y, 0.0, 0.0);
            bounds = Some(match bounds {
                Some(b) => b.union(r),
                None => r,
            });
        }
        for segment in self.segments() {
            let sb = segment.bounds();
            bounds = Some(match bounds {
                Some(b) => b.union(sb),
                None => sb,
            });
        }
        bounds
    }

    /// Flattens the contour into a polyline. Closed contours do not repeat the
    /// first point at the end.
    pub fn flatten(&self, tolerance: f64) -> Vec<Point> {
        let tol = tolerance.max(1e-4);
        let mut out: Vec<Point> = Vec::new();
        for segment in self.segments() {
            let pts = segment.flatten(tol);
            if out.is_empty() {
                if let Some(first) = pts.first() {
                    out.push(*first);
                }
            }
            for p in pts.into_iter().skip(1) {
                if out
                    .last()
                    .map(|last| last.distance_sq(p) <= EPSILON * EPSILON)
                    != Some(true)
                {
                    out.push(p);
                }
            }
        }
        if self.closed && out.len() >= 2 {
            let first = out[0];
            let last = out[out.len() - 1];
            if first.distance_sq(last) <= EPSILON * EPSILON {
                out.pop();
            }
        }
        out
    }

    /// Signed area (shoelace) of the flattened contour. Positive means the
    /// contour winds clockwise in screen space (Y down).
    pub fn signed_area(&self) -> f64 {
        let pts = self.flatten(0.1);
        signed_polygon_area(&pts)
    }

    /// Reverses the drawing direction (and swaps handle roles).
    pub fn reverse(&mut self) {
        self.anchors.reverse();
        for anchor in &mut self.anchors {
            std::mem::swap(&mut anchor.in_handle, &mut anchor.out_handle);
        }
    }
}

// ---------------------------------------------------------------------------
// Path segment
// ---------------------------------------------------------------------------

/// A single cubic bezier segment (a line is encoded as a degenerate cubic).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSegment {
    /// Start point.
    pub from: Point,
    /// First control point.
    pub c1: Point,
    /// Second control point.
    pub c2: Point,
    /// End point.
    pub to: Point,
}

impl PathSegment {
    /// Straight segment.
    pub fn line(from: Point, to: Point) -> PathSegment {
        PathSegment {
            from,
            c1: from,
            c2: to,
            to,
        }
    }

    /// Cubic segment.
    pub fn cubic(from: Point, c1: Point, c2: Point, to: Point) -> PathSegment {
        PathSegment { from, c1, c2, to }
    }

    /// Whether the segment is a straight line.
    pub fn is_line(&self) -> bool {
        self.c1 == self.from && self.c2 == self.to
    }

    /// Evaluates the segment at `t` in `[0, 1]`.
    pub fn point_at(&self, t: f64) -> Point {
        let t = t.clamp(0.0, 1.0);
        let mt = 1.0 - t;
        let a = self.from * (mt * mt * mt);
        let b = self.c1 * (3.0 * mt * mt * t);
        let c = self.c2 * (3.0 * mt * t * t);
        let d = self.to * (t * t * t);
        a + b + c + d
    }

    /// First derivative at `t`.
    pub fn derivative_at(&self, t: f64) -> Point {
        let t = t.clamp(0.0, 1.0);
        let mt = 1.0 - t;
        let a = (self.c1 - self.from) * (3.0 * mt * mt);
        let b = (self.c2 - self.c1) * (6.0 * mt * t);
        let c = (self.to - self.c2) * (3.0 * t * t);
        a + b + c
    }

    /// Tangent angle at `t` in radians.
    pub fn angle_at(&self, t: f64) -> f64 {
        let d = self.derivative_at(t);
        d.y.atan2(d.x)
    }

    /// Splits the segment at `t` into two cubic segments (de Casteljau).
    pub fn split(&self, t: f64) -> (PathSegment, PathSegment) {
        let t = t.clamp(0.0, 1.0);
        let ab = self.from.lerp(self.c1, t);
        let bc = self.c1.lerp(self.c2, t);
        let cd = self.c2.lerp(self.to, t);
        let abc = ab.lerp(bc, t);
        let bcd = bc.lerp(cd, t);
        let mid = abc.lerp(bcd, t);
        (
            PathSegment::cubic(self.from, ab, abc, mid),
            PathSegment::cubic(mid, bcd, cd, self.to),
        )
    }

    /// Exact axis aligned bounds of the segment.
    pub fn bounds(&self) -> Rect {
        if self.is_line() {
            return Rect::from_points(self.from, self.to);
        }
        let (mut min_x, mut max_x) = (self.from.x.min(self.to.x), self.from.x.max(self.to.x));
        let (mut min_y, mut max_y) = (self.from.y.min(self.to.y), self.from.y.max(self.to.y));
        for t in cubic_extrema(self.from.x, self.c1.x, self.c2.x, self.to.x) {
            let x = self.point_at(t).x;
            min_x = min_x.min(x);
            max_x = max_x.max(x);
        }
        for t in cubic_extrema(self.from.y, self.c1.y, self.c2.y, self.to.y) {
            let y = self.point_at(t).y;
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
    }

    /// Adaptively flattens the segment. Always includes both end points.
    pub fn flatten(&self, tolerance: f64) -> Vec<Point> {
        if self.is_line() {
            return vec![self.from, self.to];
        }
        let tol = tolerance.max(1e-4);
        let mut out = vec![self.from];
        flatten_cubic_recursive(self.from, self.c1, self.c2, self.to, tol, 0, &mut out);
        out
    }

    /// Approximate arc length via adaptive flattening.
    pub fn length(&self) -> f64 {
        let pts = self.flatten(0.01);
        pts.windows(2).map(|w| w[0].distance(w[1])).sum()
    }
}

/// Returns the `t` values in `(0, 1)` where the cubic derivative is zero on one
/// axis.
///
/// The derivative of a cubic bezier divided by three is
/// `a t^2 + b t + c` with
/// `c = p1 - p0`, `b = 2 (p0 - 2 p1 + p2)` and `a = 3 p1 - 3 p2 + p3 - p0`;
/// scaling the polynomial does not change its roots.
fn cubic_extrema(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    let a = 3.0 * p1 - 3.0 * p2 + p3 - p0;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let mut roots = Vec::new();
    if a.abs() < EPSILON {
        if b.abs() > EPSILON {
            let t = -c / b;
            if t > 0.0 && t < 1.0 {
                roots.push(t);
            }
        }
        return roots;
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return roots;
    }
    let sqrt_disc = disc.sqrt();
    for t in [(-b + sqrt_disc) / (2.0 * a), (-b - sqrt_disc) / (2.0 * a)] {
        if t > 0.0 && t < 1.0 {
            roots.push(t);
        }
    }
    roots
}

fn flatten_cubic_recursive(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tolerance: f64,
    depth: u32,
    out: &mut Vec<Point>,
) {
    if depth >= 18 {
        out.push(p3);
        return;
    }
    let flatness = point_line_distance(p1, p0, p3).max(point_line_distance(p2, p0, p3));
    if flatness <= tolerance {
        out.push(p3);
        return;
    }
    let ab = p0.lerp(p1, 0.5);
    let bc = p1.lerp(p2, 0.5);
    let cd = p2.lerp(p3, 0.5);
    let abc = ab.lerp(bc, 0.5);
    let bcd = bc.lerp(cd, 0.5);
    let mid = abc.lerp(bcd, 0.5);
    flatten_cubic_recursive(p0, ab, abc, mid, tolerance, depth + 1, out);
    flatten_cubic_recursive(mid, bcd, cd, p3, tolerance, depth + 1, out);
}

/// Perpendicular distance from `p` to the infinite line `a..b`.
fn point_line_distance(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len = ab.length();
    if len <= EPSILON {
        return p.distance(a);
    }
    let cross = (p - a).cross(ab);
    (cross / len).abs()
}

// ---------------------------------------------------------------------------
// VectorPath
// ---------------------------------------------------------------------------

/// A complete vector path made of one or more contours.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VectorPath {
    /// Contours in drawing order.
    #[serde(default)]
    pub contours: Vec<Contour>,
}

impl VectorPath {
    /// Empty path.
    pub fn new() -> VectorPath {
        VectorPath::default()
    }

    /// Path from a single contour.
    pub fn from_contour(contour: Contour) -> VectorPath {
        VectorPath {
            contours: vec![contour],
        }
    }

    /// Path from plain polygon points (one closed contour).
    pub fn polygon(points: impl IntoIterator<Item = Point>) -> VectorPath {
        VectorPath::from_contour(Contour::polygon(points))
    }

    /// Whether the path has no contours.
    pub fn is_empty(&self) -> bool {
        self.contours.iter().all(|c| c.anchors.is_empty())
    }

    /// Total number of anchors.
    pub fn anchor_count(&self) -> usize {
        self.contours.iter().map(|c| c.anchors.len()).sum()
    }

    /// Number of contours.
    pub fn contour_count(&self) -> usize {
        self.contours.len()
    }

    /// Bounding box, or `None` for an empty path.
    pub fn bounds(&self) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        for contour in &self.contours {
            if let Some(b) = contour.bounds() {
                bounds = Some(match bounds {
                    Some(existing) => existing.union(b),
                    None => b,
                });
            }
        }
        bounds
    }

    /// Applies an affine transform to every anchor.
    pub fn transform(&mut self, matrix: &Mat2D) {
        for contour in &mut self.contours {
            contour.transform(matrix);
        }
    }

    /// Translates every anchor.
    pub fn translate(&mut self, delta: Point) {
        for contour in &mut self.contours {
            contour.translate(delta);
        }
    }

    /// Flattens every contour. Closed contours omit the duplicate closing point.
    pub fn flatten(&self, tolerance: f64) -> Vec<Vec<Point>> {
        let tol = tolerance.max(1e-4);
        self.contours
            .iter()
            .map(|c| c.flatten(tol))
            .filter(|points| points.len() >= 2)
            .collect()
    }

    /// Signed area of the flattened path (shoelace sum over contours).
    pub fn signed_area(&self) -> f64 {
        self.contours.iter().map(|c| c.signed_area()).sum()
    }

    /// Absolute filled area. Holes created by opposite winding cancel out.
    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    /// Point containment test using the requested fill rule. Points exactly on
    /// the outline are considered inside.
    pub fn contains(&self, point: Point, fill_rule: FillRule) -> bool {
        let tolerance = 0.25;
        let polygons = self.flatten(tolerance);
        for polygon in &polygons {
            if polygon
                .windows(2)
                .any(|w| point.distance_to_segment(w[0], w[1]) <= 1e-7)
                || (polygon.len() > 2
                    && point.distance_to_segment(polygon[polygon.len() - 1], polygon[0]) <= 1e-7)
            {
                return true;
            }
        }
        match fill_rule {
            FillRule::EvenOdd => {
                let mut crossings = 0usize;
                for polygon in &polygons {
                    crossings += polygon_crossings(point, polygon);
                }
                crossings % 2 == 1
            }
            FillRule::NonZero => {
                let mut winding = 0i32;
                for polygon in &polygons {
                    winding += polygon_winding(point, polygon);
                }
                winding != 0
            }
        }
    }

    /// Shortest distance from `point` to the path outline (open contours are not
    /// implicitly closed here). Returns `f64::INFINITY` for an empty path.
    pub fn distance_to_outline(&self, point: Point) -> f64 {
        let mut best = f64::INFINITY;
        for contour in &self.contours {
            let points = contour.flatten(0.1);
            for window in points.windows(2) {
                best = best.min(point.distance_to_segment(window[0], window[1]));
            }
            if contour.closed && points.len() >= 2 {
                best = best.min(point.distance_to_segment(points[points.len() - 1], points[0]));
            }
        }
        best
    }

    /// Splits the segment starting at anchor `segment_index` of contour
    /// `contour_index` at parameter `t`, inserting a new anchor. Returns the
    /// index of the inserted anchor.
    pub fn split_segment(
        &mut self,
        contour_index: usize,
        segment_index: usize,
        t: f64,
    ) -> Option<usize> {
        let contour = self.contours.get_mut(contour_index)?;
        let segment = contour.segment(segment_index)?;
        let (left, right) = segment.split(t);
        let n = contour.anchors.len();
        let start = segment_index;
        let end = (segment_index + 1) % n;
        contour.anchors[start].out_handle = left.c1;
        contour.anchors[end].in_handle = right.c2;
        let inserted = Anchor {
            point: left.to,
            in_handle: right.c1,
            out_handle: left.c2,
            smooth: true,
        };
        let insert_at = (start + 1).min(n);
        contour.anchors.insert(insert_at, inserted);
        Some(insert_at)
    }

    /// Removes an anchor. Contours with fewer than three anchors are removed
    /// entirely, mirroring what design tools do for degenerate shapes.
    pub fn remove_anchor(&mut self, contour_index: usize, anchor_index: usize) -> bool {
        let Some(contour) = self.contours.get(contour_index) else {
            return false;
        };
        if anchor_index >= contour.anchors.len() {
            return false;
        }
        if contour.anchors.len() <= 2 {
            self.contours.remove(contour_index);
            return true;
        }
        self.contours[contour_index].anchors.remove(anchor_index);
        true
    }

    /// Reverses the winding of one contour.
    pub fn reverse_contour(&mut self, contour_index: usize) -> bool {
        match self.contours.get_mut(contour_index) {
            Some(contour) => {
                contour.reverse();
                true
            }
            None => false,
        }
    }

    /// Samples the first contour at `distance` along its outline.
    /// Returns the point and the tangent angle in radians.
    pub fn sample_along(&self, distance: f64) -> Option<(Point, f64)> {
        let contour = self.contours.first()?;
        let segments = contour.segments();
        if segments.is_empty() {
            return None;
        }
        let total: f64 = segments.iter().map(|s| s.length()).sum();
        if total <= EPSILON {
            return None;
        }
        let mut remaining = distance.rem_euclid(total);
        for segment in &segments {
            let length = segment.length().max(EPSILON);
            if remaining <= length {
                let t = (remaining / length).clamp(0.0, 1.0);
                return Some((segment.point_at(t), segment.angle_at(t)));
            }
            remaining -= length;
        }
        segments.last().map(|s| (s.to, s.angle_at(1.0)))
    }

    /// Converts the path to a `kurbo::BezPath`.
    pub fn to_kurbo(&self) -> kurbo::BezPath {
        use kurbo::{BezPath, PathEl};
        let mut out = BezPath::new();
        for contour in &self.contours {
            if contour.anchors.is_empty() {
                continue;
            }
            out.push(PathEl::MoveTo(contour.anchors[0].point.to_kurbo()));
            for segment in contour.segments() {
                if segment.is_line() {
                    out.push(PathEl::LineTo(segment.to.to_kurbo()));
                } else {
                    out.push(PathEl::CurveTo(
                        segment.c1.to_kurbo(),
                        segment.c2.to_kurbo(),
                        segment.to.to_kurbo(),
                    ));
                }
            }
            if contour.closed {
                out.push(PathEl::ClosePath);
            }
        }
        out
    }

    /// Builds a path from a `kurbo::BezPath` (quadratics are promoted to cubics).
    pub fn from_kurbo(path: &kurbo::BezPath) -> VectorPath {
        use kurbo::PathEl;
        let mut contours: Vec<Contour> = Vec::new();
        let mut current: Vec<Anchor> = Vec::new();
        for element in path.elements() {
            match *element {
                PathEl::MoveTo(p) => {
                    if current.len() > 1 {
                        contours.push(Contour::open(std::mem::take(&mut current)));
                    } else {
                        current.clear();
                    }
                    current.push(Anchor::corner(Point::from_kurbo(p)));
                }
                PathEl::LineTo(p) => {
                    if let Some(last) = current.last_mut() {
                        last.out_handle = last.point;
                    }
                    current.push(Anchor::corner(Point::from_kurbo(p)));
                }
                PathEl::QuadTo(q, p) => {
                    if let Some(last) = current.last() {
                        let p0 = last.point;
                        let q = Point::from_kurbo(q);
                        let p1 = Point::from_kurbo(p);
                        let c1 = p0.lerp(q, 2.0 / 3.0);
                        let c2 = p1.lerp(q, 2.0 / 3.0);
                        if let Some(last) = current.last_mut() {
                            last.out_handle = c1;
                        }
                        let mut anchor = Anchor::corner(p1);
                        anchor.in_handle = c2;
                        anchor.smooth = true;
                        current.push(anchor);
                    }
                }
                PathEl::CurveTo(c1, c2, p) => {
                    if let Some(last) = current.last_mut() {
                        last.out_handle = Point::from_kurbo(c1);
                    }
                    let mut anchor = Anchor::corner(Point::from_kurbo(p));
                    anchor.in_handle = Point::from_kurbo(c2);
                    anchor.smooth = true;
                    current.push(anchor);
                }
                PathEl::ClosePath => {
                    if current.len() >= 2 {
                        let first = current[0];
                        let last = current[current.len() - 1];
                        if last.point.distance_sq(first.point) <= EPSILON * EPSILON {
                            // The closing segment is the last one; move its
                            // incoming handle onto the first anchor.
                            current[0].in_handle = last.in_handle;
                            current.pop();
                        }
                        contours.push(Contour::closed(current));
                    } else {
                        contours.push(Contour::closed(current));
                    }
                    current = Vec::new();
                }
            }
        }
        if current.len() >= 2 {
            contours.push(Contour::open(current));
        }
        VectorPath { contours }
    }
}

// ---------------------------------------------------------------------------
// Polygon helpers
// ---------------------------------------------------------------------------

/// Signed shoelace area of a polygon (implicitly closed).
pub fn signed_polygon_area(points: &[Point]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum / 2.0
}

/// Even-odd ray casting crossings for a polygon.
pub fn polygon_crossings(point: Point, polygon: &[Point]) -> usize {
    let n = polygon.len();
    if n < 3 {
        return 0;
    }
    let mut crossings = 0;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        if (a.y > point.y) != (b.y > point.y) {
            let t = (point.y - a.y) / (b.y - a.y);
            if point.x < a.x + t * (b.x - a.x) {
                crossings += 1;
            }
        }
    }
    crossings
}

/// Non-zero winding number for a polygon.
pub fn polygon_winding(point: Point, polygon: &[Point]) -> i32 {
    let n = polygon.len();
    if n < 3 {
        return 0;
    }
    let mut winding = 0i32;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        if a.y <= point.y {
            if b.y > point.y && (b - a).cross(point - a) > 0.0 {
                winding += 1;
            }
        } else if b.y <= point.y && (b - a).cross(point - a) < 0.0 {
            winding -= 1;
        }
    }
    winding
}

/// Removes points that lie on the straight line between their neighbours.
pub fn simplify_collinear(points: Vec<Point>, epsilon: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points;
    }
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for point in points {
        while out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let area = (b - a).cross(point - a).abs();
            let scale = (b - a).length().max(1.0);
            if area / scale <= epsilon {
                out.pop();
            } else {
                break;
            }
        }
        out.push(point);
    }
    // Check the wrap-around corner as well, but never below three points.
    while out.len() > 3 {
        let n = out.len();
        let a = out[n - 2];
        let b = out[n - 1];
        let c = out[0];
        if (b - a).cross(c - a).abs() <= epsilon {
            out.pop();
        } else {
            break;
        }
    }
    out
}

/// Approximate length of a flattened polyline.
pub fn polyline_length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Total number of anchor points in a closed polygon representation.
pub fn polygon_count(points: &[Point]) -> usize {
    points.len()
}

/// Smallest angle step between regular polygon vertices.
pub const POLYGON_START_ANGLE: f64 = -TAU / 4.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> VectorPath {
        VectorPath::polygon([
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ])
    }

    #[test]
    fn polygon_bounds_and_area() {
        let path = square();
        let bounds = path.bounds().unwrap();
        assert_eq!(bounds, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!((path.area() - 100.0).abs() < 1e-6);
        assert_eq!(path.anchor_count(), 4);
        assert_eq!(path.contours[0].segment_count(), 4);
    }

    #[test]
    fn contains_respects_fill_rule() {
        let path = square();
        assert!(path.contains(Point::new(5.0, 5.0), FillRule::NonZero));
        assert!(!path.contains(Point::new(15.0, 5.0), FillRule::NonZero));
        // A ring: outer clockwise, inner counter-clockwise -> hole with non-zero.
        let outer = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let inner = [
            Point::new(3.0, 3.0),
            Point::new(3.0, 7.0),
            Point::new(7.0, 7.0),
            Point::new(7.0, 3.0),
        ];
        let ring = VectorPath {
            contours: vec![Contour::polygon(outer), Contour::polygon(inner)],
        };
        // Both rules agree that the hole is outside.
        assert!(!ring.contains(Point::new(5.0, 5.0), FillRule::NonZero));
        assert!(!ring.contains(Point::new(5.0, 5.0), FillRule::EvenOdd));
        assert!(ring.contains(Point::new(1.0, 1.0), FillRule::NonZero));
        // Two overlapping contours with the same winding differ between rules.
        let overlap = VectorPath {
            contours: vec![
                Contour::polygon([
                    Point::new(0.0, 0.0),
                    Point::new(10.0, 0.0),
                    Point::new(10.0, 10.0),
                    Point::new(0.0, 10.0),
                ]),
                Contour::polygon([
                    Point::new(5.0, 5.0),
                    Point::new(15.0, 5.0),
                    Point::new(15.0, 15.0),
                    Point::new(5.0, 15.0),
                ]),
            ],
        };
        assert!(overlap.contains(Point::new(7.0, 7.0), FillRule::NonZero));
        assert!(!overlap.contains(Point::new(7.0, 7.0), FillRule::EvenOdd));
    }

    #[test]
    fn flatten_closed_contour() {
        let path = square();
        let polys = path.flatten(0.1);
        assert_eq!(polys.len(), 1);
        assert_eq!(polys[0].len(), 4);
    }

    #[test]
    fn split_segment_inserts_anchor() {
        let mut path = square();
        let index = path.split_segment(0, 0, 0.5).unwrap();
        assert_eq!(index, 1);
        assert_eq!(path.anchor_count(), 5);
        let anchor = path.contours[0].anchors[1];
        assert_eq!(anchor.point, Point::new(5.0, 0.0));
    }

    #[test]
    fn split_bezier_preserves_shape() {
        let curve = PathSegment::cubic(
            Point::new(0.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(10.0, 10.0),
            Point::new(10.0, 0.0),
        );
        let (left, right) = curve.split(0.5);
        for t in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let expected = curve.point_at(t);
            let got = if t <= 0.5 {
                left.point_at(t * 2.0)
            } else {
                right.point_at((t - 0.5) * 2.0)
            };
            assert!(got.distance(expected) < 1e-9);
        }
    }

    #[test]
    fn remove_anchor() {
        let mut path = square();
        assert!(path.remove_anchor(0, 0));
        assert_eq!(path.anchor_count(), 3);
        assert!(path.remove_anchor(0, 0));
        assert_eq!(path.anchor_count(), 2);
        assert!(path.remove_anchor(0, 0));
        assert!(path.is_empty());
    }

    #[test]
    fn cubic_bounds_are_tight() {
        let segment = PathSegment::cubic(
            Point::new(0.0, 0.0),
            Point::new(0.0, 100.0),
            Point::new(100.0, 100.0),
            Point::new(100.0, 0.0),
        );
        let bounds = segment.bounds();
        assert!((bounds.w - 100.0).abs() < 1e-6);
        assert!((bounds.h - 75.0).abs() < 1e-6);
    }

    #[test]
    fn kurbo_roundtrip() {
        let path = square();
        let kurbo_path = path.to_kurbo();
        let back = VectorPath::from_kurbo(&kurbo_path);
        assert_eq!(back.contours.len(), 1);
        assert!(back.contours[0].closed);
        assert_eq!(back.contours[0].anchors.len(), 4);
        assert!((back.area() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn anchor_deserialize_defaults_handles() {
        let anchor: Anchor = serde_json::from_str(r#"{"p":{"x":1.0,"y":2.0}}"#).unwrap();
        assert_eq!(anchor.point, Point::new(1.0, 2.0));
        assert_eq!(anchor.in_handle, Point::new(1.0, 2.0));
        assert_eq!(anchor.out_handle, Point::new(1.0, 2.0));
        assert!(!anchor.smooth);
    }

    #[test]
    fn anchor_serde_roundtrip() {
        let anchor = Anchor::with_handles(
            Point::new(5.0, 5.0),
            Point::new(4.0, 5.0),
            Point::new(6.0, 5.0),
        );
        let json = serde_json::to_string(&anchor).unwrap();
        let back: Anchor = serde_json::from_str(&json).unwrap();
        assert_eq!(anchor, back);
    }

    #[test]
    fn smooth_anchor_mirrors_handles() {
        let mut anchor = Anchor::corner(Point::new(0.0, 0.0));
        anchor.out_handle = Point::new(10.0, 0.0);
        anchor.in_handle = Point::new(-2.0, 0.0);
        anchor.make_smooth();
        let out_dir = (anchor.out_handle - anchor.point).normalized().unwrap();
        let in_dir = (anchor.in_handle - anchor.point).normalized().unwrap();
        assert!((out_dir.dot(in_dir) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn reverse_contour_swaps_handles() {
        let mut contour = Contour::closed(vec![
            Anchor::corner(Point::new(0.0, 0.0)),
            Anchor::corner(Point::new(10.0, 0.0)),
            Anchor::corner(Point::new(10.0, 10.0)),
        ]);
        let before = contour.signed_area();
        contour.reverse();
        let after = contour.signed_area();
        assert!(before * after < 0.0);
    }

    #[test]
    fn simplify_collinear_removes_midpoints() {
        let points = vec![
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ];
        let simplified = simplify_collinear(points, 1e-6);
        assert_eq!(simplified.len(), 3);
    }
}
