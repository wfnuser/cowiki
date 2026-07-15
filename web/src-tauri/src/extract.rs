//! Plain-text extraction for non-Markdown source files ingested into a Space.
//!
//! Each supported format gets a best-effort conversion to readable text so
//! it can be stored as a normal OKF Source document alongside pasted text
//! and URLs. A format we can't parse reliably is rejected with a clear
//! error rather than silently degraded — a source that failed to extract
//! should never enter the wiki looking like it succeeded.

use std::io::Read;
use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader as _};
use quick_xml::events::Event;
use quick_xml::reader::Reader as XmlReader;

/// Binary formats this module knows how to convert to text.
pub const BINARY_EXTENSIONS: &[&str] = &["pdf", "docx", "xlsx", "xls", "ods", "pptx", "odt", "odp"];

/// Formats that are already text and only need reading, not parsing.
pub const PLAIN_TEXT_EXTENSIONS: &[&str] = &[
    "md", "mdx", "txt", "csv", "tsv", "json", "html", "htm", "xml", "yaml", "yml",
];

/// Every extension this module can turn into ingestible text, binary or
/// plain. The file picker and the dispatcher below both read from this so
/// they can never drift.
pub fn all_supported_extensions() -> Vec<&'static str> {
    BINARY_EXTENSIONS
        .iter()
        .chain(PLAIN_TEXT_EXTENSIONS)
        .copied()
        .collect()
}

pub fn is_supported(path: &Path) -> bool {
    extension_of(path).is_some_and(|extension| {
        BINARY_EXTENSIONS.contains(&extension.as_str())
            || PLAIN_TEXT_EXTENSIONS.contains(&extension.as_str())
    })
}

/// Read a source file into text, dispatching on extension: plain-text
/// formats are read directly, binary formats go through their parser.
pub fn read_source_file(path: &Path) -> Result<String, String> {
    let extension = extension_of(path)
        .ok_or_else(|| "file has no extension to identify its format".to_string())?;
    let text = if PLAIN_TEXT_EXTENSIONS.contains(&extension.as_str()) {
        std::fs::read_to_string(path).map_err(|error| format!("cannot read file: {error}"))?
    } else {
        match extension.as_str() {
            "pdf" => extract_pdf(path)?,
            "docx" => extract_docx(path)?,
            "xlsx" | "xls" | "ods" => extract_spreadsheet(path)?,
            "pptx" => extract_slides(path)?,
            "odt" | "odp" => extract_zip_xml_part(path, "content.xml")?,
            other => {
                return Err(format!(
                    "'.{other}' is not a supported source format (supported: {})",
                    all_supported_extensions().join(", ")
                ))
            }
        }
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("no extractable text found in this file".to_string());
    }
    Ok(trimmed.to_string())
}

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}

fn extract_pdf(path: &Path) -> Result<String, String> {
    pdf_extract::extract_text(path)
        .map_err(|error| format!("cannot extract text from PDF: {error}"))
}

fn extract_docx(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let docx =
        docx_rs::read_docx(&bytes).map_err(|error| format!("cannot parse DOCX: {error:?}"))?;

    let mut lines = Vec::new();
    for child in docx.document.children {
        match child {
            docx_rs::DocumentChild::Paragraph(paragraph) => {
                if let Some(line) = docx_paragraph_text(&paragraph) {
                    lines.push(line);
                }
            }
            docx_rs::DocumentChild::Table(table) => lines.push(docx_table_text(&table)),
            _ => {}
        }
    }
    Ok(lines.join("\n\n"))
}

/// A paragraph styled "HeadingN" becomes a Markdown heading; everything
/// else is a plain line. Run-level bold/italic markers are deliberately
/// skipped — they add fragility (a run boundary mid-word turns into
/// mangled `**` pairs) for little value in text meant for an LLM to read
/// and reorganize, not for byte-faithful document reproduction.
fn docx_paragraph_text(paragraph: &docx_rs::Paragraph) -> Option<String> {
    let text: String = paragraph
        .children
        .iter()
        .filter_map(|child| match child {
            docx_rs::ParagraphChild::Run(run) => Some(docx_run_text(run)),
            _ => None,
        })
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let heading_level = paragraph
        .property
        .style
        .as_ref()
        .and_then(|style| style.val.strip_prefix("Heading"))
        .and_then(|level| level.trim().parse::<usize>().ok())
        .map(|level| level.clamp(1, 6));

    Some(match heading_level {
        Some(level) => format!("{} {text}", "#".repeat(level)),
        None if paragraph.property.numbering_property.is_some() => format!("- {text}"),
        None => text.to_string(),
    })
}

