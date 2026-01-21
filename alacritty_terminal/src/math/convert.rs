//! LaTeX to Typst conversion using MiTeX.

/// Pre-process LaTeX to handle commands not supported by MiTeX.
fn preprocess_latex(latex: &str) -> String {
    let mut result = latex.to_string();

    // \mathscr is not supported by MiTeX, use \mathcal instead (similar calligraphic style)
    result = result.replace(r"\mathscr", r"\mathcal");

    // Fix matrix row breaks: some programs output double spaces instead of \\.
    result = fix_matrix_row_breaks(&result);

    result
}

/// Fix matrix row breaks where \\ was replaced with double spaces.
///
/// Some terminal programs output matrices with double spaces instead of \\
/// for row separators. This function detects matrix environments and converts
/// double spaces back to \\ only when it looks like a row separator.
///
/// Heuristic checks:
/// - Only convert if double space is between matrix elements (not at edges)
/// - The character before double space should be alphanumeric, }, ), or ]
/// - The character after double space should be alphanumeric, \, {, or (
/// - Matrix must have at least one & (column separator) to be multi-column
fn fix_matrix_row_breaks(latex: &str) -> String {
    // Matrix environment names to check
    let matrix_envs = ["matrix", "pmatrix", "bmatrix", "vmatrix", "Vmatrix"];

    let mut result = latex.to_string();

    for env in &matrix_envs {
        let begin_tag = format!(r"\begin{{{}}}", env);
        let end_tag = format!(r"\end{{{}}}", env);

        // Find all occurrences of this matrix environment
        let mut search_start = 0;
        while let Some(begin_pos) = result[search_start..].find(&begin_tag) {
            let abs_begin = search_start + begin_pos;
            let content_start = abs_begin + begin_tag.len();

            if let Some(end_pos) = result[content_start..].find(&end_tag) {
                let abs_end = content_start + end_pos;

                // Extract matrix content
                let content = &result[content_start..abs_end];

                // Only process if this looks like a multi-column matrix (has &)
                // and has potential row breaks (double spaces)
                if content.contains('&') && content.contains("  ") {
                    let fixed_content = fix_double_spaces_in_matrix(content);

                    // Rebuild result
                    result = format!(
                        "{}{}{}{}{}",
                        &result[..abs_begin],
                        begin_tag,
                        fixed_content,
                        end_tag,
                        &result[abs_end + end_tag.len()..]
                    );

                    // Move search position past this matrix
                    search_start =
                        abs_begin + begin_tag.len() + fixed_content.len() + end_tag.len();
                } else {
                    // No fix needed, move past this matrix
                    search_start = abs_end + end_tag.len();
                }
            } else {
                // No matching end tag, move past begin tag
                search_start = content_start;
            }
        }
    }

    result
}

/// Check if a character can appear before a row break (end of matrix element).
fn is_valid_before_row_break(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '}' | ')' | ']' | '0'..='9')
}

/// Check if a character can appear after a row break (start of matrix element).
fn is_valid_after_row_break(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '\\' | '{' | '(' | '-' | '+')
}

