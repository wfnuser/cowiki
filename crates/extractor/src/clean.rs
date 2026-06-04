use std::borrow::Cow;

/// Noise patterns to strip from extracted text.
/// These are common boilerplate lines that appear in article text.
static NOISE_PATTERNS: &[&str] = &[
    "Cookie Policy",
    "Privacy Policy",
    "Terms of Service",
    "Terms of Use",
    "All rights reserved",
    "Subscribe to our newsletter",
    "Sign up for our newsletter",
    "This site uses cookies",
    "We use cookies",
    "Accept cookies",
    "Cookie settings",
    "Read more",
    "Click here",
    "Share this",
    "Follow us on",
    "Advertisement",
    "Sponsored content",
    "Skip to content",
    "Skip to main content",
    "Back to top",
    "Table of contents",
];

/// Clean extracted text:
/// 1. Remove lines matching noise patterns (case-insensitive substring match).
/// 2. Normalize whitespace within each line (collapse runs of spaces/tabs).
/// 3. Collapse 3+ consecutive blank lines to at most 2.
/// 4. Trim the result.
pub fn clean_text(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    // Process line by line: filter noise, normalize inline whitespace
    let cleaned_lines: Vec<Cow<str>> = input
        .lines()
        .filter(|line| !is_noise_line(line))
        .map(|line| normalize_line(line))
        .collect();

    // Collapse 3+ consecutive newlines to 2.
    // Since each non-blank line contributes a trailing '\n', allowing only 1
    // blank line between content sections keeps the total at "\n\n" (2 newlines).
    let mut result = String::with_capacity(input.len());
    let mut consecutive_blanks: usize = 0;

    for line in &cleaned_lines {
        if line.trim().is_empty() {
            consecutive_blanks += 1;
            if consecutive_blanks <= 1 {
                result.push('\n');
            }
        } else {
            consecutive_blanks = 0;
            result.push_str(line);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

/// Return true if the line contains a noise pattern (case-insensitive).
fn is_noise_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    NOISE_PATTERNS
        .iter()
        .any(|pat| lower.contains(&pat.to_lowercase()))
}

/// Collapse runs of spaces and tabs within a single line into a single space.
fn normalize_line(line: &str) -> Cow<'_, str> {
    // Fast path: if no consecutive whitespace, return as-is
    let bytes = line.as_bytes();
    let mut has_multi_ws = false;
    let mut prev_ws = false;
    for &b in bytes {
        let is_ws = b == b' ' || b == b'\t';
        if is_ws && prev_ws {
            has_multi_ws = true;
            break;
        }
        prev_ws = is_ws;
    }

    if !has_multi_ws {
        return Cow::Borrowed(line);
    }

    let mut out = String::with_capacity(line.len());
    let mut in_ws = false;
    for ch in line.chars() {
        if ch == ' ' || ch == '\t' {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            in_ws = false;
            out.push(ch);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_removes_noise_patterns() {
        let input = "Great article about Rust.\nCookie Policy\nMore content here.\nPrivacy Policy\nEnd.";
        let result = clean_text(input);
        assert!(!result.contains("Cookie Policy"), "should remove Cookie Policy");
        assert!(!result.contains("Privacy Policy"), "should remove Privacy Policy");
        assert!(result.contains("Great article about Rust."), "should keep real content");
        assert!(result.contains("More content here."), "should keep real content");
        assert!(result.contains("End."), "should keep real content");
    }

    #[test]
    fn test_collapses_excessive_newlines() {
        let input = "First paragraph.\n\n\n\n\nSecond paragraph.";
        let result = clean_text(input);
        // Should have at most 2 blank lines between paragraphs
        assert!(!result.contains("\n\n\n"), "should not have 3+ consecutive newlines");
        assert!(result.contains("First paragraph."), "should keep first paragraph");
        assert!(result.contains("Second paragraph."), "should keep second paragraph");
    }

    #[test]
    fn test_normalizes_whitespace() {
        let input = "Word1   Word2\tWord3  Word4";
        let result = clean_text(input);
        assert_eq!(result, "Word1 Word2 Word3 Word4");
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(clean_text(""), "");
    }
}
