//! Serializable text layout results.
//!
//! The renderer and the layout engine both consume these structures. Glyph
//! positions are in document units, relative to the text box origin (top-left),
//! with `y` measured downwards. `baseline` is the absolute baseline of the line
//! measured from the box top, and each glyph's `y` is already that baseline plus
//! its in-line offset, so a renderer can place a rasterized glyph at
//! `(glyph.x + bitmap.left, glyph.y - bitmap.top)`.
//!
//! Text layouts are intentionally *not* serializable: they reference
//! `fontdb::ID` values that are only meaningful for the `FontSystem` that
//! produced them. The engine keeps the layout in-process and exposes glyphs for
//! rasterization through [`crate::TextEngine::rasterize_glyph`].

use cosmic_text::{fontdb, CacheKeyFlags};

/// One shaped glyph inside a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// Glyph index within its font.
    pub glyph_id: u16,
    /// Font the glyph came from. Only valid for the engine that shaped it.
    pub font_id: fontdb::ID,
    /// Font size used for shaping.
    pub font_size: f32,
    /// Font weight (`100..=900`).
    pub font_weight: u16,
    /// Horizontal pen position in document units from the box left.
    pub x: f64,
    /// Baseline position plus in-line offset, in document units from the box top.
    pub y: f64,
    /// Advance width of the glyph in document units.
    pub advance: f64,
    /// First byte of the source cluster (UTF-8, original string).
    pub cluster_start: usize,
    /// One past the last byte of the source cluster (UTF-8, original string).
    pub cluster_end: usize,
    /// Shaping flags needed to reproduce the rasterized glyph (fake italic etc.).
    pub flags: CacheKeyFlags,
}

impl Glyph {
    /// Whether the cluster is a line break or other zero-width control.
    pub fn is_zero_width(&self) -> bool {
        self.advance.abs() < f64::EPSILON
    }
}

/// One laid out line of text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    /// Index of the paragraph this line belongs to (soft-wrapped lines share it).
    pub paragraph_index: usize,
    /// The text of the line (without the trailing newline).
    pub text: String,
    /// Visible line width in document units.
    pub width: f64,
    /// Top of the line box measured from the text box top.
    pub top: f64,
    /// Line height (top to next top).
    pub height: f64,
    /// Baseline measured from the text box top.
    pub baseline: f64,
    /// Shaped glyphs in visual order.
    pub glyphs: Vec<Glyph>,
}

impl TextLine {
    /// Bottom of the line box.
    pub fn bottom(&self) -> f64 {
        self.top + self.height
    }

    /// Number of glyphs on this line.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }
}

/// Result of shaping and laying out a text object.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    /// Width of the widest line in document units.
    pub width: f64,
    /// Height of the laid-out text in document units.
    pub height: f64,
    /// Lines, top to bottom.
    pub lines: Vec<TextLine>,
    /// Whether the content was clipped (fixed box too small, or `max_lines`
    /// truncation was applied).
    pub overflow: bool,
    /// Whether the requested family was missing and a fallback is in use.
    pub fallback_used: bool,
    /// The family that was requested by the text style.
    pub requested_family: String,
    /// The installed family that was actually used, when it resolved.
    pub used_family: Option<String>,
    /// Font size used for metrics.
    pub font_size: f32,
    /// Line height used for metrics.
    pub line_height: f32,
}

impl TextLayout {
    /// An empty layout for an engine with no fonts.
    pub fn empty() -> TextLayout {
        TextLayout {
            width: 0.0,
            height: 0.0,
            lines: Vec::new(),
            overflow: false,
            fallback_used: false,
            requested_family: String::new(),
            used_family: None,
            font_size: 0.0,
            line_height: 0.0,
        }
    }

    /// Whether the layout contains no glyphs.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() || self.lines.iter().all(|line| line.glyphs.is_empty())
    }

    /// Number of laid out lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Number of glyphs across all lines.
    pub fn glyph_count(&self) -> usize {
        self.lines.iter().map(TextLine::glyph_count).sum()
    }

    /// The measured size.
    pub fn size(&self) -> dp_geom::Size {
        dp_geom::Size::new(self.width, self.height)
    }

    /// The glyph nearest to a hit position, as `(line_index, glyph_index)`.
    ///
    /// Used by future hit testing and caret placement; returns `None` for an
    /// empty layout. The nearest glyph is chosen by horizontal distance, which
    /// is sufficient for the editor's caret snapping.
    pub fn nearest_glyph(&self, x: f64, y: f64) -> Option<(usize, usize)> {
        let line_index = self
            .lines
            .iter()
            .position(|line| y < line.bottom())
            .unwrap_or(self.lines.len().saturating_sub(1));
        let line = self.lines.get(line_index)?;
        let mut best: Option<(usize, f64)> = None;
        for (index, glyph) in line.glyphs.iter().enumerate() {
            let center = glyph.x + glyph.advance * 0.5;
            let distance = (center - x).abs();
            if best.map_or(true, |(_, best_distance)| distance < best_distance) {
                best = Some((index, distance));
            }
        }
        best.map(|(index, _)| (line_index, index))
    }
}

/// A rasterized glyph as straight RGBA8 pixels.
///
/// `left`/`top` are the swash placement relative to the glyph pen position:
/// the bitmap's top-left corner sits at `(pen_x + left, pen_y - top)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlyphBitmap {
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// Horizontal bearing (pixels from the pen position).
    pub left: i32,
    /// Vertical bearing (pixels above the baseline).
    pub top: i32,
    /// Straight-alpha RGBA8 pixels (`width * height * 4` bytes).
    pub data: Vec<u8>,
}

impl GlyphBitmap {
    /// Number of pixels.
    pub fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}
