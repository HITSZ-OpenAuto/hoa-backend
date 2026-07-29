//! Lint MDX source files for issues the formatter currently fixes.
//!
//! Reports `line: rule: message` per issue instead of rewriting content,
//! so problems can be fixed upstream in the course repositories (#14).

use regex::Regex;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Breaks MDX compilation or rendering in Fumadocs
    Error,
    /// Violates an HOA content convention
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LintIssue {
    pub line: usize,
    pub rule: &'static str,
    pub severity: Severity,
    pub message: String,
}

struct LineRule {
    rule: &'static str,
    severity: Severity,
    pattern: &'static LazyLock<Regex>,
    exception: Option<&'static LazyLock<Regex>>,
    message: &'static str,
}

macro_rules! static_regex {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).unwrap());
    };
}

static_regex!(RE_BARE_URL, r"<https?://[^>]+>");
static_regex!(RE_BR_HR, r"<(?:br|hr)\s*>");
static_regex!(RE_EMPTY_TR, r"<tr>\s*</(?:table|tr)>");
static_regex!(RE_STYLE_ATTR, r#"style="[^"]*""#);
static_regex!(RE_HUGO_CALLOUT, r"\{\{[<%]\s*/?callout\b");
static_regex!(RE_HUGO_DETAILS, r"\{\{%\s*/?details\b");
static_regex!(RE_BLOCK_MATH, r"\$\$");
static_regex!(RE_INLINE_MATH, r"(?:^|[^$])\$[^$\n]+\$(?:[^$]|$)");
static_regex!(RE_CODE_FENCE, r"^\s*(```|~~~)");

const LINE_RULES: &[LineRule] = &[
    LineRule {
        rule: "no-bare-url",
        severity: Severity::Error,
        pattern: &RE_BARE_URL,
        exception: None,
        message: "bare URL in angle brackets breaks MDX; use [text](url)",
    },
    LineRule {
        rule: "self-closing-tag",
        severity: Severity::Error,
        pattern: &RE_BR_HR,
        exception: None,
        message: "<br>/<hr> must be self-closing in MDX: <br /> or <hr />",
    },
    LineRule {
        rule: "no-empty-tr",
        severity: Severity::Error,
        pattern: &RE_EMPTY_TR,
        exception: None,
        message: "empty <tr> tag; remove it",
    },
    LineRule {
        rule: "no-html-style-attr",
        severity: Severity::Error,
        pattern: &RE_STYLE_ATTR,
        exception: None,
        message: "string style attribute fails at render in MDX; use style={{...}} JSX syntax",
    },
    LineRule {
        rule: "no-hugo-callout",
        severity: Severity::Error,
        pattern: &RE_HUGO_CALLOUT,
        exception: None,
        message: "Hugo callout shortcode is invalid in MDX; use a > [!NOTE] blockquote alert",
    },
    LineRule {
        rule: "no-hugo-details",
        severity: Severity::Error,
        pattern: &RE_HUGO_DETAILS,
        exception: None,
        message: "Hugo details shortcode is invalid in MDX; use <details><summary>...</summary>",
    },
    LineRule {
        rule: "block-math-style",
        severity: Severity::Warning,
        pattern: &RE_BLOCK_MATH,
        exception: None,
        message: "$$ math delimiters; converted to ```math by hoa-backend until hoa-fuma supports remark-math",
    },
    LineRule {
        rule: "inline-math-style",
        severity: Severity::Warning,
        pattern: &RE_INLINE_MATH,
        exception: None,
        message: "single-$ inline math; converted to $$...$$ by hoa-backend until hoa-fuma supports remark-math",
    },
];

/// Lint a single MDX file's content, returning issues sorted by line
pub fn lint_content(content: &str) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let mut in_code_block = false;

    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;

        if RE_CODE_FENCE.is_match(line) {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        for rule in LINE_RULES {
            if rule.pattern.is_match(line) && !rule.exception.is_some_and(|re| re.is_match(line)) {
                issues.push(LintIssue {
                    line: line_no,
                    rule: rule.rule,
                    severity: rule.severity,
                    message: rule.message.to_string(),
                });
            }
        }
    }

    issues
}

