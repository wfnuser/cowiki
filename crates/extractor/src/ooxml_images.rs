//! Shared image extraction from OOXML files (DOCX, PPTX, XLSX).
//! OOXML files are ZIP archives. Images live in word/media/ (DOCX) or ppt/media/ (PPTX).
//! Relationship files (.rels) map IDs to image file paths.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use base64::Engine;

use crate::ExtractError;

/// Parse a .rels XML file to extract (Id → Target) mappings.
pub fn parse_rels(xml: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    // Simple regex-free XML attribute extraction for Relationship elements
    let mut pos = 0;
    while let Some(start) = xml[pos..].find("<Relationship ") {
        let abs_start = pos + start;
        let end = xml[abs_start..].find("/>").map(|e| abs_start + e + 2)
            .or_else(|| xml[abs_start..].find('>').map(|e| abs_start + e + 1));

        if let Some(end) = end {
            let elem = &xml[abs_start..end];
            let id = extract_attr(elem, "Id");
            let target = extract_attr(elem, "Target");
            if let (Some(id), Some(target)) = (id, target) {
                map.insert(id, target);
            }
            pos = end;
        } else {
            break;
        }
    }
    map
}

fn extract_attr(elem: &str, name: &str) -> Option<String> {
    let needle = format!("{}=\"", name);
    let start = elem.find(&needle)? + needle.len();
    let end = elem[start..].find('"')?;
    Some(elem[start..start + end].to_string())
}

/// Read an image from a ZIP archive by its path, return as base64 data URI.
pub fn read_image_as_data_uri<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    path: &str,
) -> Option<String> {
    let mut file = archive.by_name(path).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;

    let mime = mime_type(&buf, path);
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Some(format!("data:{};base64,{}", mime, b64))
}

/// Detect MIME type from magic bytes or file extension.
fn mime_type(data: &[u8], filename: &str) -> &'static str {
    if data.len() >= 8 {
        if &data[..8] == b"\x89PNG\r\n\x1a\n" {
            return "image/png";
        }
        if &data[..3] == b"\xff\xd8\xff" {
            return "image/jpeg";
        }
        if &data[..4] == b"GIF8" {
            return "image/gif";
        }
        if &data[..4] == b"RIFF" && data.len() >= 12 && &data[8..12] == b"WEBP" {
            return "image/webp";
        }
        if &data[..2] == b"BM" {
            return "image/bmp";
        }
    }
    // Fallback to extension
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    }
}

/// Extract images from an OOXML slide/part relationship file and embed them in the given text.
/// `rels_dir` is the directory containing .rels files (e.g., "ppt/slides/_rels").
/// `media_base` is the base path for media files (e.g., "ppt/media").
pub fn embed_images_in_text<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    text: &str,
) -> String {
    // Find all image references in markdown: ![...](path) that point to media files
    let mut result = String::new();
    let bytes = text.as_bytes();
    let mut pos = 0usize;

    while pos < bytes.len() {
        // Look for ![...](...) pattern
        if bytes[pos] == b'!' && pos + 1 < bytes.len() && bytes[pos + 1] == b'[' {
            // Find the closing bracket and opening paren
            if let Some(bracket_end) = bytes[pos..].iter().position(|&b| b == b']') {
                let abs_bracket_end = pos + bracket_end;
                if abs_bracket_end + 1 < bytes.len() && bytes[abs_bracket_end + 1] == b'(' {
                    if let Some(paren_end) = bytes[abs_bracket_end + 2..].iter().position(|&b| b == b')') {
                        let abs_paren_end = abs_bracket_end + 2 + paren_end;
                        let img_path = String::from_utf8_lossy(
                            &bytes[abs_bracket_end + 2..abs_paren_end]
                        ).to_string();

                        // Try to read the image from the ZIP
                        if let Some(data_uri) = read_image_as_data_uri(archive, &img_path) {
                            result.push_str(&format!(
                                "{}[{}]({})",
                                String::from_utf8_lossy(&bytes[pos..pos + 1]),
                                String::from_utf8_lossy(&bytes[pos + 2..abs_bracket_end]),
                                data_uri
                            ));
                            pos = abs_paren_end + 1;
                            continue;
                        }
                    }
                }
            }
        }
        result.push(bytes[pos] as char);
        pos += 1;
    }

    result
}
