//! Explicitly opted-in local converters. No shell, inherited credentials or network provider.
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_TOOL_BYTES: u64 = 32 * 1024 * 1024;
const MAX_OCR_PAGES: usize = 20;

pub struct Conversion {
    pub text: String,
    pub extractor: &'static str,
    pub pages: Option<usize>,
}

pub fn convert(
    path: &Path,
    format: &str,
    attempts: &mut Vec<String>,
) -> Result<Conversion, String> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(90);
    match format {
        "doc" => {
            attempts.push("antiword".to_string());
            run("antiword", &[path.as_os_str()], deadline).map(|text| Conversion {
                text,
                extractor: "antiword",
                pages: None,
            })
        }
        "pdf" => {
            attempts.push("pdfinfo".to_string());
            let info = run("pdfinfo", &[path.as_os_str()], deadline)?;
            let pages = info
                .lines()
                .find_map(|line| line.strip_prefix("Pages:")?.trim().parse::<usize>().ok())
                .filter(|pages| *pages > 0)
                .ok_or_else(|| {
                    "Cannot determine the PDF page count for complete extraction.".to_string()
                })?;
            attempts.push("pdftotext".to_string());
            let text = run(
                "pdftotext",
                &[
                    OsStr::new("-layout"),
                    OsStr::new("-enc"),
                    OsStr::new("UTF-8"),
                    path.as_os_str(),
                    OsStr::new("-"),
                ],
                deadline,
            );
            let page_texts = text.ok().and_then(|text| split_pdf_pages(&text, pages));
            if let Some(texts) = &page_texts {
                if texts.iter().all(|text| readable_page(text)) {
                    return Ok(Conversion {
                        text: texts.join("\n\n"),
                        extractor: "pdftotext",
                        pages: Some(pages),
                    });
                }
            }
            ocr_pdf(&path, pages, page_texts.as_deref(), deadline, attempts).map(|text| {
                Conversion {
                    text,
                    extractor: "pdftoppm+tesseract",
                    pages: Some(pages),
                }
            })
        }
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp" => {
            ocr_image(&path, deadline, attempts).map(|text| Conversion {
                text,
                extractor: "tesseract",
                pages: None,
            })
        }
        _ => Err("No local converter is available for this format.".to_string()),
    }
}

fn readable_page(text: &str) -> bool {
    !text.chars().any(super::quality::suspicious_character)
        && super::quality::assess_text(text, &mut super::ExtractionReport::new("pdf")).is_ok()
}

fn split_pdf_pages(text: &str, expected: usize) -> Option<Vec<String>> {
    // Poppler emits a form feed even after the final page. Preserve empty pages
    // until coverage has been checked; flattening them hides mixed scans.
    let pages = text
        .strip_suffix('\u{c}')
        .unwrap_or(text)
        .split('\u{c}')
        .map(|page| page.trim().to_string())
        .collect::<Vec<_>>();
    (pages.len() == expected).then_some(pages)
}

fn ocr_image(path: &Path, deadline: Instant, attempts: &mut Vec<String>) -> Result<String, String> {
    attempts.push("tesseract".to_string());
    run(
        "tesseract",
        &[path.as_os_str(), OsStr::new("stdout")],
        deadline,
    )
}

fn ocr_pdf(
    path: &Path,
    pages: usize,
    page_texts: Option<&[String]>,
    deadline: Instant,
    attempts: &mut Vec<String>,
) -> Result<String, String> {
    if pages == 0 || pages > MAX_OCR_PAGES {
        return Err(format!("Local PDF OCR supports 1–{MAX_OCR_PAGES} pages per file; this file has {pages}. Split or export it first."));
    }
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let prefix = directory.path().join("page");
    let mut sections = Vec::new();
    let mut bytes = 0;
    for page in 1..=pages {
        if let Some(text) = page_texts
            .and_then(|texts| texts.get(page - 1))
            .filter(|text| readable_page(text))
        {
            bytes += text.len();
            if bytes > MAX_TOOL_BYTES as usize {
                return Err("PDF output exceeds the extraction limit.".to_string());
            }
            sections.push(format!("## Page {page}\n\n{text}"));
            continue;
        }
        let number = OsString::from(page.to_string());
        attempts.push("pdftoppm".to_string());
        run(
            "pdftoppm",
            &[
                OsStr::new("-f"),
                &number,
                OsStr::new("-l"),
                &number,
                OsStr::new("-singlefile"),
                OsStr::new("-scale-to"),
                OsStr::new("2000"),
                OsStr::new("-png"),
                path.as_os_str(),
                prefix.as_os_str(),
            ],
            deadline,
        )?;
        let text = ocr_image(&prefix.with_extension("png"), deadline, attempts)?;
        if !readable_page(&text) {
            return Err(format!(
                "OCR produced no readable text for page {page}; no partial PDF was imported."
            ));
        }
        bytes += text.len();
        if bytes > MAX_TOOL_BYTES as usize {
            return Err("OCR output exceeds the extraction limit.".to_string());
        }
        sections.push(format!("## Page {page}\n\n{}", text.trim()));
    }
    Ok(sections.join("\n\n"))
}

