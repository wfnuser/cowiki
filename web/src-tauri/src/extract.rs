//! Plain-text extraction for non-Markdown source files ingested into a Space.
//!
//! Each supported format gets a best-effort conversion to readable text so
//! it can be stored as a normal OKF Source document alongside pasted text
//! and URLs. A format we can't parse reliably is rejected with a clear
//! error rather than silently degraded — a source that failed to extract
//! should never enter the wiki looking like it succeeded.

mod local_tools;
mod quality;
pub use quality::{ExtractionReport, ExtractionResult, QualityStatus};

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader as _};
use quick_xml::events::Event;
use quick_xml::reader::Reader as XmlReader;

const MAX_SOURCE_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_EXTRACTED_TEXT_BYTES: usize = 32 * 1024 * 1024;

fn validate_file_size(path: &Path, max_bytes: u64) -> Result<(), String> {
    let bytes = std::fs::metadata(path)
        .map_err(|error| format!("cannot inspect source file: {error}"))?
        .len();
    if bytes > max_bytes {
        return Err(format!(
            "source file is too large ({bytes} bytes; maximum is {max_bytes})"
        ));
    }
    Ok(())
}

fn validate_archive_limits(
    path: &Path,
    max_entry_bytes: u64,
    max_total_bytes: u64,
    max_entries: usize,
) -> Result<(), String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|error| format!("cannot open archive: {error}"))?;
    if archive.len() > max_entries {
        return Err(format!(
            "archive has too many entries ({}; maximum is {max_entries})",
            archive.len()
        ));
    }

    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("cannot inspect archive entry: {error}"))?;
        let entry_bytes = entry.size();
        if entry_bytes > max_entry_bytes {
            return Err(format!(
                "archive entry '{}' is too large ({entry_bytes} bytes; maximum is {max_entry_bytes})",
                entry.name()
            ));
        }
        total_bytes = total_bytes
            .checked_add(entry_bytes)
            .ok_or_else(|| "archive expanded size overflowed".to_string())?;
        if total_bytes > max_total_bytes {
            return Err(format!(
                "archive expands to too much data ({total_bytes} bytes; maximum is {max_total_bytes})"
            ));
        }
    }
    Ok(())
}

fn validate_extracted_text(text: &str, max_bytes: usize) -> Result<(), String> {
    if text.len() > max_bytes {
        return Err(format!(
            "extracted text is too large ({} bytes; maximum is {max_bytes})",
            text.len()
        ));
    }
    Ok(())
}

/// Binary formats this module knows how to convert to text.
pub const BINARY_EXTENSIONS: &[&str] = &[
    "pdf", "docx", "xlsx", "xls", "ods", "pptx", "odt", "odp", "doc", "png", "jpg", "jpeg", "tif",
    "tiff", "bmp", "webp",
];

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

/// Extract one file with a portable quality report. Failed candidates never carry
/// Markdown that a caller could accidentally persist as a successful Source.
pub fn read_source_file(path: &Path, allow_local_tools: bool) -> ExtractionResult {
    let format = extension_of(path).unwrap_or_default();
    let snapshot = (|| -> Result<_, String> {
        if !is_supported(path) {
            return Err(format!("'.{format}' is not a supported source format"));
        }
        validate_file_size(path, MAX_SOURCE_FILE_BYTES)?;
        if !std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("Source must be a regular file.".to_string());
        }
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let filename = directory.path().join(format!("source.{format}"));
        let mut source = std::fs::File::open(path)
            .map_err(|error| error.to_string())?
            .take(MAX_SOURCE_FILE_BYTES + 1);
        let mut destination =
            std::fs::File::create(&filename).map_err(|error| error.to_string())?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        let mut total = 0;
        loop {
            let bytes = source
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if bytes == 0 {
                break;
            }
            total += bytes as u64;
            if total > MAX_SOURCE_FILE_BYTES {
                return Err("Source file grew beyond the size limit.".to_string());
            }
            destination
                .write_all(&buffer[..bytes])
                .map_err(|error| error.to_string())?;
            digest.update(&buffer[..bytes]);
        }
        Ok((directory, filename, format!("{:x}", digest.finalize())))
    })();
    match snapshot {
        Ok((_directory, path, content_hash)) => {
            let mut result = extract_snapshot(&path, allow_local_tools);
            result.content_hash = content_hash;
            result
        }
        Err(error) => {
            let mut report = ExtractionReport::new(&format);
            report.status = QualityStatus::Fail;
            report.diagnostics.push(error);
            ExtractionResult {
                content_hash: String::new(),
                markdown: String::new(),
                report,
            }
        }
    }
}

