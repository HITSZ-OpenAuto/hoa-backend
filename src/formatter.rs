use regex::Regex;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Format a single MDX file with all transformations
pub fn format_mdx_file(content: &str) -> String {
    let mut result = content.to_string();

    // Apply all transformations in order
    result = remove_html_comments(&result);
    result = remove_shield_badges(&result);
    result = fix_self_closing_tags(&result);
    result = fix_malformed_html(&result);
    result = convert_style_to_jsx(&result);
    result = convert_hugo_details_to_accordion(&result);
    result = convert_math_blocks(&result);
    result = convert_inline_math(&result);

    // Clean up multiple consecutive blank lines
    let re = Regex::new(r"\n{3,}").unwrap();
    result = re.replace_all(&result, "\n\n").to_string();

    result
}

/// Remove HTML comments from content
fn remove_html_comments(content: &str) -> String {
    let re = Regex::new(r"<!--[\s\S]*?-->").unwrap();
    re.replace_all(content, "").to_string()
}

/// Remove shield.io badges (markdown image syntax)
fn remove_shield_badges(content: &str) -> String {
    content
        .split('\n')
        .filter(|&line| !line.contains("https://img.shields.io"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert HTML tags to self-closing format for MDX compatibility
fn fix_self_closing_tags(content: &str) -> String {
    let mut result = content.to_string();

    // Convert <br> to <br />
    let re_br = Regex::new(r"<br\s*>").unwrap();
    result = re_br.replace_all(&result, "<br />").to_string();

    // Convert <hr> to <hr />
    let re_hr = Regex::new(r"<hr\s*>").unwrap();
    result = re_hr.replace_all(&result, "<hr />").to_string();

    result
}

/// Fix common malformed HTML patterns
fn fix_malformed_html(content: &str) -> String {
    let mut result = content.to_string();

    // Remove empty <tr> tags before closing table
    let re_tr_table = Regex::new(r"<tr>\s*</table>").unwrap();
    result = re_tr_table.replace_all(&result, "</table>").to_string();

    // Remove empty <tr></tr> tags
    let re_empty_tr = Regex::new(r"<tr>\s*</tr>").unwrap();
    result = re_empty_tr.replace_all(&result, "").to_string();

    result
}

/// Convert CSS property name to camelCase for JSX
fn css_property_to_camel_case(prop: &str) -> String {
    let parts: Vec<&str> = prop.trim().split('-').collect();
    if parts.is_empty() {
        return String::new();
    }

    let mut result = parts[0].to_string();
    for part in &parts[1..] {
        if !part.is_empty() {
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_uppercase().next().unwrap());
                result.push_str(chars.as_str());
            }
        }
    }
    result
}

/// Convert HTML style attributes to JSX format
fn convert_style_to_jsx(content: &str) -> String {
    let re = Regex::new(r#"style="([^"]*)""#).unwrap();

    re.replace_all(content, |caps: &regex::Captures| {
        let style_str = &caps[1];
        let mut jsx_props = Vec::new();

        for prop in style_str.split(';') {
            let prop = prop.trim();
            if prop.is_empty() || !prop.contains(':') {
                continue;
            }

            let parts: Vec<&str> = prop.splitn(2, ':').collect();
            if parts.len() == 2 {
                let name = css_property_to_camel_case(parts[0].trim());
                let value = parts[1].trim();
                jsx_props.push(format!("{}: \"{}\"", name, value));
            }
        }

        if jsx_props.is_empty() {
            String::new()
        } else {
            format!("style={{{{{}}}}}", jsx_props.join(", "))
        }
    })
    .to_string()
}

/// Convert Hugo details shortcode to Fumadocs Accordion components
fn convert_hugo_details_to_accordion(content: &str) -> String {
    let mut result = content.to_string();

    // First, handle single-line shortcodes: {{% details title="..." %}} content {{% /details %}}
    let re_single_line =
        Regex::new(r#"\{\{% details title="([^"]*)"[^%]*%\}\}\s*(.+?)\s*\{\{% /details %\}\}"#)
            .unwrap();
    result = re_single_line
        .replace_all(&result, "<Accordion title=\"$1\">\n$2\n</Accordion>")
        .to_string();

    // Convert opening tags
    let re_open = Regex::new(r#"\{\{% details title="([^"]*)"[^%]*%\}\}"#).unwrap();
    result = re_open
        .replace_all(&result, r#"<Accordion title="$1">"#)
        .to_string();

    // Convert closing tags - ensure they're on their own line for MDX compatibility
    // Replace any occurrence where {{% /details %}} appears at end of line content
    let re_closing = Regex::new(r#"([^\n])\s*\{\{% /details %\}\}"#).unwrap();
    result = re_closing
        .replace_all(&result, "$1\n</Accordion>")
        .to_string();

    // Handle any remaining standalone closing tags
    result = result.replace("{{% /details %}}", "</Accordion>");

    // Wrap consecutive Accordion blocks in Accordions
    result = wrap_accordions_in_container(&result);

    result
}

/// Convert block-level math delimiters $$ $$ to ```math code blocks
/// Preserves whether there's a newline after the opening $$
fn convert_math_blocks(content: &str) -> String {
    // First, extract and protect code blocks
    let code_block_re = Regex::new(r"```[\s\S]*?```").unwrap();
    let mut code_blocks = Vec::new();
    let mut protected_content = content.to_string();

    // Replace code blocks with placeholders
    for (i, mat) in code_block_re.find_iter(content).enumerate() {
        code_blocks.push(mat.as_str().to_string());
        let placeholder = format!("___CODE_BLOCK_PLACEHOLDER_{}___", i);
        protected_content = protected_content.replacen(mat.as_str(), &placeholder, 1);
    }

    // Match $$ ... $$ (both inline and block forms) only outside code blocks
    // This regex captures: opening $$, optional newline, content, optional newline, closing $$
    let re = Regex::new(r"\$\$(\r?\n)?([\s\S]*?)(\r?\n)?\$\$").unwrap();

    let result = re.replace_all(&protected_content, |caps: &regex::Captures| {
        let has_opening_newline = caps.get(1).is_some();
        let math_content = &caps[2];
        let has_closing_newline = caps.get(3).is_some();

        // If original format had newlines, preserve them; otherwise add them
        if has_opening_newline && has_closing_newline {
            // Block format: $$\ncontent\n$$ -> ```math\ncontent\n```
            format!("```math\n{}\n```", math_content)
        } else {
            // Inline format: $$content$$ -> ```math\ncontent\n```
            format!("```math\n{}\n```", math_content)
        }
    })
    .to_string();

    // Restore code blocks
    let mut final_result = result;
    for (i, block) in code_blocks.iter().enumerate() {
        let placeholder = format!("___CODE_BLOCK_PLACEHOLDER_{}___", i);
        final_result = final_result.replace(&placeholder, block);
    }

    final_result
}

/// Convert inline math delimiters $ $ to $$ $$
/// Only converts single dollar signs, not double dollar signs
fn convert_inline_math(content: &str) -> String {
    // First, extract and protect code blocks
    let code_block_re = Regex::new(r"```[\s\S]*?```").unwrap();
    let mut code_blocks = Vec::new();
    let mut protected_content = content.to_string();

    // Replace code blocks with placeholders
    for (i, mat) in code_block_re.find_iter(content).enumerate() {
        code_blocks.push(mat.as_str().to_string());
        let placeholder = format!("___CODE_BLOCK_PLACEHOLDER_{}___", i);
        protected_content = protected_content.replacen(mat.as_str(), &placeholder, 1);
    }

    let mut result = String::new();
    let mut chars = protected_content.chars().peekable();
    let mut in_math = false;
    let mut math_buffer = String::new();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            // Check if it's a double $$
            if chars.peek() == Some(&'$') {
                // It's $$, not single $, so just pass through
                result.push(ch);
                continue;
            }

            // Check if previous char was also $
            if result.ends_with('$') {
                // Previous was $, this is second $, so it's $$, just pass through
                result.push(ch);
                continue;
            }

            // It's a single $
            if in_math {
                // Closing $
                result.push_str("$$");
                result.push_str(&math_buffer);
                result.push_str("$$");
                math_buffer.clear();
                in_math = false;
            } else {
                // Opening $
                // Check if the next content doesn't immediately have another $ or newline
                if let Some(&next_ch) = chars.peek() {
                    if next_ch == '\n' {
                        // Single $ before newline, just pass through
                        result.push(ch);
                        continue;
                    }
                }
                in_math = true;
            }
        } else if in_math {
            if ch == '\n' {
                // Newline in math mode means it's not inline math, abort
                result.push('$');
                result.push_str(&math_buffer);
                result.push(ch);
                math_buffer.clear();
                in_math = false;
            } else {
                math_buffer.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    // Handle unclosed math at end
    // If we ended while still in math mode, add the unclosed $
    if in_math {
        result.push('$');
        result.push_str(&math_buffer);
    }

    // Restore code blocks
    let mut final_result = result;
    for (i, block) in code_blocks.iter().enumerate() {
        let placeholder = format!("___CODE_BLOCK_PLACEHOLDER_{}___", i);
        final_result = final_result.replace(&placeholder, block);
    }

    final_result
}

/// Wrap consecutive Accordion blocks in a single Accordions container
fn wrap_accordions_in_container(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut in_sequence = false;
    let mut accordion_buffer = Vec::new();
    let mut depth = 0;

    for (i, line) in lines.iter().enumerate() {
        if line.contains("<Accordion ") && !in_sequence {
            // Start of accordion sequence
            in_sequence = true;
            accordion_buffer.push(line.to_string());
            depth = 1;
        } else if in_sequence {
            accordion_buffer.push(line.to_string());

            // Track depth
            if line.contains("<Accordion ") {
                depth += 1;
            }
            if line.contains("</Accordion>") {
                depth -= 1;
            }

            // Check if sequence ends
            if depth == 0 {
                // Look ahead to see if next non-empty line is another Accordion
                let mut next_is_accordion = false;
                for next_line in lines.iter().skip(i + 1) {
                    let next_line = next_line.trim();
                    if next_line.is_empty() {
                        continue;
                    }
                    if next_line.contains("<Accordion ") {
                        next_is_accordion = true;
                    }
                    break;
                }

                if !next_is_accordion {
                    // End of sequence - wrap and flush
                    result.push("<Accordions>".to_string());
                    result.append(&mut accordion_buffer);
                    result.push("</Accordions>".to_string());
                    in_sequence = false;
                }
            }
        } else {
            result.push(line.to_string());
        }
    }

    // Handle case where file ends with accordion sequence
    if !accordion_buffer.is_empty() {
        result.push("<Accordions>".to_string());
        result.extend(accordion_buffer);
        result.push("</Accordions>".to_string());
    }

    result.join("\n")
}

/// Format all MDX files in a directory recursively
pub fn format_all_mdx_files(docs_dir: &Path) -> crate::error::Result<usize> {
    let mut modified_count = 0;

    for entry in WalkDir::new(docs_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "mdx"))
    {
        let path = entry.path();
        let original = fs::read_to_string(path)?;
        let formatted = format_mdx_file(&original);

        if formatted != original {
            fs::write(path, formatted)?;
            modified_count += 1;
        }
    }

    Ok(modified_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_html_comments() {
        let input = "Hello <!-- comment --> World";
        let output = remove_html_comments(input);
        assert_eq!(output, "Hello  World");
    }

    #[test]
    fn test_remove_html_comments_multiline() {
        let input = "Text <!-- \nmultiline\ncomment\n--> more text";
        let output = remove_html_comments(input);
        assert_eq!(output, "Text  more text");
    }

    #[test]
    fn test_remove_html_comments_multiple() {
        let input = "<!-- first -->text<!-- second -->more";
        let output = remove_html_comments(input);
        assert_eq!(output, "textmore");
    }

    #[test]
    fn test_remove_shield_badges() {
        let input = "# Title\n![badge](https://img.shields.io/badge/test)\nNormal content";
        let output = remove_shield_badges(input);
        assert!(!output.contains("shields.io"));
        assert!(output.contains("Normal content"));
    }

    #[test]
    fn test_fix_self_closing_tags() {
        let input = "Line 1<br>Line 2<hr>Line 3";
        let output = fix_self_closing_tags(input);
        assert_eq!(output, "Line 1<br />Line 2<hr />Line 3");
    }

    #[test]
    fn test_fix_self_closing_tags_with_spaces() {
        let input = "Text<br >more<hr  >end";
        let output = fix_self_closing_tags(input);
        assert_eq!(output, "Text<br />more<hr />end");
    }

    #[test]
    fn test_fix_malformed_html() {
        let input = "<table><tr></table>";
        let output = fix_malformed_html(input);
        assert_eq!(output, "<table></table>");
    }

    #[test]
    fn test_fix_malformed_html_empty_tr() {
        let input = "<table><tr></tr><tr><td>data</td></tr></table>";
        let output = fix_malformed_html(input);
        assert!(!output.contains("<tr></tr>"));
        assert!(output.contains("<td>data</td>"));
    }

    #[test]
    fn test_css_to_camel_case() {
        assert_eq!(css_property_to_camel_case("text-align"), "textAlign");
        assert_eq!(
            css_property_to_camel_case("background-color"),
            "backgroundColor"
        );
        assert_eq!(css_property_to_camel_case("margin"), "margin");
    }

    #[test]
    fn test_css_to_camel_case_edge_cases() {
        assert_eq!(css_property_to_camel_case(""), "");
        assert_eq!(css_property_to_camel_case("font-size"), "fontSize");
        assert_eq!(
            css_property_to_camel_case("border-top-left-radius"),
            "borderTopLeftRadius"
        );
    }

    #[test]
    fn test_convert_style_to_jsx() {
        let input = r#"<div style="text-align:center;color:red;"></div>"#;
        let output = convert_style_to_jsx(input);
        assert!(output.contains("textAlign"));
        assert!(output.contains("color"));
    }

    #[test]
    fn test_convert_style_to_jsx_empty() {
        let input = r#"<div style=""></div>"#;
        let output = convert_style_to_jsx(input);
        assert!(!output.contains("style="));
    }

    #[test]
    fn test_convert_style_to_jsx_complex() {
        let input =
            r#"<div style="margin-top: 10px; padding-left: 20px; background-color: #fff;"></div>"#;
        let output = convert_style_to_jsx(input);
        assert!(output.contains("marginTop"));
        assert!(output.contains("paddingLeft"));
        assert!(output.contains("backgroundColor"));
    }

    #[test]
    fn test_convert_hugo_details_to_accordion() {
        let input = r#"{{% details title="Test" %}}Content here{{% /details %}}"#;
        let output = convert_hugo_details_to_accordion(input);
        assert!(output.contains("<Accordion title=\"Test\">"));
        assert!(output.contains("</Accordion>"));
        assert!(output.contains("Content here"));
    }

    #[test]
    fn test_convert_hugo_details_multiline() {
        let input = r#"{{% details title="Question" %}}
Line 1
Line 2
{{% /details %}}"#;
        let output = convert_hugo_details_to_accordion(input);
        assert!(output.contains("<Accordion title=\"Question\">"));
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
    }

    #[test]
    fn test_wrap_accordions_in_container() {
        let input = r#"<Accordion title="Q1">
A1
</Accordion>
<Accordion title="Q2">
A2
</Accordion>"#;
        let output = wrap_accordions_in_container(input);
        assert!(output.contains("<Accordions>"));
        assert!(output.contains("</Accordions>"));
    }

    #[test]
    fn test_wrap_accordions_single() {
        let input = r#"<Accordion title="Q1">
A1
</Accordion>"#;
        let output = wrap_accordions_in_container(input);
        assert!(output.contains("<Accordions>"));
        assert!(output.contains("</Accordions>"));
    }

    #[test]
    fn test_format_mdx_file_integration() {
        let input = r#"<!-- comment -->
# Title
![badge](https://img.shields.io/test)
<br>
<div style="text-align:center;">Content</div>
Math: $x = {1}$
{{% details title="Test" %}}Answer{{% /details %}}"#;

        let output = format_mdx_file(input);

        // Check all transformations applied
        assert!(!output.contains("<!--"));
        assert!(!output.contains("shields.io"));
        assert!(output.contains("<br />"));
        assert!(output.contains("textAlign"));
        // assert!(output.contains(r"\{"));
        assert!(output.contains("<Accordion"));
    }

    #[test]
    fn test_convert_math_blocks_with_newlines() {
        let input = "Some text\n$$\nx = y + z\n$$\nMore text";
        let output = convert_math_blocks(input);
        assert!(output.contains("```math\nx = y + z\n```"));
        assert!(!output.contains("$$\n"));
    }

    #[test]
    fn test_convert_math_blocks_inline_format() {
        let input = "Some text $$x = y + z$$ more text";
        let output = convert_math_blocks(input);
        assert!(output.contains("```math\nx = y + z\n```"));
        assert!(!output.contains("$$x"));
    }

    #[test]
    fn test_convert_math_blocks_multiline() {
        let input = "Text\n$$\n\\int_0^1 x^2 dx\n= \\frac{1}{3}\n$$\nEnd";
        let output = convert_math_blocks(input);
        assert!(output.contains("```math"));
        assert!(output.contains("\\int_0^1 x^2 dx"));
        assert!(output.contains("= \\frac{1}{3}"));
        assert!(output.contains("```"));
    }

    #[test]
    fn test_convert_inline_math() {
        let input = "The equation $x = y + z$ is simple.";
        let output = convert_inline_math(input);
        assert_eq!(output, "The equation $$x = y + z$$ is simple.");
    }

    #[test]
    fn test_convert_inline_math_multiple() {
        let input = "We have $a = b$ and $c = d$ here.";
        let output = convert_inline_math(input);
        assert_eq!(output, "We have $$a = b$$ and $$c = d$$ here.");
    }

    #[test]
    fn test_convert_inline_math_preserve_content() {
        let input = "Math: $x = {1}$ and $y^2 + z_i$";
        let output = convert_inline_math(input);
        assert_eq!(output, "Math: $$x = {1}$$ and $$y^2 + z_i$$");
    }

    #[test]
    fn test_convert_inline_math_does_not_affect_block_math() {
        // Block math with $$ should not be converted by inline math converter
        let input = "Text $$x = y$$ more";
        let output = convert_inline_math(input);
        assert_eq!(output, input); // Should remain unchanged
    }

    #[test]
    fn test_convert_inline_math_with_newline_block() {
        // Block math with newlines should not be affected
        let input = "$$\nx = y\n$$";
        let output = convert_inline_math(input);
        assert_eq!(output, input); // Should remain unchanged
    }

    #[test]
    fn test_math_conversion_integration() {
        let input = "Text $inline$ math\n$$\nblock\nmath\n$$\nMore $x$ and $$E=mc^2$$";
        let mut output = convert_math_blocks(input);
        output = convert_inline_math(&output);

        assert!(output.contains("$$inline$$"));
        assert!(output.contains("```math\nblock\nmath\n```"));
        assert!(output.contains("$$x$$"));
        assert!(output.contains("```math\nE=mc^2\n```"));
    }

    #[test]
    fn test_convert_math_blocks_preserves_content() {
        let input = "$$\\frac{a}{b}$$";
        let output = convert_math_blocks(input);
        assert_eq!(output, "```math\n\\frac{a}{b}\n```");
    }

    #[test]
    fn test_convert_math_blocks_ignores_code_blocks() {
        // Math inside code blocks should NOT be converted
        let input = "Normal text $$x = y$$\n```markdown\n$$\\sin x$$\n```\nMore $$a = b$$";
        let output = convert_math_blocks(input);

        // Math outside code blocks should be converted
        assert!(output.contains("```math\nx = y\n```"));
        assert!(output.contains("```math\na = b\n```"));

        // Math inside code blocks should remain unchanged
        assert!(output.contains("```markdown\n$$\\sin x$$\n```"));
    }

    #[test]
    fn test_convert_inline_math_ignores_code_blocks() {
        // Inline math inside code blocks should NOT be converted
        let input = "Normal $x$ math\n```javascript\nlet price = $100;\n```\nMore $y$ here";
        let output = convert_inline_math(input);

        // Inline math outside code blocks should be converted
        assert!(output.contains("$$x$$"));
        assert!(output.contains("$$y$$"));

        // Dollar signs inside code blocks should remain unchanged
        assert!(output.contains("```javascript\nlet price = $100;\n```"));
    }

    #[test]
    fn test_code_block_protection_with_multiple_blocks() {
        let input = r#"Text with $inline$ math.
```python
# This has $$math$$ in code
x = $5
```
More $$block$$ math here.
```rust
let formula = "$$E=mc^2$$";
```
Final $a$ inline."#;

        let mut output = convert_math_blocks(input);
        output = convert_inline_math(&output);

        // Check conversions happened outside code blocks
        assert!(output.contains("$$inline$$"));
        assert!(output.contains("```math\nblock\n```"));
        assert!(output.contains("$$a$$"));

        // Check code blocks remained unchanged
        assert!(output.contains("# This has $$math$$ in code"));
        assert!(output.contains("x = $5"));
        assert!(output.contains(r#"let formula = "$$E=mc^2$$";"#));
    }
}
