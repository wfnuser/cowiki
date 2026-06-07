use std::io::Cursor;

use async_trait::async_trait;
use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};
use crate::decode_base64;

pub struct XlsxExtractor;

/// Format a cell value as a string.
fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 {
                format!("{}", *f as i64)
            } else {
                format!("{}", f)
            }
        }
        Data::Int(i) => format!("{}", i),
        Data::Bool(b) => format!("{}", b),
        Data::DateTime(d) => d.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("ERROR:{}", e),
    }
}

#[async_trait]
impl SourceExtractor for XlsxExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::Xlsx]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::NoAuth
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let bytes = if input.encoding.as_deref() == Some("base64") {
            decode_base64(&input.content)?
        } else {
            return Err(ExtractError::InvalidInput(
                "XLSX requires base64 encoding".into(),
            ));
        };

        let original = bytes.clone();
        let cursor = Cursor::new(bytes);

        let mut workbook: Xlsx<_> = open_workbook_from_rs(cursor)
            .map_err(|e| ExtractError::ExtractionFailed {
                source_type: SourceType::Xlsx,
                message: format!("failed to open XLSX: {}", e),
            })?;

        let sheet_names = workbook.sheet_names().to_vec();
        let mut markdown = String::new();
        let mut metadata = ExtractMetadata::default();
        metadata.extra.insert(
            "sheets".to_string(),
            sheet_names.join(", "),
        );

        for name in &sheet_names {
            if let Ok(range) = workbook.worksheet_range(name) {
                let mut rows: Vec<Vec<Data>> = Vec::new();
                for row_result in range.rows() {
                    rows.push(row_result.to_vec());
                }

                if rows.is_empty() {
                    continue;
                }

                let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                if max_cols == 0 {
                    continue;
                }

                markdown.push_str(&format!("\n## {}\n\n", name));

                for (row_idx, row) in rows.iter().enumerate() {
                    markdown.push_str("| ");
                    for col_idx in 0..max_cols {
                        let cell_str = row
                            .get(col_idx)
                            .map(|v| cell_to_string(v))
                            .unwrap_or_default();
                        let escaped = cell_str.replace('|', "\\|").replace('\n', " ");
                        markdown.push_str(&escaped);
                        markdown.push_str(" |");
                    }
                    markdown.push('\n');

                    if row_idx == 0 {
                        markdown.push('|');
                        for _ in 0..max_cols {
                            markdown.push_str(" --- |");
                        }
                        markdown.push('\n');
                    }
                }
                markdown.push('\n');
            }
        }

        let filename = input.filename.as_deref().unwrap_or("spreadsheet.xlsx");
        let suggested = format!("{}.md", filename.trim_end_matches(".xlsx"));

        Ok(ExtractResult {
            text: markdown.trim().to_string(),
            suggested_filename: suggested,
            metadata,
            original_content: original,
        })
    }
}
