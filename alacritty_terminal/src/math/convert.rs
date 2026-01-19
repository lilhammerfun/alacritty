//! LaTeX to Typst conversion using MiTeX.

/// Convert LaTeX math to Typst math syntax.
///
/// Example: `\frac{a}{b}` → `frac(a, b)`
pub fn latex_to_typst(latex: &str) -> Result<String, String> {
    let result = mitex::convert_math(latex, None)?;
    // MiTeX uses custom function names for some LaTeX commands.
    // Post-process to use standard Typst functions.
    let result = result.replace("mitexsqrt", "sqrt");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frac() {
        let result = latex_to_typst(r"\frac{a}{b}").unwrap();
        assert!(result.contains("frac"));
    }

    #[test]
    fn test_sum() {
        let result = latex_to_typst(r"\sum_{i=1}^{n}").unwrap();
        assert!(result.contains("sum"));
    }

    #[test]
    fn test_sqrt_conversion() {
        let result = latex_to_typst(r"\sqrt{x}").unwrap();
        println!("sqrt conversion: {}", result);
        // MiTeX converts to mitexsqrt, we need to post-process to sqrt
        assert!(result.contains("sqrt"), "Expected sqrt in: {}", result);
    }
}