fn extract_snapshot(path: &Path, allow_local_tools: bool) -> ExtractionResult {
    let format = extension_of(path).unwrap_or_default();
    let mut report = ExtractionReport::new(&format);
    let native = extract_native(path, &format, &mut report);
    let validation = native.as_ref().map_err(Clone::clone).and_then(|text| {
        validate_extracted_text(text, MAX_EXTRACTED_TEXT_BYTES)?;
        quality::assess_text(text, &mut report.clone())
    });
    let needs_fallback = validation.is_err()
        || native
            .as_ref()
            .is_ok_and(|text| text.chars().any(quality::suspicious_character));
    let mut candidate = native;
    // Resource-limit failures must never trigger a converter on the rejected input.
    let can_convert = validate_file_size(path, MAX_SOURCE_FILE_BYTES).is_ok()
        && matches!(
            format.as_str(),
            "pdf" | "doc" | "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp"
        );
    if allow_local_tools && needs_fallback && can_convert {
        match local_tools::convert(path, &format, &mut report.attempts) {
            Ok(local_tools::Conversion {
                text,
                extractor,
                pages,
            }) => {
                let mut check = ExtractionReport::new(&format);
                if quality::assess_text(&text, &mut check).is_ok() {
                    if let Err(error) = &validation {
                        report.warn(format!("Native extraction: {error}"));
                    }
                    report.warn("The native extraction was unavailable or unreliable; a local converter was used. Compare the result with the original.");
                    report.fallback(extractor);
                    if let Some(pages) = pages {
                        report.coverage(pages, pages, "PDF pages");
                    }
                    candidate = Ok(text);
                } else {
                    report.warn("The local converter also returned unreadable text.");
                }
            }
            Err(error) => report.warn(error),
        }
    }
    let result = candidate.and_then(|text| {
        validate_extracted_text(&text, MAX_EXTRACTED_TEXT_BYTES)?;
        quality::assess_text(&text, &mut report)?;
        let clean: String = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .chars()
            .map(|ch| {
                if ch.is_control() && !matches!(ch, '\n' | '\t') {
                    '\u{fffd}'
                } else {
                    ch
                }
            })
            .collect();
        validate_extracted_text(&clean, MAX_EXTRACTED_TEXT_BYTES)?;
        let markdown = clean.trim().to_string();
        report.characters = markdown.chars().count();
        Ok(markdown)
    });
    match result {
        Ok(markdown) => ExtractionResult {
            content_hash: String::new(),
            markdown,
            report,
        },
        Err(error) => {
            report.status = QualityStatus::Fail;
            report.diagnostics.push(error);
            ExtractionResult {
                content_hash: String::new(),
                markdown: String::new(),
                report,
            }
        }
    }
}

