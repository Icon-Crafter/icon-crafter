//! Vector PDF export: generate SVG from the display list, then convert it with
//! `svg2pdf`. Text becomes selectable PDF text where fonts resolve.

use dp_model::{Document, PageId};

use crate::{ExportError, ExportOptions};

/// Exports a page as a vector PDF.
pub fn export_pdf(
    document: &Document,
    page: PageId,
    options: &ExportOptions,
) -> Result<Vec<u8>, ExportError> {
    let svg = crate::svg::export_svg(document, page, options)?;
    let mut usvg_options = svg2pdf::usvg::Options::default();
    usvg_options.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(&svg, &usvg_options)
        .map_err(|error| ExportError::Conversion(error.to_string()))?;
    svg2pdf::to_pdf(
        &tree,
        svg2pdf::ConversionOptions::default(),
        svg2pdf::PageOptions::default(),
    )
    .map_err(|error| ExportError::Conversion(error.to_string()))
}