fn run(program: &str, args: &[&OsStr], deadline: Instant) -> Result<String, String> {
    let directories = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    #[cfg(unix)]
    let directories = directories
        .into_iter()
        .chain(["/usr/local/bin", "/usr/bin", "/bin", "/opt/homebrew/bin"].map(PathBuf::from));
    let executable = resolve_program(program, directories, cfg!(windows)).ok_or_else(|| format!("Cannot find {program}. Install the local converter and add its directory to PATH, then restart CoWiki."))?;
    run_resolved(&executable, args, deadline)
}

fn resolve_program(
    program: &str,
    directories: impl IntoIterator<Item = PathBuf>,
    windows: bool,
) -> Option<PathBuf> {
    if program.is_empty() || program.contains(['/', '\\', ':']) {
        return None;
    }
    let name = if windows {
        format!("{program}.exe")
    } else {
        program.to_string()
    };
    directories
        .into_iter()
        .filter(|directory| directory.is_absolute())
        .find_map(|directory| {
            let path = directory.join(&name);
            if !path.is_file() {
                return None;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if !windows && path.metadata().ok()?.permissions().mode() & 0o111 == 0 {
                    return None;
                }
            }
            path.canonicalize().ok()
        })
}

fn run_resolved(program: &Path, args: &[&OsStr], deadline: Instant) -> Result<String, String> {
    let output = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
    let mut command = Command::new(program);
    command.args(args).env_clear().env("LC_ALL", "C");
    // Preserve only OS/runtime locations needed by native converters, never
    // Agent, Cloud or API credentials. Executable lookup happened before clearing.
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name).filter(|value| Path::new(value).is_absolute()) {
            command.env(name, value);
        }
    }
    if let Some(parent) = program.parent() {
        command.env("PATH", parent);
    }
    let label = program.file_name().unwrap_or_default().to_string_lossy();
    let mut child = command
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(output.reopen().map_err(|error| error.to_string())?)
        .spawn()
        .map_err(|_| {
            format!("Cannot start {label}. Install the local converter or import a text export.")
        })?;
    let status = loop {
        let oversized = output
            .as_file()
            .metadata()
            .map(|m| m.len() > MAX_TOOL_BYTES)
            .unwrap_or(true);
        if Instant::now() >= deadline || oversized {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{label} exceeded the local extraction time or output limit."
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Cannot wait for {label}: {error}"));
            }
        }
    };
    if !status.success() {
        return Err(format!("{label} could not extract this file. Check that the original is readable and the converter supports its format."));
    }
    let mut text = String::new();
    File::open(output.path())
        .map_err(|error| error.to_string())?
        .take(MAX_TOOL_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("{label} did not return UTF-8 text: {error}"))?;
    if text.len() > MAX_TOOL_BYTES as usize {
        return Err(format!("{label} output exceeds the extraction limit."));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn local_converter_timeout_is_bounded_and_missing_tools_are_actionable() {
        let error = run(
            "sleep",
            &[OsStr::new("10")],
            Instant::now() + Duration::from_millis(30),
        )
        .unwrap_err();
        assert!(error.contains("limit"));
        let error = run(
            "cowiki-nonexistent-converter",
            &[],
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.contains("Install"));
    }

    #[test]
    #[cfg(unix)]
    fn converter_arguments_are_not_interpreted_by_a_shell() {
        let text = run(
            "printf",
            &[
                OsStr::new("%s"),
                OsStr::new("literal; $(touch never-created)"),
            ],
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(text, "literal; $(touch never-created)");
    }

    #[test]
    fn windows_tools_resolve_exe_from_absolute_unicode_path_entries() {
        let root = tempfile::tempdir().unwrap();
        let tools = root.path().join("OCR 工具 with spaces");
        std::fs::create_dir(&tools).unwrap();
        for name in ["tesseract", "pdftotext", "antiword"] {
            let exe = tools.join(format!("{name}.exe"));
            std::fs::write(&exe, b"fixture").unwrap();
            assert_eq!(
                resolve_program(name, [PathBuf::from("relative"), tools.clone()], true),
                Some(exe.canonicalize().unwrap())
            );
        }
        std::fs::write(tools.join("wrapper.cmd"), b"fixture").unwrap();
        assert!(resolve_program("wrapper", [tools.clone()], true).is_none());
        assert!(resolve_program("../tesseract", [tools], true).is_none());
        assert!(resolve_program("tesseract", [PathBuf::from("relative")], true).is_none());
    }

    #[test]
    fn poppler_page_boundaries_preserve_missing_and_unreadable_pages() {
        let pages = split_pdf_pages("digital text\n\u{c}\u{c}", 2).unwrap();
        assert!(readable_page(&pages[0]));
        assert!(!readable_page(&pages[1]));
        assert!(!readable_page("\u{fffd}\u{fffd}"));
        assert!(split_pdf_pages("digital text\u{c}", 2).is_none());
        assert_eq!(
            split_pdf_pages("first\u{c}second\u{c}", 2).unwrap(),
            ["first", "second"]
        );
    }
}
