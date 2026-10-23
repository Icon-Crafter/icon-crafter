//! Import/export for Icon Crafter.
//!
//! * [`svg`] exports the display list as editable SVG.
//! * [`import`] imports SVG (vector paths; raster images and text are counted
//!   and skipped with warnings instead of silently losing content).
//! * [`raster`] exports PNG/JPEG through the deterministic CPU renderer.
//! * [`pdf`] exports vector PDF by converting the generated SVG.
//!
//! The native document format itself lives in `dp-persist`.

pub mod import;
pub mod pdf;
pub mod raster;
pub mod svg;
pub mod website;

use dp_geom::Rect;
use dp_model::{Color, Document, PageId};

pub use import::{ImportResult, import_svg};
pub use svg::export_svg;

/// Output format for page export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// PNG raster.
    Png,
    /// JPEG raster (alpha is composited over the background).
    Jpeg,
    /// Editable SVG.
    Svg,
    /// Vector PDF.
    Pdf,
}

impl ExportFormat {
    /// File extension without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Svg => "svg",
            ExportFormat::Pdf => "pdf",
        }
    }

    /// MIME type of the produced bytes.
    pub fn mime(self) -> &'static str {
        match self {
            ExportFormat::Png => "image/png",
            ExportFormat::Jpeg => "image/jpeg",
            ExportFormat::Svg => "image/svg+xml",
            ExportFormat::Pdf => "application/pdf",
        }
    }
}

/// Export settings.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Raster scale factor (1.0 = one target pixel per document unit).
    pub scale: f64,
    /// Background colour; `None` keeps transparency for PNG/SVG and uses white
    /// for JPEG/PDF.
    pub background: Option<Color>,
    /// Explicit export bounds; defaults to the content bounds of the page.
    pub bounds: Option<Rect>,
    /// Optional clip rectangle (used for "export selection").
    pub clip: Option<Rect>,
    /// Decimal precision for SVG coordinates.
    pub precision: u8,
    /// JPEG quality in `1..=100`.
    pub jpeg_quality: u8,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            scale: 1.0,
            background: None,
            bounds: None,
            clip: None,
            precision: 2,
            jpeg_quality: 90,
        }
    }
}

/// Exported bytes plus metadata.
#[derive(Debug, Clone)]
pub struct ExportedFile {
    /// Encoded file bytes.
    pub bytes: Vec<u8>,
    /// MIME type of `bytes`.
    pub mime: &'static str,
    /// Suggested extension without the dot.
    pub extension: &'static str,
}

/// Errors raised by import/export.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The page does not exist in the document.
    #[error("page {0:?} does not exist")]
    MissingPage(PageId),
    /// The page has no content to export.
    #[error("the page has no visible content to export")]
    EmptyDocument,
    /// Scene construction failed.
    #[error("scene construction failed: {0}")]
    Scene(String),
    /// Rasterization failed.
    #[error("rasterization failed: {0}")]
    Render(String),
    /// SVG parsing or PDF conversion failed.
    #[error("svg/pdf conversion failed: {0}")]
    Conversion(String),
    /// Encoding failed.
    #[error("encoding failed: {0}")]
    Encoding(String),
    /// Writing to disk failed.
    #[error("io failed: {0}")]
    Io(String),
}

impl From<dp_render::BuildError> for ExportError {
    fn from(error: dp_render::BuildError) -> Self {
        ExportError::Scene(error.to_string())
    }
}

impl From<dp_render::RenderError> for ExportError {
    fn from(error: dp_render::RenderError) -> Self {
        ExportError::Render(error.to_string())
    }
}

/// Exports a page in the requested format.
pub fn export_page(
    document: &Document,
    page: PageId,
    format: ExportFormat,
    options: &ExportOptions,
) -> Result<ExportedFile, ExportError> {
    let bytes = match format {
        ExportFormat::Png | ExportFormat::Jpeg => {
            raster::export_raster(document, page, format, options)?
        }
        ExportFormat::Svg => {
            let svg = svg::export_svg(document, page, options)?;
            svg.into_bytes()
        }
        ExportFormat::Pdf => pdf::export_pdf(document, page, options)?,
    };
    Ok(ExportedFile {
        bytes,
        mime: format.mime(),
        extension: format.extension(),
    })
}

/// Exports a page and writes it to `path`.
pub fn export_page_to_file(
    document: &Document,
    page: PageId,
    format: ExportFormat,
    options: &ExportOptions,
    path: impl AsRef<std::path::Path>,
) -> Result<(), ExportError> {
    let file = export_page(document, page, format, options)?;
    std::fs::write(path, file.bytes).map_err(|error| ExportError::Io(error.to_string()))
}

/// Version of the import/export API surface.
pub const IO_VERSION: &str = "0.1.0";
