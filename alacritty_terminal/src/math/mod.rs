//! Math formula rendering using Typst.
//!
//! This module provides LaTeX-to-PNG conversion for inline formula rendering.
//! Data flow: LaTeX → MiTeX → Typst → PNG → GraphicData

mod convert;
mod layout;
mod world;

pub use convert::latex_to_typst;
pub use layout::{extract_layout, LayoutContent, LayoutItem, TypstMathLayout};
pub use world::{compile_math, compile_math_document};

use crate::graphics::{ColorType, GraphicData, GraphicId};

/// Rendered formula image data.
pub struct FormulaImage {
    /// RGBA pixel data.
    pub pixels: Vec<u8>,
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
}

/// Render LaTeX to PNG image data.
///
/// Returns RGBA pixel data suitable for creating a GraphicData.
pub fn render_latex_to_png(latex: &str, ppi: f32) -> Result<FormulaImage, String> {
    // Convert LaTeX to Typst.
    let typst_math = latex_to_typst(latex)?;

    // Compile to document.
    let document = compile_math_document(&typst_math).map_err(|e| e.join(", "))?;

    // Get the first page.
    let page = document
        .pages
        .first()
        .ok_or_else(|| "No pages in document".to_string())?;

    // Render to pixmap.
    let pixmap = typst_render::render(page, ppi / 72.0);

    Ok(FormulaImage {
        width: pixmap.width() as usize,
        height: pixmap.height() as usize,
        pixels: pixmap.take(),
    })
}

/// Create a GraphicData from a rendered formula.
pub fn formula_to_graphic(image: FormulaImage, id: GraphicId) -> GraphicData {
    GraphicData {
        id,
        width: image.width,
        height: image.height,
        color_type: ColorType::Rgba,
        pixels: image.pixels,
        is_opaque: false,
    }
}

/// Render LaTeX to Typst math layout.
pub fn render_latex(latex: &str) -> Result<TypstMathLayout, String> {
    // Convert LaTeX to Typst.
    let typst_math = latex_to_typst(latex)?;

    // Compile to Frame.
    let frame = compile_math(&typst_math).map_err(|e| e.join(", "))?;

    // Extract layout items.
    let items = extract_layout(&frame);

    Ok(TypstMathLayout {
        items,
        width_pt: frame.width().to_pt(),
        height_pt: frame.height().to_pt(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_latex_frac() {
        let result = render_latex(r"\frac{a}{b}");
        assert!(result.is_ok(), "render_latex failed: {:?}", result.err());

        let layout = result.unwrap();
        assert!(layout.items.len() >= 2, "Expected at least 2 items (a, b), got {}", layout.items.len());
        assert!(layout.width_pt > 0.0, "Width should be positive");
        assert!(layout.height_pt > 0.0, "Height should be positive");
    }

    #[test]
    fn test_render_latex_sum() {
        let result = render_latex(r"\sum_{i=1}^{n}");
        assert!(result.is_ok(), "render_latex failed: {:?}", result.err());

        let layout = result.unwrap();
        assert!(!layout.items.is_empty(), "Expected at least 1 item");
    }

    #[test]
    fn test_render_latex_sqrt() {
        let result = render_latex(r"\sqrt{x}");
        assert!(result.is_ok(), "render_latex failed: {:?}", result.err());

        let layout = result.unwrap();
        assert!(!layout.items.is_empty(), "Expected at least 1 item");
    }
}