fn extract_native(
    path: &Path,
    extension: &str,
    report: &mut ExtractionReport,
) -> Result<String, String> {
    if !is_supported(path) {
        return Err(format!(
            "'.{extension}' is not a supported source format (supported: {})",
            all_supported_extensions().join(", ")
        ));
    }
    validate_file_size(path, MAX_SOURCE_FILE_BYTES)?;
    if matches!(extension, "docx" | "xlsx" | "ods" | "pptx" | "odt" | "odp") {
        validate_archive_limits(
            path,
            MAX_ARCHIVE_ENTRY_BYTES,
            MAX_ARCHIVE_TOTAL_BYTES,
            MAX_ARCHIVE_ENTRIES,
        )?;
    }
    if PLAIN_TEXT_EXTENSIONS.contains(&extension) {
        let text =
            std::fs::read_to_string(path).map_err(|error| format!("cannot read file: {error}"))?;
        if matches!(
            extension,
            "json" | "yaml" | "yml" | "xml" | "csv" | "tsv" | "html" | "htm"
        ) {
            // Keep structured input readable without interpreting its tags as Markdown/HTML.
            quality::assess_text(&text, report)?;
            let fence = "`".repeat(
                text.split(|ch| ch != '`')
                    .map(str::len)
                    .max()
                    .unwrap_or(0)
                    .max(2)
                    + 1,
            );
            if matches!(extension, "html" | "htm") {
                report.warn("This is a local HTML source listing. Use URL capture for readable web-page extraction.");
            }
            let language = if matches!(extension, "html" | "htm") {
                "text"
            } else {
                extension
            };
            return Ok(format!("{fence}{language}\n{}\n{fence}", text.trim()));
        }
        return Ok(text);
    }
    match extension {
        "pdf" => {
            report.warn("PDF layout and reading order are not verified. Check columns, tables and image-only pages against the original.");
            extract_pdf(path, report)
        }
        "docx" => {
            let xml = extract_zip_xml_part(path, "word/document.xml")?;
            let parsed = extract_docx(path);
            let expected = xml.chars().filter(|ch| ch.is_alphanumeric()).count();
            let actual = parsed.as_ref().map(|text| text.chars().filter(|ch| ch.is_alphanumeric()).count()).unwrap_or(0);
            if parsed.is_err() || actual * 100 < expected * 95 {
                report.attempts.push("cowiki-docx-xml".to_string());
                report.warn("The DOCX parser omitted document text. The XML fallback preserves text, but flattens document formatting.");
                report.fallback("cowiki-docx-xml");
                return Ok(xml);
            }
            parsed
        }
        "xlsx" | "xls" | "ods" => extract_spreadsheet(path, report),
        "pptx" => extract_slides(path, report),
        "odt" | "odp" => {
            report.warn("ODF text is extracted in XML order; layout, embedded images and complex tables need review.");
            extract_zip_xml_part(path, "content.xml")
        }
        "doc" | "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp" => {
            Err("This format needs local extraction tools. Enable them and install Tesseract for images or antiword for legacy DOC files.".to_string())
        }
        _ => Err(format!("'.{extension}' is not a supported source format (supported: {})", all_supported_extensions().join(", "))),
    }
}

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
}

fn extract_pdf(path: &Path, report: &mut ExtractionReport) -> Result<String, String> {
    let pages = pdf_extract::extract_text_by_pages(path)
        .map_err(|error| format!("cannot extract text from PDF: {error}"))?;
    let extracted = pages
        .iter()
        .filter(|text| quality::assess_text(text, &mut ExtractionReport::new("pdf")).is_ok())
        .count();
    report.coverage(pages.len(), extracted, "PDF pages");
    if pages.is_empty() || extracted != pages.len() {
        return Err(format!("Only {extracted} of {} PDF pages contain readable text. Blank or scanned pages require local OCR or a text export; no partial PDF was imported.", pages.len()));
    }
    Ok(pages.join("\n\n"))
}

