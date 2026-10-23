//! Text engine for Icon Crafter.
//!
//! Responsibilities:
//!
//! * discover installed fonts and resolve requested families (with missing-font
//!   detection, because `cosmic-text` falls back silently),
//! * shape and lay out text into a deterministic, document-space layout that the
//!   layout engine, renderer and dev-mode code generation all consume,
//! * rasterize individual glyphs to RGBA masks for the CPU reference rasterizer
//!   and for texture-atlas uploads on the GPU path,
//! * cache layout results so dragging a 400-glyph label does not reshape text on
//!   every frame.
//!
//! The engine owns exactly one `FontSystem` for the whole application (font
//! databases are expensive to build), and it is deliberately *not* shared across
//! threads: shaping mutates internal caches. Callers hold it behind `&mut` from
//! the editor engine, which is single-threaded per document.

mod fonts;
mod layout;

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::{
    Align, Attrs, Buffer, CacheKey, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, Metrics,
    Shaping, Style, SwashCache, SwashContent, Weight, Wrap,
};
use dp_model::geometry::{AutoSize, TextAlign, TextContent, TextParagraph, TextStyle};

pub use cosmic_text;
pub use fonts::{FontCatalog, FontMatch};
pub use layout::{Glyph, GlyphBitmap, TextLayout, TextLine};

/// Errors the text engine can report.
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    /// The operating system exposed no usable fonts.
    #[error("no fonts were discovered on this system; text cannot be rendered")]
    NoFonts,
}

/// Default upper bound of cached layouts.
pub const DEFAULT_LAYOUT_CACHE_LIMIT: usize = 512;

/// Shape, layout, measure and rasterize text.
pub struct TextEngine {
    font_system: FontSystem,
    catalog: FontCatalog,
    swash: SwashCache,
    cache: HashMap<String, Arc<TextLayout>>,
    cache_limit: usize,
}