/// Lint a target path: a single markdown file, or a directory of .mdx files.
/// Returns (files with issues, error count, warning count).
pub fn lint_path(target: &Path) -> crate::error::Result<(usize, usize, usize)> {
    let entries: Vec<_> = if target.is_file() {
        vec![target.to_path_buf()]
    } else {
        let mut entries: Vec<_> = fs::read_dir(target)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "mdx"))
            .collect();
        entries.sort();
        entries
    };

    let mut files_with_issues = 0;
    let mut error_count = 0;
    let mut warning_count = 0;

    for path in entries {
        let content = fs::read_to_string(&path)?;
        let issues = lint_content(&content);
        if issues.is_empty() {
            continue;
        }

        files_with_issues += 1;
        let name = path.display();

        for issue in &issues {
            match issue.severity {
                Severity::Error => error_count += 1,
                Severity::Warning => warning_count += 1,
            }
            println!(
                "{}:{}: {}: {} [{}]",
                name, issue.line, issue.severity, issue.message, issue.rule
            );
        }
    }

    Ok((files_with_issues, error_count, warning_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules_of(content: &str) -> Vec<&'static str> {
        lint_content(content).into_iter().map(|i| i.rule).collect()
    }

    #[test]
    fn test_allows_html_comments() {
        // HTML comments are stripped by the formatter, not reported upstream
        assert!(rules_of("Hello <!-- hidden --> world").is_empty());
        assert!(rules_of(r#"<!-- TOML-SECTION: title="教材" -->"#).is_empty());
    }

    #[test]
    fn test_detects_bare_url() {
        assert_eq!(rules_of("See <https://example.com>"), ["no-bare-url"]);
    }

    #[test]
    fn test_detects_br_hr() {
        assert_eq!(rules_of("a<br>b"), ["self-closing-tag"]);
        assert_eq!(rules_of("a<hr >b"), ["self-closing-tag"]);
        assert!(rules_of("a<br />b").is_empty());
    }

    #[test]
    fn test_detects_empty_tr() {
        assert_eq!(rules_of("<tr></tr>"), ["no-empty-tr"]);
        assert_eq!(rules_of("<tr></table>"), ["no-empty-tr"]);
    }

    #[test]
    fn test_detects_style_attr() {
        assert_eq!(
            rules_of(r#"<div style="color: red">x</div>"#),
            ["no-html-style-attr"]
        );
    }

    #[test]
    fn test_detects_hugo_shortcodes() {
        assert_eq!(
            rules_of(r#"{{< callout type="info" >}}"#),
            ["no-hugo-callout"]
        );
        assert_eq!(rules_of("{{< /callout >}}"), ["no-hugo-callout"]);
        assert_eq!(
            rules_of(r#"{{% details title="t" %}}"#),
            ["no-hugo-details"]
        );
        assert_eq!(rules_of("{{% /details %}}"), ["no-hugo-details"]);
    }

    #[test]
    fn test_detects_math_styles() {
        assert_eq!(rules_of("block $$x = y$$ math"), ["block-math-style"]);
        assert_eq!(rules_of("inline $x = y$ math"), ["inline-math-style"]);
    }

    #[test]
    fn test_ignores_code_blocks() {
        let content = "```js\nlet a = $5; // <br> <!-- x -->\n```\nclean line";
        assert!(rules_of(content).is_empty());
    }

    #[test]
    fn test_detects_after_code_block_closes() {
        let content = "```\n$ safe $\n```\nreal $x$ math";
        assert_eq!(rules_of(content), ["inline-math-style"]);
    }

    #[test]
    fn test_allows_shield_badge() {
        assert!(rules_of("![badge](https://img.shields.io/badge/x)").is_empty());
    }

    #[test]
    fn test_line_numbers() {
        let content = "clean\n<br>\nclean\n<https://example.com>";
        let issues = lint_content(content);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].line, 2);
        assert_eq!(issues[1].line, 4);
    }

    #[test]
    fn test_clean_content() {
        let content = "# Title\n\nNormal **markdown** with a [link](https://example.com).\n\n```math\nx = y\n```\n";
        assert!(rules_of(content).is_empty());
    }

    #[test]
    fn test_severities() {
        let issues = lint_content("<br> and $$x$$");
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(issues[1].severity, Severity::Warning);
    }
}
