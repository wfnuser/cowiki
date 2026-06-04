use scraper::{Html, Selector};

/// Extract main content text from HTML.
///
/// Primary: use the `readability` crate's article extractor.
/// Fallback: CSS selector heuristics to find the main content area.
pub fn extract_content(html: &str, url: &str) -> String {
    // Primary: readability extractor
    if let Ok(parsed_url) = url::Url::parse(url) {
        let mut bytes = html.as_bytes();
        match readability::extractor::extract(&mut bytes, &parsed_url) {
            Ok(product) => {
                let text = product.text.trim().to_string();
                if !text.is_empty() {
                    return text;
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "readability extractor failed, falling back to CSS heuristics");
            }
        }
    }

    // Fallback: CSS selector heuristics
    extract_via_selectors(html)
}

/// Try a series of CSS selectors in priority order and return the first non-empty text.
fn extract_via_selectors(html: &str) -> String {
    let document = Html::parse_document(html);

    let candidates = [
        "main",
        "article",
        "[role='main']",
        ".main-content",
        "#main-content",
        ".content",
        "#content",
        ".post",
        ".entry-content",
        "body",
    ];

    for selector_str in &candidates {
        let Ok(selector) = Selector::parse(selector_str) else {
            continue;
        };
        if let Some(el) = document.select(&selector).next() {
            let text: String = el.text().collect::<Vec<_>>().join(" ");
            let text = text.trim().to_string();
            if !text.is_empty() {
                return text;
            }
        }
    }

    String::new()
}