impl std::fmt::Debug for TextEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEngine")
            .field("families", &self.catalog.len())
            .field("cached_layouts", &self.cache.len())
            .field("cache_limit", &self.cache_limit)
            .finish()
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    /// Creates an engine backed by the system font database.
    ///
    /// This is expensive (font discovery can take a second in release builds and
    /// several seconds in debug builds); create it once and keep it.
    pub fn new() -> Self {
        Self::with_cache_limit(DEFAULT_LAYOUT_CACHE_LIMIT)
    }

    /// Creates an engine with a specific layout cache size (`0` disables caching).
    pub fn with_cache_limit(cache_limit: usize) -> Self {
        let font_system = FontSystem::new();
        let catalog = FontCatalog::from_db(font_system.db());
        if catalog.is_empty() {
            tracing::error!("text engine started without any system fonts");
        } else {
            tracing::debug!("text engine loaded {} font families", catalog.len());
        }
        Self {
            font_system,
            catalog,
            swash: SwashCache::new(),
            cache: HashMap::new(),
            cache_limit,
        }
    }

    /// The installed font catalog.
    pub fn catalog(&self) -> &FontCatalog {
        &self.catalog
    }

    /// Resolves a family name, reporting missing-font fallback.
    pub fn resolve_family(&self, family: &str) -> FontMatch {
        self.catalog.resolve(family)
    }

    /// Replaces the font system (useful for tests and headless servers).
    pub fn set_font_system(&mut self, font_system: FontSystem) {
        self.catalog = FontCatalog::from_db(font_system.db());
        self.font_system = font_system;
        self.cache.clear();
    }

    /// Drops all cached layouts (call after fonts change or a font finishes loading).
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Number of cached layouts.
    pub fn cached_layout_count(&self) -> usize {
        self.cache.len()
    }

    /// Lays out a text object's content.
    ///
    /// `max_width`/`max_height` are the available space in document units; pass
    /// `None` for an unconstrained axis.
    pub fn layout(
        &mut self,
        content: &TextContent,
        max_width: Option<f64>,
        max_height: Option<f64>,
    ) -> TextLayout {
        self.layout_styled(
            &content.characters,
            &content.style,
            &content.paragraph,
            content.auto_size,
            max_width,
            max_height,
        )
    }

    /// Lays out a text content and returns a shared handle to the cached result.
    pub fn layout_shared(
        &mut self,
        content: &TextContent,
        max_width: Option<f64>,
        max_height: Option<f64>,
    ) -> Arc<TextLayout> {
        self.layout_styled_shared(
            &content.characters,
            &content.style,
            &content.paragraph,
            content.auto_size,
            max_width,
            max_height,
        )
    }

    /// Lays out raw characters with explicit styles.
    pub fn layout_styled(
        &mut self,
        characters: &str,
        style: &TextStyle,
        paragraph: &TextParagraph,
        auto_size: AutoSize,
        max_width: Option<f64>,
        max_height: Option<f64>,
    ) -> TextLayout {
        (*self.layout_styled_shared(
            characters, style, paragraph, auto_size, max_width, max_height,
        ))
        .clone()
    }

    /// Lays out raw characters and returns the shared cached layout.
    #[allow(clippy::too_many_arguments)]
    pub fn layout_styled_shared(
        &mut self,
        characters: &str,
        style: &TextStyle,
        paragraph: &TextParagraph,
        auto_size: AutoSize,
        max_width: Option<f64>,
        max_height: Option<f64>,
    ) -> Arc<TextLayout> {
        let key = cache_key(
            characters, style, paragraph, auto_size, max_width, max_height,
        );
        if let Some(layout) = self.cache.get(&key) {
            return Arc::clone(layout);
        }
        let layout = Arc::new(self.shape(
            characters, style, paragraph, auto_size, max_width, max_height,
        ));
        if self.cache_limit > 0 {
            if self.cache.len() >= self.cache_limit {
                // Deterministic and cheap: drop the whole cache rather than
                // paying for an LRU on every insertion. Layout caches are warm
                // within a few frames anyway.
                self.cache.clear();
            }
            self.cache.insert(key, Arc::clone(&layout));
        }
        layout
    }

    /// Measures text without keeping a strong reference to the layout.
    pub fn measure(
        &mut self,
        content: &TextContent,
        max_width: Option<f64>,
        max_height: Option<f64>,
    ) -> dp_geom::Size {
        self.layout_shared(content, max_width, max_height).size()
    }

    fn shape(
        &mut self,
        characters: &str,
        style: &TextStyle,
        paragraph: &TextParagraph,
        auto_size: AutoSize,
        max_width: Option<f64>,
        max_height: Option<f64>,
    ) -> TextLayout {
        let resolution = self.catalog.resolve(&style.font_family);
        let font_size = style.font_size.clamp(1.0, 4096.0) as f32;
        let line_height = style
            .line_height
            .resolve(style.font_size)
            .map(|value| value.clamp(1.0, 16384.0))
            .unwrap_or(style.font_size * 1.2) as f32;

        let metrics = Metrics::new(font_size, line_height);

        // Auto width means "hug content": only fixed-width modes constrain the
        // shaping width. Fixed size also constrains the height.
        let (width_opt, height_opt) = match auto_size {
            AutoSize::WidthAndHeight => (None, None),
            AutoSize::Height => (max_width.map(|v| v.max(0.0) as f32), None),
            AutoSize::None => (
                max_width.map(|v| v.max(0.0) as f32),
                max_height.map(|v| v.max(0.0) as f32),
            ),
        };

        if characters.is_empty() {
            return TextLayout {
                width: 0.0,
                height: 0.0,
                lines: Vec::new(),
                overflow: false,
                fallback_used: resolution.fallback,
                requested_family: resolution.requested,
                used_family: resolution.matched,
                font_size,
                line_height,
                font_weight: style.font_weight,
                italic: style.italic,
                letter_spacing: style.letter_spacing,
            };
        }

        let align = match paragraph.align {
            TextAlign::Left => None,
            TextAlign::Center => Some(Align::Center),
            TextAlign::Right => Some(Align::Right),
            TextAlign::Justified => Some(Align::Justified),
        };
        let letter_spacing_em = if style.font_size > 0.0 {
            (style.letter_spacing / style.font_size) as f32
        } else {
            0.0
        };
        let attrs = Attrs::new()
            .family(Family::Name(&style.font_family))
            .weight(Weight(style.font_weight))
            .style(if style.italic {
                Style::Italic
            } else {
                Style::Normal
            })
            .letter_spacing(letter_spacing_em);

        // `cosmic-text` truncates to `max_lines` with an ellipsis but does not
        // report whether the text was longer. Shape once without the limit when
        // a line limit is set; the probe tells us whether the box overflowed.
        let max_lines = paragraph.max_lines.unwrap_or(0) as usize;
        let mut probed_lines = None;
        if max_lines > 0 {
            let mut probe = Buffer::new(&mut self.font_system, metrics);
            {
                let mut probe = probe.borrow_with(&mut self.font_system);
                probe.set_size(width_opt, None);
                probe.set_wrap(if width_opt.is_some() {
                    Wrap::WordOrGlyph
                } else {
                    Wrap::None
                });
                probe.set_text(characters, &attrs, Shaping::Advanced, align);
                probed_lines = Some(probe.layout_runs().count());
            }
        }

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        let mut lines: Vec<TextLine> = Vec::new();
        let mut width = 0.0_f64;

        {
            let mut buffer = buffer.borrow_with(&mut self.font_system);
            buffer.set_size(width_opt, height_opt);
            buffer.set_wrap(if width_opt.is_some() {
                Wrap::WordOrGlyph
            } else {
                Wrap::None
            });
            if max_lines > 0 {
                buffer.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(max_lines)));
            }
            buffer.set_text(characters, &attrs, Shaping::Advanced, align);

            for run in buffer.layout_runs() {
                let mut glyphs = Vec::with_capacity(run.glyphs.len());
                for glyph in run.glyphs {
                    let x = glyph.x + glyph.font_size * glyph.x_offset;
                    let y = run.line_y + glyph.y - glyph.font_size * glyph.y_offset;
                    glyphs.push(Glyph {
                        glyph_id: glyph.glyph_id,
                        font_id: glyph.font_id,
                        font_size: glyph.font_size,
                        font_weight: glyph.font_weight.0,
                        x: x as f64,
                        y: y as f64,
                        advance: glyph.w as f64,
                        cluster_start: glyph.start,
                        cluster_end: glyph.end,
                        flags: glyph.cache_key_flags,
                    });
                }
                width = width.max(run.line_w as f64);
                lines.push(TextLine {
                    paragraph_index: run.line_i,
                    text: run.text.to_string(),
                    width: run.line_w as f64,
                    top: run.line_top as f64,
                    height: run.line_height as f64,
                    baseline: run.line_y as f64,
                    glyphs,
                });
            }
        }

        let mut height = lines
            .last()
            .map(|line| line.top + line.height)
            .unwrap_or(0.0);
        let mut overflow = false;
        if let Some(limit) = height_opt {
            if height > limit as f64 + 0.5 {
                overflow = true;
                height = limit as f64;
            }
        }
        if max_lines > 0 {
            if probed_lines.is_some_and(|count| count > max_lines) {
                overflow = true;
            }
            if lines.len() > max_lines {
                overflow = true;
                lines.truncate(max_lines);
            }
        }

        TextLayout {
            width,
            height,
            lines,
            overflow,
            fallback_used: resolution.fallback,
            requested_family: resolution.requested,
            used_family: resolution.matched,
            font_size,
            line_height,
            font_weight: style.font_weight,
            italic: style.italic,
            letter_spacing: style.letter_spacing,
        }
    }

    /// Rasterizes one glyph to straight-alpha RGBA8 pixels.
    ///
    /// Glyph masks are cached inside the `SwashCache`, so repeated calls for the
    /// same glyph and subpixel position are cheap.
    pub fn rasterize_glyph(&mut self, glyph: &Glyph) -> Option<GlyphBitmap> {
        let position = (glyph.x as f32, glyph.y as f32);
        let (key, _, _) = CacheKey::new(
            glyph.font_id,
            glyph.glyph_id,
            glyph.font_size,
            position,
            Weight(glyph.font_weight),
            glyph.flags,
        );
        let TextEngine {
            font_system, swash, ..
        } = self;
        let image = swash.get_image(font_system, key).as_ref()?;
        let width = image.placement.width;
        let height = image.placement.height;
        if width == 0 || height == 0 {
            return None;
        }
        let pixels = width as usize * height as usize;
        let mut data = Vec::with_capacity(pixels * 4);
        match image.content {
            SwashContent::Mask => {
                if image.data.len() < pixels {
                    return None;
                }
                for &alpha in &image.data[..pixels] {
                    data.extend_from_slice(&[255, 255, 255, alpha]);
                }
            }
            SwashContent::Color => {
                if image.data.len() < pixels * 4 {
                    return None;
                }
                data.extend_from_slice(&image.data[..pixels * 4]);
            }
            _ => return None,
        }
        Some(GlyphBitmap {
            width,
            height,
            left: image.placement.left,
            top: image.placement.top,
            data,
        })
    }

    /// The rasterized size and placement of one glyph, without copying pixels.
    pub fn glyph_bounds(&mut self, glyph: &Glyph) -> Option<(u32, u32, i32, i32)> {
        let position = (glyph.x as f32, glyph.y as f32);
        let (key, _, _) = CacheKey::new(
            glyph.font_id,
            glyph.glyph_id,
            glyph.font_size,
            position,
            Weight(glyph.font_weight),
            glyph.flags,
        );
        let TextEngine {
            font_system, swash, ..
        } = self;
        let image = swash.get_image(font_system, key).as_ref()?;
        Some((
            image.placement.width,
            image.placement.height,
            image.placement.left,
            image.placement.top,
        ))
    }
}

