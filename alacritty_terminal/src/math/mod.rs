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
/// - `font_size_pt`: Font size in points (e.g., 16.0 for 16pt terminal font)
/// - `scale`: Render scale factor (2.0 for retina displays)
/// - `fg_color`: Foreground color RGB
/// - `cell_height`: Terminal cell height in pixels (for vertical alignment)
///
/// Returns RGBA pixel data suitable for creating a GraphicData.
/// The image is padded at the top to align the formula to the cell baseline.
pub fn render_latex_to_png(
    latex: &str,
    font_size_pt: f32,
    scale: f32,
    fg_color: [u8; 3],
    cell_height: usize,
) -> Result<FormulaImage, String> {
    // Convert LaTeX to Typst.
    let typst_math = latex_to_typst(latex)?;

    // Compile to document with specified font size and color.
    let document = compile_math_document(&typst_math, font_size_pt, fg_color).map_err(|e| e.join(", "))?;

    // Get the first page.
    let page = document
        .pages
        .first()
        .ok_or_else(|| "No pages in document".to_string())?;

    // Render to pixmap with scale factor.
    let pixmap = typst_render::render(page, scale);
    let rendered_width = pixmap.width() as usize;
    let rendered_height = pixmap.height() as usize;
    let rendered_pixels = pixmap.take();

    // If the rendered image is shorter than cell_height, add top padding
    // to vertically center the formula within the cell.
    if rendered_height < cell_height {
        let padding_top = (cell_height - rendered_height) / 2;
        let new_height = cell_height;
        let mut new_pixels = vec![0u8; rendered_width * new_height * 4]; // RGBA, transparent

        // Copy rendered pixels to center of new image.
        for y in 0..rendered_height {
            let src_offset = y * rendered_width * 4;
            let dst_offset = (y + padding_top) * rendered_width * 4;
            new_pixels[dst_offset..dst_offset + rendered_width * 4]
                .copy_from_slice(&rendered_pixels[src_offset..src_offset + rendered_width * 4]);
        }

        Ok(FormulaImage {
            width: rendered_width,
            height: new_height,
            pixels: new_pixels,
        })
    } else {
        Ok(FormulaImage {
            width: rendered_width,
            height: rendered_height,
            pixels: rendered_pixels,
        })
    }
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

    /// End-to-end test: verify all formulas render without error.
    /// This tests the full pipeline: LaTeX → MiTeX → Typst → render.
    #[test]
    fn test_e2e_all_formulas() {
        let test_cases = vec![
            // Basic
            (r"x^2", "x-squared"),
            (r"x_1", "x-sub-1"),
            (r"\frac{a}{b}", "frac"),
            // Roots
            (r"\sqrt{x}", "sqrt"),
            (r"\sqrt[3]{8}", "cbrt"),
            // Greek
            (r"\alpha \beta \gamma", "greek"),
            // Operators
            (r"\sum_{i=1}^{n} x_i", "sum"),
            (r"\int_0^1 f(x) dx", "integral"),
            (r"\lim_{x \to 0} f(x)", "limit"),
            // Relations
            (r"a \leq b", "leq"),
            (r"x \in A", "in"),
            // Brackets
            (r"\left( \frac{a}{b} \right)", "left-right"),
            (r"\langle x, y \rangle", "angle-brackets"),
            // Arrows
            (r"\rightarrow \Rightarrow", "arrows"),
            (r"\xleftarrow{abc}", "xarrow"),
            // Accents
            (r"\hat{a} \vec{v} \bar{x}", "accents"),
            (r"\overbrace{a+b}", "overbrace"),
            (r"\underbrace{x+y}", "underbrace"),
            // Font styles
            (r"\mathbf{x}", "mathbf"),
            (r"\mathbb{R}", "mathbb"),
            (r"\mathcal{L}", "mathcal"),
            (r"\text{if } x > 0", "text"),
            // Functions
            (r"\sin x \cos y", "trig"),
            (r"\log x \ln y", "log"),
            // Matrices
            (r"\begin{matrix} a & b \\ c & d \end{matrix}", "matrix"),
            (r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", "pmatrix"),
            (r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}", "bmatrix"),
            (r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}", "vmatrix"),
            (r"\begin{Vmatrix} a & b \\ c & d \end{Vmatrix}", "Vmatrix"),
            // Famous formulas
            (r"e^{i\pi} + 1 = 0", "euler"),
            (r"E = mc^2", "mass-energy"),
            (r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}", "quadratic"),
        ];

        println!("\n=== End-to-End Render Test ===\n");
        let mut failures = Vec::new();

        for (latex, name) in &test_cases {
            match render_latex(latex) {
                Ok(_) => println!("✓ {}: {}", name, latex),
                Err(e) => {
                    println!("✗ {}: {} → ERROR: {}", name, latex, e);
                    failures.push((*name, *latex, e));
                }
            }
        }

        println!("\n=== Summary ===");
        println!("Total: {}, Passed: {}, Failed: {}",
            test_cases.len(),
            test_cases.len() - failures.len(),
            failures.len()
        );

        if !failures.is_empty() {
            println!("\nFailures:");
            for (name, latex, err) in &failures {
                println!("  - {}: {} → {}", name, latex, err);
            }
            panic!("Some formulas failed to render");
        }
    }
}
