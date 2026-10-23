//! Viewport: the camera that maps document coordinates to screen pixels.
//!
//! A viewport stores the document-space position of the screen origin
//! (`x`, `y`), a zoom factor, the size of the visible area in CSS pixels and
//! the device pixel ratio. Screen coordinates used by pointer input are CSS
//! pixels; the raster target is `width * dpr` by `height * dpr` device pixels.

use dp_geom::{Mat2D, Point, Rect};

/// Smallest and largest supported zoom factors.
pub const MIN_ZOOM: f64 = 0.01;
/// Largest supported zoom factor.
pub const MAX_ZOOM: f64 = 64.0;

/// A camera over the document.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Viewport {
    /// Document-space x coordinate of the top-left screen corner.
    pub x: f64,
    /// Document-space y coordinate of the top-left screen corner.
    pub y: f64,
    /// Zoom factor (CSS pixels per document unit).
    pub zoom: f64,
    /// Visible width in CSS pixels.
    pub width: f64,
    /// Visible height in CSS pixels.
    pub height: f64,
    /// Device pixel ratio of the target surface.
    pub dpr: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport::new(0.0, 0.0, 1.0, 1280.0, 800.0, 1.0)
    }
}

impl Viewport {
    /// Creates a viewport, clamping zoom into the supported range.
    pub fn new(x: f64, y: f64, zoom: f64, width: f64, height: f64, dpr: f64) -> Self {
        Viewport {
            x,
            y,
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            width: width.max(1.0),
            height: height.max(1.0),
            dpr: if dpr.is_finite() && dpr > 0.0 {
                dpr
            } else {
                1.0
            },
        }
    }

    /// Clamps a zoom factor to the supported range.
    pub fn clamp_zoom(zoom: f64) -> f64 {
        if zoom.is_finite() {
            zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            1.0
        }
    }

    /// Device pixels per document unit.
    pub fn scale(&self) -> f64 {
        self.zoom * self.dpr
    }

    /// Maps document coordinates to device pixels.
    pub fn transform(&self) -> Mat2D {
        let scale = self.scale();
        Mat2D::translation(-self.x * scale, -self.y * scale).mul(Mat2D::scale(scale, scale))
    }

    /// Maps document coordinates to CSS pixels (used by the UI overlay).
    pub fn css_transform(&self) -> Mat2D {
        Mat2D::translation(-self.x * self.zoom, -self.y * self.zoom)
            .mul(Mat2D::scale(self.zoom, self.zoom))
    }

    /// Converts a document point to device pixels.
    pub fn world_to_device(&self, point: Point) -> Point {
        self.transform().apply_point(point)
    }

    /// Converts a document point to CSS pixels.
    pub fn world_to_css(&self, point: Point) -> Point {
        self.css_transform().apply_point(point)
    }

    /// Converts a CSS-pixel screen point to document space.
    pub fn css_to_world(&self, point: Point) -> Point {
        Point::new(self.x + point.x / self.zoom, self.y + point.y / self.zoom)
    }

    /// Converts a device-pixel point to document space.
    pub fn device_to_world(&self, point: Point) -> Point {
        Point::new(
            self.x + point.x / self.scale(),
            self.y + point.y / self.scale(),
        )
    }

    /// The visible region in document coordinates.
    pub fn visible_world_rect(&self) -> Rect {
        Rect::new(
            self.x,
            self.y,
            self.width / self.zoom,
            self.height / self.zoom,
        )
    }

    /// The device-pixel size of the target surface.
    pub fn device_size(&self) -> (u32, u32) {
        let w = (self.width * self.dpr).round().clamp(1.0, 16384.0) as u32;
        let h = (self.height * self.dpr).round().clamp(1.0, 16384.0) as u32;
        (w, h)
    }

    /// Pans by a CSS-pixel delta.
    pub fn pan_css(&mut self, dx: f64, dy: f64) {
        self.x -= dx / self.zoom;
        self.y -= dy / self.zoom;
    }