fn docx_run_text(run: &docx_rs::Run) -> String {
    run.children
        .iter()
        .filter_map(|child| match child {
            docx_rs::RunChild::Text(text) => Some(text.text.as_str()),
            docx_rs::RunChild::Tab(_) => Some("\t"),
            _ => None,
        })
        .collect()
}

fn docx_table_text(table: &docx_rs::Table) -> String {
    table
        .rows
        .iter()
        .map(|row| {
            let docx_rs::TableChild::TableRow(row) = row;
            row.cells
                .iter()
                .map(|cell| {
                    let docx_rs::TableRowChild::TableCell(cell) = cell;
                    cell.children
                        .iter()
                        .filter_map(|content| match content {
                            docx_rs::TableCellContent::Paragraph(paragraph) => {
                                docx_paragraph_text(paragraph)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders every sheet as a Markdown table under its own heading.
fn extract_spreadsheet(path: &Path) -> Result<String, String> {
    let mut workbook =
        open_workbook_auto(path).map_err(|error| format!("cannot open spreadsheet: {error}"))?;
    let mut sections = Vec::new();
    for (name, range) in workbook.worksheets() {
        if range.is_empty() {
            continue;
        }
        let mut rows = range.rows().map(spreadsheet_row_to_markdown);
        let Some(header) = rows.next() else {
            continue;
        };
        let column_count = range.get_size().1.max(1);
        let separator = format!("|{}", " --- |".repeat(column_count));
        let mut table = vec![header, separator];
        table.extend(rows);
        sections.push(format!("## {name}\n\n{}", table.join("\n")));
    }
    Ok(sections.join("\n\n"))
}

fn spreadsheet_row_to_markdown(row: &[Data]) -> String {
    let cells = row
        .iter()
        .map(|cell| match cell {
            Data::Empty => String::new(),
            other => other.to_string().replace('|', "\\|"),
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!("| {cells} |")
}

/// PPTX keeps each slide as its own `ppt/slides/slideN.xml` part. Walk the
/// archive for that pattern (there's no index of slide count elsewhere in
/// the package) and read them back in slide order.
fn extract_slides(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("cannot open PPTX archive: {error}"))?;

    let mut slide_numbers = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("cannot read PPTX archive entry: {error}"))?;
        if let Some(number) = slide_number(entry.name()) {
            slide_numbers.push(number);
        }
    }
    slide_numbers.sort_unstable();

    let mut slides = Vec::new();
    for number in slide_numbers {
        let name = format!("ppt/slides/slide{number}.xml");
        let mut entry = archive
            .by_name(&name)
            .map_err(|error| format!("cannot read {name}: {error}"))?;
        let mut xml = String::new();
        entry
            .read_to_string(&mut xml)
            .map_err(|error| format!("cannot read {name}: {error}"))?;
        let text = xml_visible_text(&xml)?;
        if !text.is_empty() {
            slides.push(format!("## Slide {number}\n\n{text}"));
        }
    }
    Ok(slides.join("\n\n"))
}

fn slide_number(entry_name: &str) -> Option<u32> {
    entry_name
        .strip_prefix("ppt/slides/slide")?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

fn extract_zip_xml_part(path: &Path, part_name: &str) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("cannot open archive: {error}"))?;
    let mut entry = archive
        .by_name(part_name)
        .map_err(|error| format!("cannot read {part_name}: {error}"))?;
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|error| format!("cannot read {part_name}: {error}"))?;
    xml_visible_text(&xml)
}

/// Walks an XML document's text nodes in order, inserting a line break
/// after every paragraph-like element (`<a:p>` in OOXML, `<text:p>` /
/// `<text:h>` in ODF — matched by local name so both formats share one
/// code path). Good enough for feeding an LLM the reading-order content;
/// it does not attempt to preserve every structural nuance.
fn xml_visible_text(xml: &str) -> Result<String, String> {
    let mut reader = XmlReader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = String::new();
    let mut buffer = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| format!("malformed XML: {error}"))?
        {
            Event::Text(text) => {
                let decoded = text
                    .decode()
                    .map_err(|error| format!("malformed XML text: {error}"))?;
                push_xml_text(&mut out, &decoded);
            }
            Event::CData(text) => {
                let decoded = text
                    .decode()
                    .map_err(|error| format!("malformed XML text: {error}"))?;
                push_xml_text(&mut out, &decoded);
            }
            Event::End(end) if matches!(end.local_name().as_ref(), b"p" | b"h") => {
                out.push('\n');
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n"))
}

fn push_xml_text(out: &mut String, decoded: &str) {
    if decoded.trim().is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with(['\n', ' ']) {
        out.push(' ');
    }
    out.push_str(decoded.trim());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_extensions_with_a_clear_message() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("legacy.doc");
        std::fs::write(&path, b"not really a doc").unwrap();
        let error = read_source_file(&path).unwrap_err();
        assert!(error.contains("not a supported source format"));
        assert!(!is_supported(&path));
    }

    #[test]
    fn plain_text_formats_are_read_directly() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.txt");
        std::fs::write(&path, "Plain notes.").unwrap();
        assert!(is_supported(&path));
        assert_eq!(read_source_file(&path).unwrap(), "Plain notes.");
    }

    #[test]
    fn xlsx_becomes_a_markdown_table_per_sheet() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("book.xlsx");
        write_minimal_xlsx(&path, "Sheet1", &[["Name", "Score"], ["Ada", "10"]]);
        let text = read_source_file(&path).unwrap();
        assert!(text.contains("## Sheet1"));
        assert!(text.contains("| Name | Score |"));
        assert!(text.contains("| Ada | 10 |"));
    }

    #[test]
    fn pptx_extracts_slide_text_in_order() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("deck.pptx");
        write_minimal_pptx(&path, &["First slide title", "Second slide title"]);
        let text = read_source_file(&path).unwrap();
        let first = text.find("First slide title").unwrap();
        let second = text.find("Second slide title").unwrap();
        assert!(first < second);
        assert!(text.contains("## Slide 1"));
        assert!(text.contains("## Slide 2"));
    }

    #[test]
    fn odt_extracts_paragraph_text() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("note.odt");
        write_minimal_odf_zip(
            &path,
            "<office:document-content xmlns:office=\"urn:office\" xmlns:text=\"urn:text\">\
             <office:body><office:text>\
             <text:p>Hello from an ODF document.</text:p>\
             </office:text></office:body></office:document-content>",
        );
        let text = read_source_file(&path).unwrap();
        assert_eq!(text, "Hello from an ODF document.");
    }

    fn write_minimal_xlsx(path: &Path, sheet_name: &str, rows: &[[&str; 2]]) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("[Content_Types].xml", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#).unwrap();

        zip.start_file("_rels/.rels", options).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#).unwrap();

        zip.start_file("xl/_rels/workbook.xml.rels", options)
            .unwrap();
        zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();

        zip.start_file("xl/workbook.xml", options).unwrap();
        let workbook_xml = format!(
            r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="{sheet_name}" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets></workbook>"#
        );
        zip.write_all(workbook_xml.as_bytes()).unwrap();

        zip.start_file("xl/worksheets/sheet1.xml", options).unwrap();
        let mut sheet_data = String::from(
            r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        );
        for (row_index, row) in rows.iter().enumerate() {
            let row_number = row_index + 1;
            sheet_data.push_str(&format!(r#"<row r="{row_number}">"#));
            for (col_index, value) in row.iter().enumerate() {
                let column = (b'A' + col_index as u8) as char;
                sheet_data.push_str(&format!(
                    r#"<c r="{column}{row_number}" t="inlineStr"><is><t>{value}</t></is></c>"#
                ));
            }
            sheet_data.push_str("</row>");
        }
        sheet_data.push_str("</sheetData></worksheet>");
        zip.write_all(sheet_data.as_bytes()).unwrap();

        zip.finish().unwrap();
    }

    fn write_minimal_pptx(path: &Path, slide_titles: &[&str]) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (index, title) in slide_titles.iter().enumerate() {
            let slide_number = index + 1;
            zip.start_file(format!("ppt/slides/slide{slide_number}.xml"), options)
                .unwrap();
            let slide_xml = format!(
                r#"<?xml version="1.0"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
            );
            zip.write_all(slide_xml.as_bytes()).unwrap();
        }

        zip.finish().unwrap();
    }

    fn write_minimal_odf_zip(path: &Path, content_xml: &str) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("content.xml", options).unwrap();
        zip.write_all(content_xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
}