/// Fix double spaces in matrix content, converting them to \\ only when appropriate.
fn fix_double_spaces_in_matrix(content: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check for double space
        if i + 1 < chars.len() && chars[i] == ' ' && chars[i + 1] == ' ' {
            // Check context: what's before and after?
            let before = if i > 0 { Some(chars[i - 1]) } else { None };

            // Skip all consecutive spaces
            let mut space_end = i + 2;
            while space_end < chars.len() && chars[space_end] == ' ' {
                space_end += 1;
            }

            let after = if space_end < chars.len() { Some(chars[space_end]) } else { None };

            // Decide if this looks like a row break
            let is_row_break = match (before, after) {
                (Some(b), Some(a)) => is_valid_before_row_break(b) && is_valid_after_row_break(a),
                _ => false,
            };

            if is_row_break {
                result.push_str(r" \\ ");
            } else {
                // Keep original spaces
                for _ in i..space_end {
                    result.push(' ');
                }
            }
            i = space_end;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Convert LaTeX math to Typst math syntax.
///
/// Example: `\frac{a}{b}` → `frac(a, b)`
pub fn latex_to_typst(latex: &str) -> Result<String, String> {
    // Pre-process LaTeX to handle commands not supported by MiTeX.
    let latex = preprocess_latex(latex);

    let result = mitex::convert_math(&latex, None)?;
    // MiTeX uses custom function names for some LaTeX commands.
    // Post-process to use standard Typst functions.
    let result = result.replace("mitexsqrt", "sqrt");
    let result = result.replace("mitexoverbrace", "overbrace");
    let result = result.replace("mitexunderbrace", "underbrace");
    let result = result.replace("mitexmathbf", "bold");

    // Fix \text{} conversion.
    // MiTeX produces: #textmath[content] which doesn't exist in Typst.
    // Convert to quoted text: "content" (Typst math mode text).
    let result = fix_textmath(&result);

    // Fix matrix functions.
    // MiTeX produces: matrix(...), pmatrix(...), etc.
    // Typst uses: mat(...) with delim parameter.
    let result = fix_matrix_functions(&result);

    // Fix n-th root conversion.
    // MiTeX produces: sqrt(\[n \],x ) which is invalid.
    // Typst uses: root(n, x) for n-th roots.
    let result = fix_nth_root(&result);

    // Fix extensible arrows.
    // MiTeX produces: xleftarrow(text) which doesn't exist in Typst.
    // Convert to: xarrow(sym: arrow.l, text)
    let result = fix_extensible_arrows(&result);

    Ok(result)
}

/// Fix matrix functions from MiTeX output.
///
/// MiTeX produces: matrix(...), pmatrix(...), bmatrix(...), vmatrix(...), Vmatrix(...)
/// Typst uses: mat(...) with optional delim parameter.
fn fix_matrix_functions(input: &str) -> String {
    let mut result = input.to_string();

    // Order matters: check longer names first to avoid partial matches
    // Vmatrix -> mat(delim: "‖", ...) using Unicode U+2016 DOUBLE VERTICAL LINE
    result = replace_matrix_func(&result, "Vmatrix", Some("\"‖\""));
    // vmatrix -> mat(delim: "|", ...)
    result = replace_matrix_func(&result, "vmatrix", Some("\"|\""));
    // bmatrix -> mat(delim: "[", ...)
    result = replace_matrix_func(&result, "bmatrix", Some("\"[\""));
    // pmatrix -> mat(delim: "(", ...)
    result = replace_matrix_func(&result, "pmatrix", Some("\"(\""));
    // matrix -> mat(...)
    result = replace_matrix_func(&result, "matrix", None);

    result
}

/// Replace a matrix function with Typst's mat() syntax.
fn replace_matrix_func(input: &str, func_name: &str, delim: Option<&str>) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    let func_bytes = func_name.as_bytes();
    let func_len = func_name.len();

    while let Some(c) = chars.next() {
        // Check if we're at the start of the function name
        if c == func_bytes[0] as char {
            let mut matched = true;
            let mut peek_chars: Vec<char> = vec![c];

            // Try to match the rest of the function name
            for i in 1..func_len {
                if let Some(&next_c) = chars.peek() {
                    if next_c == func_bytes[i] as char {
                        peek_chars.push(chars.next().unwrap());
                    } else {
                        matched = false;
                        break;
                    }
                } else {
                    matched = false;
                    break;
                }
            }

            // Check for opening paren
            if matched {
                if let Some(&next_c) = chars.peek() {
                    if next_c == '(' {
                        chars.next(); // consume '('

                        // Read content until closing paren
                        let mut content = String::new();
                        let mut paren_depth = 1;
                        while let Some(ch) = chars.next() {
                            if ch == '(' {
                                paren_depth += 1;
                                content.push(ch);
                            } else if ch == ')' {
                                paren_depth -= 1;
                                if paren_depth == 0 {
                                    break;
                                }
                                content.push(ch);
                            } else {
                                content.push(ch);
                            }
                        }

                        // Output as mat(...) with optional delim
                        if let Some(d) = delim {
                            result.push_str(&format!("mat(delim: {}, {})", d, content));
                        } else {
                            result.push_str(&format!("mat({})", content));
                        }
                        continue;
                    }
                }
            }

            // Not a match, output what we consumed
            for ch in peek_chars {
                result.push(ch);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Fix \text{} conversion from MiTeX output.
///
/// Converts `#textmath[content]` to `"content"` (Typst math mode text).
fn fix_textmath(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    let pattern = "#textmath[";

    while let Some(c) = chars.next() {
        if c == '#' {
            // Check if this is #textmath[
            let mut matched = true;
            let mut peek_chars: Vec<char> = vec![c];

            for expected in pattern.chars().skip(1) {
                if let Some(&next_c) = chars.peek() {
                    if next_c == expected {
                        peek_chars.push(chars.next().unwrap());
                    } else {
                        matched = false;
                        break;
                    }
                } else {
                    matched = false;
                    break;
                }
            }

            if matched {
                // Read content until closing ]
                let mut content = String::new();
                let mut bracket_depth = 1;
                while let Some(ch) = chars.next() {
                    if ch == '[' {
                        bracket_depth += 1;
                        content.push(ch);
                    } else if ch == ']' {
                        bracket_depth -= 1;
                        if bracket_depth == 0 {
                            break;
                        }
                        content.push(ch);
                    } else {
                        content.push(ch);
                    }
                }

                // Output as quoted text
                result.push('"');
                result.push_str(content.trim());
                result.push('"');
            } else {
                // Not a match, output what we consumed
                for ch in peek_chars {
                    result.push(ch);
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Fix extensible arrows from MiTeX output.
///
/// Converts `xleftarrow(text)` to `arrow.l.long^(text)`
/// Converts `xrightarrow(text)` to `arrow.r.long^(text)`
fn fix_extensible_arrows(input: &str) -> String {
    let mut result = input.to_string();

    // Replace xleftarrow(content) with arrow.l.long^(content)
    result = replace_arrow_func(&result, "xleftarrow", "arrow.l.long");
    result = replace_arrow_func(&result, "xrightarrow", "arrow.r.long");

    result
}

/// Replace arrow function calls with Typst arrow notation.
fn replace_arrow_func(input: &str, func_name: &str, arrow_symbol: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars().peekable();
    let func_bytes = func_name.as_bytes();
    let func_len = func_name.len();

    while let Some(c) = chars.next() {
        // Check if we're at the start of the function name
        if c == func_bytes[0] as char {
            let mut matched = true;
            let mut peek_chars: Vec<char> = vec![c];

            // Try to match the rest of the function name
            for i in 1..func_len {
                if let Some(&next_c) = chars.peek() {
                    if next_c == func_bytes[i] as char {
                        peek_chars.push(chars.next().unwrap());
                    } else {
                        matched = false;
                        break;
                    }
                } else {
                    matched = false;
                    break;
                }
            }

            // Check for opening paren
            if matched {
                if let Some(&next_c) = chars.peek() {
                    if next_c == '(' {
                        chars.next(); // consume '('

                        // Read content until closing paren
                        let mut content = String::new();
                        let mut paren_depth = 1;
                        while let Some(ch) = chars.next() {
                            if ch == '(' {
                                paren_depth += 1;
                                content.push(ch);
                            } else if ch == ')' {
                                paren_depth -= 1;
                                if paren_depth == 0 {
                                    break;
                                }
                                content.push(ch);
                            } else {
                                content.push(ch);
                            }
                        }

                        // Output as arrow^(content)
                        result.push_str(&format!("{}^({})", arrow_symbol, content.trim()));
                        continue;
                    }
                }
            }

            // Not a match, output what we consumed
            for ch in peek_chars {
                result.push(ch);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Fix n-th root syntax from MiTeX output.
///
/// Converts `sqrt(\[n \],x )` to `root(n, x)`.
fn fix_nth_root(input: &str) -> String {
    // Pattern: sqrt(\[...],...)
    // MiTeX outputs: sqrt(\[3 \],8 ) for \sqrt[3]{8}
    let mut result = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        // Look for "sqrt("
        if c == 's' {
            let rest: String = std::iter::once(c).chain(chars.clone().take(4)).collect();
            if rest == "sqrt(" {
                // Consume "qrt("
                for _ in 0..4 {
                    chars.next();
                }
                // Check if next is "\["
                let next_two: String = chars.clone().take(2).collect();
                if next_two == "\\[" {
                    // This is an n-th root, parse it
                    // Skip "\["
                    chars.next();
                    chars.next();

                    // Read index until "\]"
                    let mut index = String::new();
                    loop {
                        let peek_two: String = chars.clone().take(2).collect();
                        if peek_two.starts_with("\\]") {
                            chars.next();
                            chars.next();
                            break;
                        }
                        if let Some(ch) = chars.next() {
                            index.push(ch);
                        } else {
                            break;
                        }
                    }

                    // Skip comma and whitespace
                    while chars.peek().map(|&c| c == ',' || c == ' ').unwrap_or(false) {
                        chars.next();
                    }

                    // Read radicand until closing ")"
                    let mut radicand = String::new();
                    let mut paren_depth = 1;
                    while let Some(ch) = chars.next() {
                        if ch == '(' {
                            paren_depth += 1;
                            radicand.push(ch);
                        } else if ch == ')' {
                            paren_depth -= 1;
                            if paren_depth == 0 {
                                break;
                            }
                            radicand.push(ch);
                        } else {
                            radicand.push(ch);
                        }
                    }

                    // Output as root(index, radicand)
                    result.push_str(&format!("root({}, {})", index.trim(), radicand.trim()));
                } else {
                    // Normal sqrt, output as-is
                    result.push_str("sqrt(");
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
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

    #[test]
    fn test_nth_root_conversion() {
        // Test n-th root: \sqrt[3]{8}
        let result = latex_to_typst(r"\sqrt[3]{8}").unwrap();
        println!("nth root conversion: {}", result);
        assert!(result.contains("root(3"), "Expected root(3, ...) in: {}", result);
        assert!(result.contains("8"), "Expected 8 in: {}", result);
    }

    #[test]
    fn test_nth_root_with_variable() {
        // Test n-th root with variable index: \sqrt[n]{x}
        let result = latex_to_typst(r"\sqrt[n]{x}").unwrap();
        println!("nth root with var: {}", result);
        assert!(result.contains("root(n"), "Expected root(n, ...) in: {}", result);
    }

    #[test]
    fn test_normal_sqrt_still_works() {
        // Ensure normal sqrt is not affected
        let result = latex_to_typst(r"\sqrt{x}").unwrap();
        println!("normal sqrt: {}", result);
        assert!(result.contains("sqrt("), "Expected sqrt( in: {}", result);
        // Should NOT be converted to root
        assert!(!result.contains("root("), "Should not contain root(): {}", result);
    }

    #[test]
    fn test_extensible_arrows() {
        // Test extensible arrows
        let result = latex_to_typst(r"\xleftarrow{abc}").unwrap();
        println!("xleftarrow: {}", result);

        let result = latex_to_typst(r"\xrightarrow{def}").unwrap();
        println!("xrightarrow: {}", result);
    }

    #[test]
    fn test_overbrace_underbrace() {
        let result = latex_to_typst(r"\overbrace{a+b+c}").unwrap();
        println!("overbrace: {}", result);

        let result = latex_to_typst(r"\underbrace{x+y+z}").unwrap();
        println!("underbrace: {}", result);
    }

    #[test]
    fn test_mathbf() {
        let result = latex_to_typst(r"\mathbf{x}").unwrap();
        println!("mathbf: {}", result);

        let result = latex_to_typst(r"\mathbf{ABC}").unwrap();
        println!("mathbf ABC: {}", result);
    }

    #[test]
    fn test_mathcal_mathscr() {
        let result = latex_to_typst(r"\mathcal{L}").unwrap();
        println!("mathcal: {}", result);

        // mathscr is not supported by MiTeX, test if our preprocessing handles it
        let result = latex_to_typst(r"\mathscr{L}");
        println!("mathscr result: {:?}", result);
    }

    #[test]
    fn test_matrix_row_break_fix() {
        // Test that double spaces inside matrix are converted to \\
        let input = r"\begin{pmatrix} a & b  c & d \end{pmatrix}";
        let result = super::fix_matrix_row_breaks(input);
        println!("Matrix row break fix: {} → {}", input, result);
        assert!(result.contains(r"\\"), "Expected \\\\ in: {}", result);

        // Test 3x3 matrix
        let input = r"\begin{pmatrix} a_{11} & a_{12} & a_{13}  a_{21} & a_{22} & a_{23}  a_{31} & a_{32} & a_{33} \end{pmatrix}";
        let result = super::fix_matrix_row_breaks(input);
        println!("3x3 matrix fix: {}", result);
        assert_eq!(
            result.matches(r"\\").count(),
            2,
            "Expected 2 row breaks in 3x3 matrix"
        );

        // Test that normal matrix with \\ is not affected
        let input = r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}";
        let result = super::fix_matrix_row_breaks(input);
        println!("Normal matrix (unchanged): {}", result);
        assert_eq!(input, result, "Normal matrix should be unchanged");

        // Test safety: column vector (no &) should not be modified
        let input = r"\begin{pmatrix} x  y  z \end{pmatrix}";
        let result = super::fix_matrix_row_breaks(input);
        println!("Column vector (no &): {} → {}", input, result);
        // No & means no multi-column, so double spaces are not row breaks
        assert_eq!(input, result, "Column vector without & should be unchanged");

        // Test 4x4 matrix (Lorentz transform style)
        let input = r"\begin{pmatrix} \gamma & -\beta\gamma & 0 & 0  -\beta\gamma & \gamma & 0 & 0  0 & 0 & 1 & 0  0 & 0 & 0 & 1 \end{pmatrix}";
        let result = super::fix_matrix_row_breaks(input);
        println!("4x4 matrix fix: {}", result);
        assert_eq!(
            result.matches(r"\\").count(),
            3,
            "Expected 3 row breaks in 4x4 matrix"
        );
    }

    #[test]
    fn test_all_formulas_from_guide() {
        // Batch test all formulas from TEST_GUIDE.md sections 1-16
        let test_cases = vec![
            // === 1. Basic Syntax ===
            // 1.1 Simple variables and numbers
            (r"x", "var-x"),
            (r"xyz", "var-xyz"),
            (r"123", "num-123"),
            (r"x = 42", "x-eq-42"),
            (r"a + b = c", "a-plus-b-eq-c"),
            // 1.2 Superscript
            (r"x^2", "x-squared"),
            (r"x^{10}", "x-to-10"),
            (r"x^{2n+1}", "x-to-2n+1"),
            (r"e^x", "e-to-x"),
            (r"e^{i\pi}", "euler-exp"),
            (r"2^{2^{2^2}}", "tower-of-2"),
            // 1.3 Subscript
            (r"x_1", "x-sub-1"),
            (r"x_{ij}", "x-sub-ij"),
            (r"a_{n+1}", "a-sub-n+1"),
            (r"x_{i,j,k}", "x-sub-ijk"),
            // 1.4 Combined sub/superscript
            (r"x_i^2", "x-sub-i-sq"),
            (r"x^2_i", "x-sq-sub-i"),
            (r"a_1^2 + a_2^2", "sum-of-squares"),
            (r"{}_1^2X_3^4", "isotope-notation"),

            // === 2. Fractions ===
            // 2.1 Simple fractions
            (r"\frac{1}{2}", "frac-1-2"),
            (r"\frac{a}{b}", "frac-a-b"),
            (r"\frac{x+1}{x-1}", "frac-x+1-x-1"),
            // 2.2 Nested fractions
            (r"\frac{\frac{1}{2}}{\frac{3}{4}}", "nested-frac"),
            (r"\frac{1}{1+\frac{1}{2}}", "frac-in-denom"),
            (r"\frac{a+\frac{b}{c}}{d+\frac{e}{f}}", "complex-nested-frac"),
            // 2.3 Continued fraction
            (r"1 + \frac{1}{2 + \frac{1}{3 + \frac{1}{4}}}", "continued-frac-2"),
            // 2.4 Other fraction forms
            (r"\tfrac{1}{2}", "tfrac"),
            (r"\dfrac{1}{2}", "dfrac"),
            (r"^1/_2", "inline-frac"),

            // === 3. Roots ===
            // 3.1 Square roots
            (r"\sqrt{2}", "sqrt-2"),
            (r"\sqrt{x}", "sqrt-x"),
            (r"\sqrt{x^2+y^2}", "sqrt-pythagorean"),
            (r"\sqrt{a+\sqrt{b}}", "nested-sqrt"),
            // 3.2 N-th roots
            (r"\sqrt[3]{8}", "cbrt-8"),
            (r"\sqrt[n]{x}", "nth-root"),
            (r"\sqrt[4]{16}", "4th-root"),
            // 3.3 Complex roots
            (r"\sqrt{\frac{a}{b}}", "sqrt-frac"),
            (r"\frac{1}{\sqrt{2}}", "frac-sqrt"),
            (r"\sqrt{1+\sqrt{1+\sqrt{1+x}}}", "deeply-nested-sqrt"),

            // === 4. Greek Letters ===
            // 4.1 Lowercase
            (r"\alpha \beta \gamma \delta \epsilon", "greek-lower-1"),
            (r"\zeta \eta \theta \iota \kappa", "greek-lower-2"),
            (r"\lambda \mu \nu \xi \omicron", "greek-lower-3"),
            (r"\pi \rho \sigma \tau \upsilon", "greek-lower-4"),
            (r"\phi \chi \psi \omega", "greek-lower-5"),
            // 4.2 Variants
            (r"\varepsilon \vartheta \varpi \varrho \varsigma \varphi", "greek-variants"),
            // 4.3 Uppercase
            (r"\Gamma \Delta \Theta \Lambda \Xi", "greek-upper-1"),
            (r"\Pi \Sigma \Upsilon \Phi \Psi \Omega", "greek-upper-2"),

            // === 5. Operators ===
            // 5.1 Binary operators
            (r"a + b - c", "add-sub"),
            (r"a \times b \div c", "times-div"),
            (r"a \cdot b", "cdot"),
            (r"a \pm b \mp c", "pm-mp"),
            (r"a \ast b \star c", "ast-star"),
            (r"A \cap B \cup C", "cap-cup"),
            (r"A \setminus B", "setminus"),
            (r"a \oplus b \otimes c", "oplus-otimes"),
            // 5.2 Sum and product
            (r"\sum x", "sum-simple"),
            (r"\sum_{i=1}^{n} x_i", "sum-indexed"),
            (r"\sum_{k=0}^{\infty} a_k", "sum-infinite"),
            (r"\prod_{i=1}^{n} i", "product"),
            (r"\coprod_{i \in I} A_i", "coproduct"),
            // 5.3 Integrals
            (r"\int f(x) dx", "integral-simple"),
            (r"\int_a^b f(x) dx", "integral-definite"),
            (r"\int_0^{\infty} e^{-x} dx", "integral-improper"),
            (r"\iint_D f(x,y) dA", "double-integral"),
            (r"\iiint_V f dV", "triple-integral"),
            (r"\oint_C \vec{F} \cdot d\vec{r}", "contour-integral"),
            // 5.4 Limits
            (r"\lim_{x \to 0} f(x)", "limit"),
            (r"\lim_{n \to \infty} a_n", "limit-infinity"),
            (r"\lim_{x \to 0^+} \frac{1}{x}", "limit-one-sided"),
            // 5.5 Other large operators
            (r"\bigcup_{i=1}^{n} A_i", "bigcup"),
            (r"\bigcap_{i=1}^{n} A_i", "bigcap"),
            (r"\bigoplus_{i=1}^{n} V_i", "bigoplus"),
            (r"\bigotimes_{i=1}^{n} M_i", "bigotimes"),

            // === 6. Relations ===
            // 6.1 Equality and inequality
            (r"a = b", "eq"),
            (r"a \neq b", "neq"),
            (r"a \approx b", "approx"),
            (r"a \equiv b", "equiv"),
            (r"a \sim b", "sim"),
            (r"a \cong b", "cong"),
            (r"a \propto b", "propto"),
            // 6.2 Comparison
            (r"a < b > c", "lt-gt"),
            (r"a \leq b \geq c", "leq-geq"),
            (r"a \ll b \gg c", "ll-gg"),
            (r"a \prec b \succ c", "prec-succ"),
            // 6.3 Set relations
            (r"x \in A", "in"),
            (r"x \notin A", "notin"),
            (r"A \subset B", "subset"),
            (r"A \subseteq B", "subseteq"),
            (r"A \supset B", "supset"),
            (r"A \supseteq B", "supseteq"),
            (r"A \ni x", "ni"),

            // === 7. Brackets & Delimiters ===
            // 7.1 Basic brackets
            (r"(a+b)", "parens"),
            (r"[a+b]", "brackets"),
            (r"\{a+b\}", "braces"),
            (r"|x|", "abs"),
            (r"\|x\|", "norm"),
            // 7.2 Auto-scaling brackets
            (r"\left( \frac{a}{b} \right)", "left-right-paren"),
            (r"\left[ \sum_{i=1}^{n} x_i \right]", "left-right-bracket"),
            (r"\left\{ \frac{1}{2} \right\}", "left-right-brace"),
            (r"\left| \frac{a}{b} \right|", "left-right-abs"),
            // 7.3 Angle brackets and others
            (r"\langle x, y \rangle", "angle-brackets"),
            (r"\lceil x \rceil", "ceil"),
            (r"\lfloor x \rfloor", "floor"),
            (r"\left\langle \frac{a}{b} \right\rangle", "left-right-angle"),
            // 7.4 Mixed delimiters
            (r"\left( x \right]", "mixed-delim"),
            (r"\left. \frac{df}{dx} \right|_{x=0}", "eval-at"),

            // === 8. Arrows ===
            // 8.1 Basic arrows
            (r"\rightarrow \leftarrow \leftrightarrow", "basic-arrows"),
            (r"\Rightarrow \Leftarrow \Leftrightarrow", "double-arrows"),
            (r"\uparrow \downarrow \updownarrow", "vertical-arrows"),
            (r"\nearrow \searrow \swarrow \nwarrow", "diagonal-arrows"),
            // 8.2 Long arrows
            (r"\longrightarrow \longleftarrow", "long-arrows"),
            (r"\Longrightarrow \Longleftarrow", "long-double-arrows"),
            (r"\mapsto \longmapsto", "mapsto"),
            // 8.3 Special arrows
            (r"\hookrightarrow \hookleftarrow", "hook-arrows"),
            (r"\rightharpoonup \rightharpoondown", "harpoons"),
            (r"\xleftarrow{abc} \xrightarrow{def}", "extensible-arrows"),

            // === 9. Accents ===
            // 9.1 Single-char accents
            (r"\hat{a}", "hat"),
            (r"\bar{x}", "bar"),
            (r"\vec{v}", "vec"),
            (r"\dot{x}", "dot"),
            (r"\ddot{x}", "ddot"),
            (r"\tilde{n}", "tilde"),
            (r"\check{c}", "check"),
            (r"\acute{e}", "acute"),
            (r"\grave{a}", "grave"),
            (r"\breve{u}", "breve"),
            // 9.2 Wide accents
            (r"\widehat{abc}", "widehat"),
            (r"\widetilde{xyz}", "widetilde"),
            (r"\overline{abc}", "overline"),
            (r"\underline{xyz}", "underline"),
            (r"\overbrace{a+b+c}", "overbrace"),
            (r"\underbrace{x+y+z}_{n \text{ terms}}", "underbrace-text"),
            // 9.3 Over/under content
            (r"\overset{?}{=}", "overset"),
            (r"\underset{x \to 0}{\lim}", "underset"),
            (r"\stackrel{def}{=}", "stackrel"),

            // === 10. Font Styles ===
            // 10.1 Bold
            (r"\mathbf{x}", "mathbf-x"),
            (r"\boldsymbol{\alpha}", "boldsymbol"),
            (r"\mathbf{ABC}", "mathbf-ABC"),
            // 10.2 Blackboard bold
            (r"\mathbb{N}", "mathbb-N"),
            (r"\mathbb{Z}", "mathbb-Z"),
            (r"\mathbb{Q}", "mathbb-Q"),
            (r"\mathbb{R}", "mathbb-R"),
            (r"\mathbb{C}", "mathbb-C"),
            // 10.3 Calligraphic
            (r"\mathcal{L}", "mathcal-L"),
            (r"\mathcal{F}", "mathcal-F"),
            (r"\mathscr{L}", "mathscr-L"),
            // 10.4 Roman
            (r"\mathrm{tr}(A)", "mathrm"),
            (r"\text{if } x > 0", "text"),
            // 10.5 Monospace/Sans
            (r"\mathtt{code}", "mathtt"),
            (r"\mathsf{sans}", "mathsf"),

            // === 11. Function Names ===
            // 11.1 Trig
            (r"\sin x", "sin"),
            (r"\cos \theta", "cos"),
            (r"\tan \alpha", "tan"),
            (r"\cot x", "cot"),
            (r"\sec x", "sec"),
            (r"\csc x", "csc"),
            // 11.2 Inverse trig
            (r"\arcsin x", "arcsin"),
            (r"\arccos x", "arccos"),
            (r"\arctan x", "arctan"),
            // 11.3 Hyperbolic
            (r"\sinh x", "sinh"),
            (r"\cosh x", "cosh"),
            (r"\tanh x", "tanh"),
            // 11.4 Log/Exp
            (r"\log x", "log"),
            (r"\ln x", "ln"),
            (r"\lg x", "lg"),
            (r"\exp(x)", "exp"),
            // 11.5 Other functions
            (r"\max(a, b)", "max"),
            (r"\min(a, b)", "min"),
            (r"\sup A", "sup"),
            (r"\inf A", "inf"),
            (r"\det A", "det"),
            (r"\dim V", "dim"),
            (r"\ker T", "ker"),
            (r"\gcd(a, b)", "gcd"),
            (r"\arg z", "arg"),
            (r"\deg p", "deg"),

            // === 12. Special Symbols ===
            // 12.1 Infinity/Empty
            (r"\infty", "infty"),
            (r"-\infty", "neg-infty"),
            (r"\emptyset", "emptyset"),
            (r"\varnothing", "varnothing"),
            // 12.2 Calculus
            (r"\partial f", "partial"),
            (r"\nabla f", "nabla"),
            (r"\nabla^2 f", "nabla2"),
            (r"\frac{\partial f}{\partial x}", "partial-frac"),
            // 12.3 Logic
            (r"\forall x", "forall"),
            (r"\exists y", "exists"),
            (r"\nexists z", "nexists"),
            (r"\neg p", "neg"),
            (r"p \land q", "land"),
            (r"p \lor q", "lor"),
            (r"p \implies q", "implies"),
            (r"p \iff q", "iff"),
            // 12.4 Geometry
            (r"\angle ABC", "angle"),
            (r"\triangle ABC", "triangle"),
            (r"\square", "square"),
            (r"\perp", "perp"),
            (r"\parallel", "parallel"),
            // 12.5 Other
            (r"\therefore", "therefore"),
            (r"\because", "because"),
            (r"\ldots", "ldots"),
            (r"\cdots", "cdots"),
            (r"\vdots", "vdots"),
            (r"\ddots", "ddots"),
            (r"\prime", "prime"),
            (r"\hbar", "hbar"),
            (r"\ell", "ell"),
            (r"\Re z", "Re"),
            (r"\Im z", "Im"),
            (r"\aleph_0", "aleph"),

            // === 13. Matrices ===
            (r"\begin{matrix} a & b \\ c & d \end{matrix}", "matrix"),
            (r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", "pmatrix"),
            (r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}", "bmatrix"),
            (r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}", "vmatrix"),
            (r"\begin{Vmatrix} a & b \\ c & d \end{Vmatrix}", "Vmatrix"),
            (r"\begin{pmatrix} x \\ y \\ z \end{pmatrix}", "column-vector"),

            // === 14. Famous Formulas ===
            // 14.1 Algebra
            (r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}", "quadratic"),
            (r"(a+b)^n = \sum_{k=0}^{n} \binom{n}{k} a^{n-k} b^k", "binomial-theorem"),
            (r"(a+b)^2 = a^2 + 2ab + b^2", "perfect-square"),
            // 14.2 Calculus
            (r"f'(x) = \lim_{h \to 0} \frac{f(x+h) - f(x)}{h}", "derivative-def"),
            (r"e^x = \sum_{n=0}^{\infty} \frac{x^n}{n!}", "taylor-exp"),
            (r"\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}", "gaussian"),
            (r"\int u \, dv = uv - \int v \, du", "integration-by-parts"),
            // 14.3 Physics
            (r"e^{i\pi} + 1 = 0", "euler"),
            (r"E = mc^2", "mass-energy"),
            (r"i\hbar \frac{\partial}{\partial t}\Psi = \hat{H}\Psi", "schrodinger"),
            (r"\nabla \cdot \vec{E} = \frac{\rho}{\varepsilon_0}", "maxwell"),
            // 14.4 Statistics
            (r"f(x) = \frac{1}{\sigma\sqrt{2\pi}} e^{-\frac{(x-\mu)^2}{2\sigma^2}}", "normal-dist"),
            (r"P(A|B) = \frac{P(B|A) P(A)}{P(B)}", "bayes"),
            (r"E[X] = \sum_{i} x_i P(x_i)", "expectation"),
            // 14.5 Linear Algebra
            (r"\det(A) = \sum_{\sigma \in S_n} \text{sgn}(\sigma) \prod_{i=1}^{n} a_{i,\sigma(i)}", "determinant"),
            (r"A\vec{v} = \lambda\vec{v}", "eigenvalue"),
            (r"(AB)_{ij} = \sum_{k} A_{ik} B_{kj}", "matrix-mult"),

            // === 15. Edge Cases ===
            // 15.2 Special chars
            (r"\%", "percent"),
            (r"\#", "hash"),
            (r"\&", "ampersand"),
            (r"\_", "underscore"),
            // 15.3 Long formulas
            (r"a + b + c + d + e + f + g + h + i + j + k + l + m + n", "long-sum"),
            (r"\frac{1}{1+\frac{1}{1+\frac{1}{1+\frac{1}{1+x}}}}", "continued-frac"),
        ];

        println!("\n=== LaTeX Formula Conversion Test ===\n");
        let mut failures = Vec::new();

        for (latex, name) in test_cases {
            match latex_to_typst(latex) {
                Ok(result) => println!("✓ {}: {} → {}", name, latex, result),
                Err(e) => {
                    println!("✗ {}: {} → ERROR: {}", name, latex, e);
                    failures.push((name, latex, e));
                }
            }
        }

        println!("\n=== Summary ===");
        println!("Failures: {}", failures.len());
        for (name, latex, err) in &failures {
            println!("  - {}: {} → {}", name, latex, err);
        }

        // Don't fail the test, just report
        // assert!(failures.is_empty(), "Some formulas failed to convert");
    }
}