    /// Pans by a document-space delta.
    pub fn pan_world(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    /// Zooms around an anchor given in CSS pixels, keeping the document point
    /// under the anchor fixed on screen.
    pub fn zoom_at_css(&mut self, anchor: Point, new_zoom: f64) {
        let new_zoom = Self::clamp_zoom(new_zoom);
        let world = self.css_to_world(anchor);
        self.zoom = new_zoom;
        self.x = world.x - anchor.x / new_zoom;
        self.y = world.y - anchor.y / new_zoom;
    }

    /// Zooms around the centre of the visible area.
    pub fn zoom_to(&mut self, new_zoom: f64) {
        let centre = Point::new(self.width / 2.0, self.height / 2.0);
        self.zoom_at_css(centre, new_zoom);
    }

    /// Frames a document rectangle with the given padding in CSS pixels.
    pub fn fit_rect(&mut self, rect: Rect, padding: f64) {
        let pad = padding.max(0.0);
        let avail_w = (self.width - pad * 2.0).max(1.0);
        let avail_h = (self.height - pad * 2.0).max(1.0);
        let zoom_x = avail_w / rect.w.max(1e-6);
        let zoom_y = avail_h / rect.h.max(1e-6);
        self.zoom = Self::clamp_zoom(zoom_x.min(zoom_y));
        self.x = rect.x + rect.w / 2.0 - self.width / 2.0 / self.zoom;
        self.y = rect.y + rect.h / 2.0 - self.height / 2.0 / self.zoom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_maps_world_origin_to_screen_origin() {
        let vp = Viewport::new(10.0, 20.0, 2.0, 800.0, 600.0, 1.0);
        let p = vp.world_to_device(Point::new(10.0, 20.0));
        assert!((p.x).abs() < 1e-9 && (p.y).abs() < 1e-9, "{p:?}");
        let p = vp.world_to_device(Point::new(11.0, 21.0));
        assert!(
            (p.x - 2.0).abs() < 1e-9 && (p.y - 2.0).abs() < 1e-9,
            "{p:?}"
        );
    }

    #[test]
    fn device_and_world_roundtrip() {
        let vp = Viewport::new(-100.0, 50.0, 3.5, 800.0, 600.0, 2.0);
        let world = Point::new(12.5, -7.25);
        let back = vp.device_to_world(vp.world_to_device(world));
        assert!((back.x - world.x).abs() < 1e-9);
        assert!((back.y - world.y).abs() < 1e-9);
    }

    #[test]
    fn zoom_at_keeps_anchor_fixed() {
        let mut vp = Viewport::new(0.0, 0.0, 1.0, 800.0, 600.0, 1.0);
        let anchor = Point::new(200.0, 150.0);
        let before = vp.css_to_world(anchor);
        vp.zoom_at_css(anchor, 2.0);
        let after = vp.css_to_world(anchor);
        assert!((before.x - after.x).abs() < 1e-9);
        assert!((before.y - after.y).abs() < 1e-9);
        assert!((vp.zoom - 2.0).abs() < 1e-9);
    }

    #[test]
    fn fit_rect_centres_content() {
        let mut vp = Viewport::new(0.0, 0.0, 1.0, 800.0, 600.0, 1.0);
        vp.fit_rect(Rect::new(100.0, 100.0, 400.0, 300.0), 0.0);
        let centre = vp.world_to_device(Point::new(300.0, 250.0));
        assert!((centre.x - 400.0).abs() < 1e-6);
        assert!((centre.y - 300.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut vp = Viewport::new(0.0, 0.0, 1.0, 100.0, 100.0, 1.0);
        vp.zoom_to(1e9);
        assert_eq!(vp.zoom, MAX_ZOOM);
        vp.zoom_to(0.0);
        assert_eq!(vp.zoom, MIN_ZOOM);
    }

    #[test]
    fn visible_world_rect_matches_zoom() {
        let vp = Viewport::new(10.0, 20.0, 2.0, 800.0, 600.0, 1.0);
        let r = vp.visible_world_rect();
        assert!((r.x - 10.0).abs() < 1e-9);
        assert!((r.y - 20.0).abs() < 1e-9);
        assert!((r.w - 400.0).abs() < 1e-9);
        assert!((r.h - 300.0).abs() < 1e-9);
    }
}
