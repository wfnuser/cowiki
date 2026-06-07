use std::io::{Cursor, Read};

use async_trait::async_trait;
use docx_rs::{DocumentChild, ParagraphChild, RunChild, TableCellContent, TableRowChild};

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};
use crate::decode_base64;

pub struct DocxExtractor;

/// Detect heading level from paragraph style name.
fn heading_level(para: &docx_rs::Paragraph) -> Option<usize> {
    let style = para.property.style.as_ref()?;
    let name = style.val.to_lowercase();
    if name.starts_with("heading") || name.starts_with("title") {
        name.chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .ok()
            .or(Some(1))
    } else {
        None
    }
}

/// Extract plain text from paragraph children.
fn collect_para_text(children: &[ParagraphChild]) -> String {
    children
        .iter()
        .filter_map(|child| match child {
            ParagraphChild::Run(run) => Some(
                run.children.iter().filter_map(|rc| match rc {
                    RunChild::Text(t) => Some(t.text.clone()),
                    RunChild::Tab(_) => Some("\t".to_string()),
                    RunChild::Break(_) => Some("\n".to_string()),
                    _ => None,
                }).collect::<Vec<_>>().join("")
            ),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn is_empty_para(para: &docx_rs::Paragraph) -> bool {
    collect_para_text(&para.children).trim().is_empty()
}

/// Extract text from a table cell's content.
fn cell_text(content: &[TableCellContent]) -> String {
    content.iter().filter_map(|c| match c {
        TableCellContent::Paragraph(p) => Some(collect_para_text(&p.children)),
        _ => None,
    }).collect::<Vec<_>>().join(" ")
}

#[async_trait]
impl SourceExtractor for DocxExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Docx]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let bytes = if input.encoding.as_deref() == Some("base64") {
            decode_base64(&input.content)?
        } else {
            return Err(ExtractError::InvalidInput("DOCX requires base64 encoding".into()));
        };

        let original = bytes.clone();
        let docx = docx_rs::read_docx(&bytes)
            .map_err(|e| ExtractError::ExtractionFailed {
                source_type: SourceType::Docx,
                message: format!("failed to parse DOCX: {}", e),
            })?;

        let mut markdown = String::new();
        let mut metadata = ExtractMetadata::default();

        for child in &docx.document.children {
            match child {
                DocumentChild::Paragraph(para) => {
                    if is_empty_para(para) {
                        markdown.push('\n');
                        continue;
                    }
                    let text = collect_para_text(&para.children);
                    if let Some(level) = heading_level(para) {
                        let prefix = "#".repeat(level.min(6));
                        markdown.push_str(&format!("\n{} {}\n\n", prefix, text.trim()));
                        if metadata.title.is_none() {
                            metadata.title = Some(text.trim().to_string());
                        }
                    } else {
                        markdown.push_str(&text);
                        markdown.push_str("\n\n");
                    }
                }
                DocumentChild::Table(table) => {
                    markdown.push('\n');
                    for (row_idx, table_child) in table.rows.iter().enumerate() {
                        if let docx_rs::TableChild::TableRow(row) = table_child {
                            markdown.push_str("| ");
                            for cell in &row.cells {
                                if let TableRowChild::TableCell(tc) = cell {
                                    let ct = cell_text(&tc.children);
                                    markdown.push_str(&ct.replace('\n', " ").trim());
                                }
                                markdown.push_str(" |");
                            }
                            markdown.push('\n');
                            if row_idx == 0 {
                                markdown.push('|');
                                for _ in &row.cells {
                                    markdown.push_str(" --- |");
                                }
                                markdown.push('\n');
                            }
                        }
                    }
                    markdown.push('\n');
                }
                _ => {}
            }
        }

        let filename = input.filename.as_deref().unwrap_or("document.docx");
        let suggested = format!("{}.md", filename.trim_end_matches(".docx"));

        let mut cleaned = String::new();
        let mut blank: usize = 0;
        for line in markdown.lines() {
            if line.trim().is_empty() {
                blank += 1;
                if blank <= 2 { cleaned.push('\n'); }
            } else {
                blank = 0;
                cleaned.push_str(line);
                cleaned.push('\n');
            }
        }

        // Extract embedded images from the DOCX ZIP
        if let Ok(mut zip_archive) = zip::ZipArchive::new(Cursor::new(original.clone())) {
            // Collect image paths first (to avoid double borrow)
            let mut image_paths: Vec<String> = Vec::new();
            if let Ok(mut rels_file) = zip_archive.by_name("word/_rels/document.xml.rels") {
                let mut rels_xml = String::new();
                if rels_file.read_to_string(&mut rels_xml).is_ok() {
                    let rels = crate::ooxml_images::parse_rels(&rels_xml);
                    for (_, target) in &rels {
                        if target.contains("media/") {
                            image_paths.push(format!("word/{}", target));
                        }
                    }
                }
            }
            // Now read images (rels_file has been dropped)
            let mut image_count = 0usize;
            for path in &image_paths {
                if let Some(data_uri) = crate::ooxml_images::read_image_as_data_uri(&mut zip_archive, path) {
                    image_count += 1;
                    cleaned.push_str(&format!("![图片{}]({})\n\n", image_count, data_uri));
                }
            }
        }

        Ok(ExtractResult {
            text: cleaned.trim().to_string(),
            suggested_filename: suggested,
            metadata,
            original_content: original,
        })
    }
}
