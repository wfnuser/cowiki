use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QualityStatus {
    Pass,
    Warn,
    Fallback,
    Fail,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionReport {
    pub status: QualityStatus,
    pub format: String,
    pub extractor: String,
    pub version: u32,
    pub characters: usize,
    pub expected_units: Option<usize>,
    pub extracted_units: Option<usize>,
    pub diagnostics: Vec<String>,
    pub attempts: Vec<String>,
}

impl ExtractionReport {
    pub fn new(format: &str) -> Self {
        Self {
            status: QualityStatus::Pass,
            format: format.to_string(),
            extractor: "cowiki-native".to_string(),
            version: 1,
            characters: 0,
            expected_units: None,
            extracted_units: None,
            diagnostics: Vec::new(),
            attempts: vec!["cowiki-native".to_string()],
        }
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        if self.status == QualityStatus::Pass {
            self.status = QualityStatus::Warn;
        }
        self.diagnostics.push(message.into());
    }

    pub fn fallback(&mut self, extractor: &str) {
        self.status = QualityStatus::Fallback;
        self.extractor = extractor.to_string();
    }

    pub fn coverage(&mut self, expected: usize, extracted: usize, unit: &str) {
        self.expected_units = Some(expected);
        self.extracted_units = Some(extracted);
        if extracted < expected {
            self.warn(format!(
                "Only {extracted} of {expected} {unit} contain extracted text. Check the original for missing content."
            ));
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub content_hash: String,
    pub markdown: String,
    pub report: ExtractionReport,
}

pub fn suspicious_character(ch: char) -> bool {
    ch == '\u{fffd}'
        || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
        || matches!(ch as u32, 0xe000..=0xf8ff | 0xf0000..=0xffffd | 0x100000..=0x10fffd)
}

pub fn assess_text(text: &str, report: &mut ExtractionReport) -> Result<(), String> {
    report.characters = text.chars().count();
    let non_space = text.chars().filter(|ch| !ch.is_whitespace()).count();
    if non_space == 0 {
        return Err("No extractable text found. Scanned PDFs and images may need local OCR; enable local extraction tools or import a text export.".to_string());
    }
    let suspicious = text.chars().filter(|ch| suspicious_character(*ch)).count();
    if suspicious * 5 >= non_space {
        return Err("The extracted text is mostly unreadable glyphs. Try local extraction tools or a text export of the original.".to_string());
    }
    if suspicious > 0 {
        report.warn(format!(
            "Found {suspicious} unreadable or private-use characters. Compare the extracted text with the original."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multilingual_text_and_short_notes_are_not_treated_as_bad_extractions() {
        for text in ["你好，世界。", "مرحبا بالعالم", "é", "42", "🦀"] {
            let mut report = ExtractionReport::new("txt");
            assert!(assess_text(text, &mut report).is_ok());
            assert_eq!(report.status, QualityStatus::Pass);
        }
    }

    #[test]
    fn nonempty_garbled_text_is_rejected_and_partial_damage_is_visible() {
        assert!(assess_text("\u{fffd}\u{e001}abc", &mut ExtractionReport::new("pdf")).is_err());
        let mut report = ExtractionReport::new("pdf");
        assess_text(
            "This paragraph contains one damaged glyph: \u{fffd}",
            &mut report,
        )
        .unwrap();
        assert_eq!(report.status, QualityStatus::Warn);
        assert_eq!(report.diagnostics.len(), 1);
    }
}