#[cfg(test)]
mod mixed_pdf_tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("Mixed 中文 source.pdf");
        std::fs::write(
            &path,
            include_bytes!("../tests/fixtures/mixed-text-scan.pdf"),
        )
        .unwrap();
        (root, path)
    }

    #[test]
    fn mixed_pdf_never_succeeds_with_only_its_text_page() {
        let (_root, path) = fixture();
        let pages = pdf_extract::extract_text_by_pages(&path).unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages[0].contains("DIGITAL PAGE EVIDENCE"));
        assert!(pages[1].trim().is_empty());
        let result = read_source_file(&path, false);
        assert_eq!(result.report.status, QualityStatus::Fail);
        assert_eq!(result.report.expected_units, Some(2));
        assert_eq!(result.report.extracted_units, Some(1));
        assert!(result.markdown.is_empty());
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|message| message.contains("no partial PDF")));
    }

    #[test]
    #[ignore = "requires installed Poppler and Tesseract; run explicitly for converter validation"]
    fn mixed_pdf_recovers_scanned_page_with_local_tools() {
        let (_root, path) = fixture();
        let result = read_source_file(&path, true);
        assert_eq!(
            result.report.status,
            QualityStatus::Fallback,
            "{:?}",
            result.report
        );
        assert_eq!(result.report.expected_units, Some(2));
        assert_eq!(result.report.extracted_units, Some(2));
        assert!(result.markdown.contains("DIGITAL PAGE EVIDENCE"));
        assert!(
            result.markdown.contains("SCANNED PAGE EVIDENCE"),
            "{}",
            result.markdown
        );
        assert_eq!(
            result
                .report
                .attempts
                .iter()
                .filter(|tool| *tool == "tesseract")
                .count(),
            1
        );
    }
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
fn extract_spreadsheet(path: &Path, report: &mut ExtractionReport) -> Result<String, String> {
    let mut workbook =
        open_workbook_auto(path).map_err(|error| format!("cannot open spreadsheet: {error}"))?;
    let mut sections = Vec::new();
    let names = workbook.sheet_names().to_vec();
    let mut nonempty = 0;
    for name in &names {
        let range = match workbook.worksheet_range(name) {
            Ok(range) => range,
            Err(error) => {
                report.warn(format!("Cannot read sheet '{name}': {error}"));
                continue;
            }
        };
        if range.is_empty()
            || range.rows().flatten().all(|cell| {
                matches!(cell, Data::Empty)
                    || matches!(cell, Data::String(value) if value.trim().is_empty())
            })
        {
            continue;
        }
        nonempty += 1;
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
    report.coverage(names.len(), nonempty, "sheets");
    Ok(sections.join("\n\n"))
}

fn spreadsheet_row_to_markdown(row: &[Data]) -> String {
    let cells = row
        .iter()
        .map(|cell| match cell {
            Data::Empty => String::new(),
            other => other
                .to_string()
                .replace("\r\n", "<br>")
                .replace(['\r', '\n'], "<br>")
                .replace('|', "\\|"),
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!("| {cells} |")
}

/// PPTX keeps each slide as its own `ppt/slides/slideN.xml` part. Walk the
/// archive for that pattern (there's no index of slide count elsewhere in
/// the package) and read them back in slide order.
fn extract_slides(path: &Path, report: &mut ExtractionReport) -> Result<String, String> {
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

    let expected = slide_numbers.len();
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
    report.coverage(expected, slides.len(), "slides");
    report
        .warn("Slide images, diagrams and speaker notes are not included in this text extraction.");
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
    reader.config_mut().trim_text(false);
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
            Event::GeneralRef(reference) => {
                let name = reference.decode().map_err(|error| error.to_string())?;
                let entity = format!("&{name};");
                let decoded =
                    quick_xml::escape::unescape(&entity).map_err(|error| error.to_string())?;
                out.push_str(&decoded);
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
    if decoded.trim().is_empty() && decoded.contains(['\n', '\r']) {
        return;
    }
    out.push_str(decoded);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_extensions_with_a_clear_message() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("unknown.xyz");
        std::fs::write(&path, b"not really a doc").unwrap();
        let result = read_source_file(&path, false);
        assert_eq!(result.report.status, QualityStatus::Fail);
        let error = result.report.diagnostics.join(" ");
        assert!(error.contains("not a supported source format"));
        assert!(!is_supported(&path));
    }

    #[test]
    fn plain_text_formats_are_read_directly() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.txt");
        std::fs::write(&path, "Plain notes.").unwrap();
        assert!(is_supported(&path));
        assert_eq!(read_source_file(&path, false).markdown, "Plain notes.");
    }

    #[test]
    fn xlsx_becomes_a_markdown_table_per_sheet() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("book.xlsx");
        write_minimal_xlsx(&path, "Sheet1", &[["Name", "Score"], ["Ada", "10"]]);
        let text = read_source_file(&path, false).markdown;
        assert!(text.contains("## Sheet1"));
        assert!(text.contains("| Name | Score |"));
        assert!(text.contains("| Ada | 10 |"));
    }

    #[test]
    fn spreadsheet_cells_stay_inside_their_markdown_row() {
        let row = spreadsheet_row_to_markdown(&[Data::String("line one\nline two | value".into())]);
        assert_eq!(row, "| line one<br>line two \\| value |");
    }

    #[test]
    fn pptx_extracts_slide_text_in_order() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("deck.pptx");
        write_minimal_pptx(&path, &["First slide title", "Second slide title"]);
        let text = read_source_file(&path, false).markdown;
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
        let text = read_source_file(&path, false).markdown;
        assert_eq!(text, "Hello from an ODF document.");
    }

    #[test]
    fn quality_reports_partial_slide_coverage_instead_of_silent_success() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("partial.pptx");
        write_minimal_pptx(&path, &["Readable slide", ""]);
        let extracted = read_source_file(&path, false);
        assert_eq!(extracted.report.status, QualityStatus::Warn);
        assert_eq!(extracted.report.expected_units, Some(2));
        assert_eq!(extracted.report.extracted_units, Some(1));
        assert!(extracted
            .report
            .diagnostics
            .iter()
            .any(|message| message.contains("1 of 2")));
    }

    #[test]
    fn quality_rejects_garbled_nonempty_text_and_keeps_fail_body_empty() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("broken.txt");
        std::fs::write(&path, "\u{fffd}\u{e000}\u{e001}text").unwrap();
        let extracted = read_source_file(&path, false);
        assert_eq!(extracted.report.status, QualityStatus::Fail);
        assert!(extracted.markdown.is_empty());
    }

    #[test]
    fn local_ocr_is_explicitly_opted_in() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("scan.png");
        std::fs::write(&path, b"invalid image").unwrap();
        let extracted = read_source_file(&path, false);
        assert_eq!(extracted.report.status, QualityStatus::Fail);
        assert_eq!(extracted.report.attempts, ["cowiki-native"]);
        assert!(extracted
            .report
            .diagnostics
            .iter()
            .any(|message| message.contains("Enable")));
    }

    #[test]
    fn docx_fallback_recovers_text_nested_in_hyperlinks() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("linked.docx");
        let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        archive
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:hyperlink><w:r><w:t>Important linked text &amp; evidence</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#).unwrap();
        archive.finish().unwrap();
        let extracted = read_source_file(&path, false);
        assert_eq!(extracted.report.status, QualityStatus::Fallback);
        assert_eq!(extracted.report.extractor, "cowiki-docx-xml");
        assert!(extracted
            .markdown
            .contains("Important linked text & evidence"));
    }

    #[test]
    fn docx_quality_detects_silent_loss_in_a_valid_document() {
        use docx_rs::{Docx, Hyperlink, HyperlinkType, Paragraph, Run};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("valid.docx");
        Docx::new()
            .add_paragraph(
                Paragraph::new()
                    .add_run(Run::new().add_text("Short intro. "))
                    .add_hyperlink(
                        Hyperlink::new("https://example.com", HyperlinkType::External).add_run(
                            Run::new().add_text(
                                "Essential evidence that the native paragraph reader omits.",
                            ),
                        ),
                    ),
            )
            .build()
            .pack(std::fs::File::create(&path).unwrap())
            .unwrap();
        let native = extract_docx(&path).unwrap();
        assert!(!native.contains("Essential evidence"));
        let extracted = read_source_file(&path, false);
        assert_eq!(extracted.report.status, QualityStatus::Fallback);
        assert!(extracted.markdown.contains("Essential evidence"));
    }

    #[test]
    fn xml_recovery_preserves_spaces_between_text_runs() {
        let text = xml_visible_text("<p><t>one</t><t> </t><t>two</t><t>&amp;</t><t>three</t></p>")
            .unwrap();
        assert_eq!(text, "one two&three");
    }

    #[test]
    fn structured_text_is_fenced_without_breaking_embedded_fences() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("data.json");
        std::fs::write(&path, r#"{"example":"```"}"#).unwrap();
        let extracted = read_source_file(&path, false);
        assert_eq!(extracted.report.status, QualityStatus::Pass);
        assert!(extracted.markdown.starts_with("````json\n"));
        assert!(extracted.markdown.ends_with("\n````"));
    }

    #[test]
    fn rejects_input_larger_than_the_configured_limit() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("large.txt");
        std::fs::write(&path, b"123456789").unwrap();

        let error = validate_file_size(&path, 8).unwrap_err();
        assert!(error.contains("too large"));
    }

    #[test]
    fn rejects_archive_entries_that_expand_past_the_configured_limit() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("large.pptx");
        write_minimal_pptx(&path, &["This slide expands beyond a tiny test limit"]);

        let error = validate_archive_limits(&path, 8, 1_024, 10).unwrap_err();
        assert!(error.contains("archive entry"));
    }

    #[test]
    fn rejects_extracted_text_larger_than_the_configured_limit() {
        let error = validate_extracted_text("123456789", 8).unwrap_err();
        assert!(error.contains("extracted text"));
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
