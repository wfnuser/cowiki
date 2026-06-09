use crate::{Heading, Metadata};
use scraper::{Html, Selector};

/// Extract the page title from the `<title>` element.
pub fn extract_title(html: &str) -> String {
    let document = Html::parse_document(html);
    let selector = Selector::parse("title").unwrap();
    document
        .select(&selector)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default()
}

/// Extract meta tags (description, author) and Open Graph tags.
pub fn extract_metadata(html: &str, domain: &str) -> Metadata {
    let document = Html::parse_document(html);
    let meta_selector = Selector::parse("meta").unwrap();

    let mut description: Option<String> = None;
    let mut author: Option<String> = None;
    let mut og_title: Option<String> = None;
    let mut og_description: Option<String> = None;
    let mut og_image: Option<String> = None;

    for el in document.select(&meta_selector) {
        let name = el
            .value()
            .attr("name")
            .or_else(|| el.value().attr("property"))
            .map(|s| s.to_lowercase());
        let content = el.value().attr("content").map(|s| s.trim().to_string());

        match (name.as_deref(), content) {
            (Some("description"), Some(c)) if description.is_none() => {
                description = Some(c);
            }
            (Some("author"), Some(c)) if author.is_none() => {
                author = Some(c);
            }
            (Some("og:title"), Some(c)) if og_title.is_none() => {
                og_title = Some(c);
            }
            (Some("og:description"), Some(c)) if og_description.is_none() => {
                og_description = Some(c);
            }
            (Some("og:image"), Some(c)) if og_image.is_none() => {
                og_image = Some(c);
            }
            _ => {}
        }
    }

    Metadata {
        description,
        author,
        domain: domain.to_string(),
        og_title,
        og_description,
        og_image,
    }
}

/// Extract all H1–H6 headings in document order.
pub fn extract_headings(html: &str) -> Vec<Heading> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("h1, h2, h3, h4, h5, h6").unwrap();
    let mut headings = Vec::new();

    for el in document.select(&selector) {
        let tag = el.value().name();
        let level: u8 = tag.chars().nth(1).and_then(|c| c.to_digit(10)).unwrap_or(1) as u8;
        let text = el.text().collect::<String>().trim().to_string();
        if !text.is_empty() {
            headings.push(Heading { level, text });
        }
    }

    headings
}
