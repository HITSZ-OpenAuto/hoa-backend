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
    result = escape_curly_braces_in_math(&result);
    result = convert_hugo_details_to_accordion(&result);

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
        .collect::<Vec<&str>>()
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

/// Escape curly braces inside LaTeX math expressions for MDX compatibility
fn escape_curly_braces_in_math(content: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            // Check if it's display math ($$)
            let is_display = i + 1 < chars.len() && chars[i + 1] == '$';
            let delimiter_len = if is_display { 2 } else { 1 };

            // Find closing delimiter
            let mut j = i + delimiter_len;
            let mut found_close = false;

            while j < chars.len() {
                if chars[j] == '$' {
                    if is_display && j + 1 < chars.len() && chars[j + 1] == '$' {
                        found_close = true;
                        break;
                    } else if !is_display {
                        found_close = true;
                        break;
                    }
                }
                j += 1;
            }

            if found_close {
                // Add opening delimiter
                for _ in 0..delimiter_len {
                    result.push('$');
                }

                // Escape braces in math content
                for k in (i + delimiter_len)..j {
                    if chars[k] == '{' || chars[k] == '}' {
                        // Check if already escaped
                        if k == 0 || chars[k - 1] != '\\' {
                            result.push('\\');
                        }
                    }
                    result.push(chars[k]);
                }

                // Add closing delimiter
                for _ in 0..delimiter_len {
                    result.push('$');
                }

                i = j + delimiter_len;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
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
                for j in (i + 1)..lines.len() {
                    let next_line = lines[j].trim();
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
                    result.extend(accordion_buffer.drain(..));
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
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "mdx"))
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
    fn test_fix_self_closing_tags() {
        let input = "Line 1<br>Line 2<hr>Line 3";
        let output = fix_self_closing_tags(input);
        assert_eq!(output, "Line 1<br />Line 2<hr />Line 3");
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
    fn test_convert_style_to_jsx() {
        let input = r#"<div style="text-align:center;color:red;"></div>"#;
        let output = convert_style_to_jsx(input);
        assert!(output.contains("textAlign"));
        assert!(output.contains("color"));
    }

    #[test]
    fn test_escape_math_braces() {
        let input = "This is $x = {1, 2, 3}$ math";
        let output = escape_curly_braces_in_math(input);
        assert!(output.contains(r"\{"));
        assert!(output.contains(r"\}"));
    }
}