fn cache_key(
    characters: &str,
    style: &TextStyle,
    paragraph: &TextParagraph,
    auto_size: AutoSize,
    max_width: Option<f64>,
    max_height: Option<f64>,
) -> String {
    format!(
        "{characters}\u{1f}{family}\u{1f}{weight}/{italic}/{size}/{line_unit}/{line_value}/{letter}/{case:?}/{align:?}/{max_lines:?}/{auto:?}/{max_h}/{max_w}",
        family = style.font_family,
        weight = style.font_weight,
        italic = style.italic,
        size = style.font_size.to_bits(),
        line_unit = style.line_height.unit as u8,
        line_value = style.line_height.value.to_bits(),
        letter = style.letter_spacing.to_bits(),
        case = style.text_case,
        align = paragraph.align,
        max_lines = paragraph.max_lines,
        auto = auto_size,
        max_h = max_height.map(f64::to_bits).unwrap_or(u64::MAX),
        max_w = max_width.map(f64::to_bits).unwrap_or(u64::MAX),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn engine() -> MutexGuard<'static, TextEngine> {
        static ENGINE: OnceLock<Mutex<TextEngine>> = OnceLock::new();
        ENGINE
            .get_or_init(|| Mutex::new(TextEngine::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn plain(text: &str) -> TextContent {
        TextContent::plain(text)
    }

    #[test]
    fn engine_reports_installed_fonts() {
        let engine = engine();
        assert!(!engine.catalog().is_empty());
        assert!(engine.catalog().default_family().is_some());
    }

    #[test]
    fn lays_out_single_line() {
        let mut engine = engine();
        let layout = engine.layout(&plain("Hello, design tools"), None, None);
        assert_eq!(layout.line_count(), 1);
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
        assert!(layout.glyph_count() > 0);
        assert!(!layout.overflow);
    }

    #[test]
    fn wraps_when_width_is_constrained() {
        let mut engine = engine();
        // Auto-height text has a fixed width: it wraps at the constraint.
        let mut content = plain("The quick brown fox jumps over the lazy dog again and again");
        content.auto_size = AutoSize::Height;
        let layout = engine.layout(&content, Some(120.0), None);
        assert!(layout.line_count() > 1, "expected wrapping, got one line");
        assert!(layout.width <= 120.5);
        assert!(layout.height > layout.line_height as f64);
    }

    #[test]
    fn empty_text_has_zero_size() {
        let mut engine = engine();
        let layout = engine.layout(&plain(""), None, None);
        assert!(layout.is_empty());
        assert_eq!(layout.height, 0.0);
        assert_eq!(layout.width, 0.0);
    }

    #[test]
    fn missing_family_reports_fallback() {
        let mut engine = engine();
        let mut content = plain("Fallback text");
        content.style.font_family = "Definitely Not A Font 9000".to_string();
        let layout = engine.layout(&content, None, None);
        assert!(layout.fallback_used);
        assert_eq!(layout.used_family, None);
        assert!(layout.width > 0.0, "fallback must still produce glyphs");
    }

    #[test]
    fn unknown_family_falls_back_but_known_resolves() {
        let engine = engine();
        let default = engine.catalog().default_family().unwrap().to_string();
        let resolved = engine.resolve_family(&default);
        assert!(!resolved.fallback);
        assert!(resolved.matched.is_some());
        let missing = engine.resolve_family("Nope Sans 123");
        assert!(missing.fallback);
    }

    #[test]
    fn max_lines_truncates_and_marks_overflow() {
        let mut engine = engine();
        let mut content = plain("one two three four five six seven eight nine ten");
        content.auto_size = AutoSize::Height;
        content.paragraph.max_lines = Some(2);
        let layout = engine.layout(&content, Some(80.0), None);
        assert!(layout.line_count() <= 2);
        assert!(layout.overflow);
    }

    #[test]
    fn glyphs_have_positions_and_clusters() {
        let mut engine = engine();
        let layout = engine.layout(&plain("AV"), None, None);
        let line = &layout.lines[0];
        assert!(!line.glyphs.is_empty());
        let first = &line.glyphs[0];
        assert_eq!(first.cluster_start, 0);
        assert!(first.advance > 0.0);
        assert!(first.font_id != cosmic_text::fontdb::ID::dummy());
        if line.glyphs.len() > 1 {
            assert!(line.glyphs[1].x > first.x);
        }
    }

    #[test]
    fn rasterizes_glyph_pixels() {
        let mut engine = engine();
        let layout = engine.layout(&plain("A"), None, None);
        let glyph = layout.lines[0].glyphs[0].clone();
        let bitmap = engine.rasterize_glyph(&glyph).expect("glyph bitmap");
        assert!(bitmap.width > 0);
        assert!(bitmap.height > 0);
        assert_eq!(bitmap.data.len(), bitmap.pixel_count() * 4);
        assert!(bitmap.data.iter().any(|byte| *byte > 0));
    }

    #[test]
    fn layout_cache_reuses_results() {
        let mut engine = engine();
        engine.clear_cache();
        let content = plain("cache me");
        let first = engine.layout_shared(&content, None, None);
        let second = engine.layout_shared(&content, None, None);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(engine.cached_layout_count(), 1);
    }

    #[test]
    fn unicode_text_shapes() {
        let mut engine = engine();
        let layout = engine.layout(&plain("héllo 🦀 café"), None, None);
        assert!(!layout.is_empty());
        assert!(layout.width > 0.0);
    }
}
