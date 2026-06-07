use std::io::{Cursor, Read};

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};
use crate::decode_base64;

pub struct PptxExtractor;

/// Extract text from PPTX slide XML.
///
/// Recursively finds ALL `<a:t>` text elements anywhere in the XML tree
/// (paragraphs, table cells, group shapes, SmartArt, etc.).
/// Inserts newlines at paragraph (`<a:p>`) boundaries.
fn extract_slide_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut text = String::new();
    let mut para_depth = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == b"p" {
                    para_depth += 1;
                }
            }
            Ok(Event::Text(ref e)) => {
                if para_depth > 0 {
                    if let Ok(t) = e.unescape() {
                        let trimmed = t.trim();
                        if !trimmed.is_empty() {
                            text.push_str(trimmed);
                            text.push(' ');
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == b"p" {
                    para_depth -= 1;
                    // Trim trailing space and add newline
                    text = text.trim_end().to_string();
                    text.push('\n');
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    text
}

/// Extract speaker notes from a notes slide XML.
fn extract_notes_text(xml: &str) -> String {
    extract_slide_text(xml)
}

#[async_trait]
impl SourceExtractor for PptxExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Pptx]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let bytes = if input.encoding.as_deref() == Some("base64") {
            decode_base64(&input.content)?
        } else {
            return Err(ExtractError::InvalidInput(
                "PPTX requires base64 encoding".into(),
            ));
        };

        let original = bytes.clone();
        let cursor = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| ExtractError::ParseError(format!("PPTX is not a valid ZIP: {}", e)))?;

        // Collect slide XML files and notes files
        let mut slide_numbers: Vec<usize> = Vec::new();
        let mut has_notes: Vec<(usize, String)> = Vec::new(); // (slide_num, path)

        for i in 0..archive.len() {
            let file = archive.by_index(i)
                .map_err(|e| ExtractError::ParseError(format!("ZIP entry error: {}", e)))?;
            let name = file.name().to_string();

            // Match slide XML: ppt/slides/slideN.xml
            if let Some(rest) = name.strip_prefix("ppt/slides/slide") {
                if let Some(num_str) = rest.strip_suffix(".xml") {
                    if let Ok(n) = num_str.parse::<usize>() {
                        slide_numbers.push(n);
                    }
                }
            }

            // Match notes: ppt/notesSlides/notesSlideN.xml
            if let Some(rest) = name.strip_prefix("ppt/notesSlides/notesSlide") {
                if rest.ends_with(".xml") {
                    let num_str = rest.trim_end_matches(".xml");
                    if let Ok(n) = num_str.parse::<usize>() {
                        has_notes.push((n, name));
                    }
                }
            }
        }
        slide_numbers.sort_unstable();

        let mut markdown = String::new();
        let mut metadata = ExtractMetadata::default();
        metadata.page_count = Some(slide_numbers.len());

        for slide_num in &slide_numbers {
            let slide_path = format!("ppt/slides/slide{}.xml", slide_num);

            let slide_text = if let Ok(mut file) = archive.by_name(&slide_path) {
                let mut xml = String::new();
                if file.read_to_string(&mut xml).is_ok() {
                    extract_slide_text(&xml)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Find matching notes
            let notes_text = has_notes
                .iter()
                .find(|(n, _)| n == slide_num)
                .and_then(|(_, path)| archive.by_name(path).ok())
                .and_then(|mut file| {
                    let mut xml = String::new();
                    file.read_to_string(&mut xml).ok()?;
                    Some(extract_notes_text(&xml))
                })
                .unwrap_or_default();

            let content = format!("{}\n{}", slide_text, notes_text).trim().to_string();
            if !content.is_empty() {
                markdown.push_str(&format!("## Slide {}\n\n", slide_num));

                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        markdown.push('\n');
                    } else if trimmed.starts_with('\u{2022}')
                        || trimmed.starts_with('-')
                        || trimmed.starts_with('•')
                    {
                        markdown.push_str(&format!(
                            "- {}\n",
                            trimmed
                                .trim_start_matches('\u{2022}')
                                .trim_start_matches('-')
                                .trim_start_matches('•')
                                .trim()
                        ));
                    } else {
                        markdown.push_str(trimmed);
                        markdown.push('\n');
                    }
                }

                // Extract images for this slide from relationship file
                let rels_path = format!("ppt/slides/_rels/slide{}.xml.rels", slide_num);
                let mut image_paths: Vec<String> = Vec::new();
                if let Ok(mut rels_file) = archive.by_name(&rels_path) {
                    let mut rels_xml = String::new();
                    if rels_file.read_to_string(&mut rels_xml).is_ok() {
                        let rels = crate::ooxml_images::parse_rels(&rels_xml);
                        for (_, target) in &rels {
                            if target.contains("media/") || target.starts_with("image") {
                                let img_path = if target.starts_with("../") {
                                    format!("ppt/{}", target.strip_prefix("../").unwrap_or(target))
                                } else {
                                    format!("ppt/slides/{}", target)
                                };
                                image_paths.push(img_path);
                            }
                        }
                    }
                }
                for path in &image_paths {
                    if let Some(data_uri) = crate::ooxml_images::read_image_as_data_uri(&mut archive, path) {
                        markdown.push_str(&format!("![图片]({})\n\n", data_uri));
                    }
                }

                markdown.push_str("\n\n");
            }
        }

        let filename = input.filename.as_deref().unwrap_or("presentation.pptx");
        let suggested = format!("{}.md", filename.trim_end_matches(".pptx"));

        Ok(ExtractResult {
            text: markdown.trim().to_string(),
            suggested_filename: suggested,
            metadata,
            original_content: original,
        })
    }
}
