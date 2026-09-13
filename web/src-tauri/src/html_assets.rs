use std::io::Read;
use std::path::Path;

pub fn is_html(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}
pub fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("HTML asset exceeds 8 MiB".into());
    }
    Ok(bytes)
}

pub fn read_asset(root: &Path, document: &str, asset: &str) -> Result<Vec<u8>, String> {
    fn checked(root: &Path, relative: &Path) -> Result<std::path::PathBuf, String> {
        let mut current = root.to_path_buf();
        for component in relative.components() {
            let std::path::Component::Normal(part) = component else {
                return Err("Invalid HTML resource path".into());
            };
            if part.to_string_lossy().starts_with('.') {
                return Err("Hidden resources are not allowed".into());
            }
            current.push(part);
            if std::fs::symlink_metadata(&current)
                .map_err(|e| e.to_string())?
                .file_type()
                .is_symlink()
            {
                return Err("Symbolic links are not allowed".into());
            }
        }
        Ok(current)
    }
    let doc = Path::new(document);
    if !is_html(doc) {
        return Err("Expected an HTML document".into());
    }
    let document_file = checked(root, doc)?;
    if !document_file.is_file() {
        return Err("HTML document not found".into());
    }
    let asset = Path::new(asset);
    if !asset.starts_with(doc.parent().ok_or("Missing HTML directory")?) {
        return Err("Resource is outside the HTML directory".into());
    }
    let extension = asset
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ![
        "css", "js", "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "woff", "woff2", "ttf",
        "otf", "mp3", "wav", "mp4", "webm",
    ]
    .contains(&extension.as_str())
    {
        return Err("Unsupported HTML resource type".into());
    }
    let file = checked(root, asset)?;
    if !file.is_file() {
        return Err("HTML resource is not a file".into());
    }
    read_limited(&file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_only_package_assets_and_rejects_escape_hidden_and_links() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("paper/assets")).unwrap();
        std::fs::write(root.join("paper/index.html"), "<h1>Paper</h1>").unwrap();
        std::fs::write(root.join("paper/assets/main.js"), "window.demo = true;").unwrap();
        std::fs::write(root.join("outside.js"), "private").unwrap();
        assert_eq!(
            read_asset(root, "paper/index.html", "paper/assets/main.js").unwrap(),
            b"window.demo = true;"
        );
        for path in [
            "outside.js",
            "paper/../outside.js",
            "/etc/passwd",
            "paper/.env",
            "paper/assets/secrets.json",
        ] {
            assert!(
                read_asset(root, "paper/index.html", path).is_err(),
                "{path}"
            );
        }
        assert!(read_asset(root, "outside.js", "outside.js").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("outside.js"), root.join("paper/assets/link.js"))
                .unwrap();
            assert!(read_asset(root, "paper/index.html", "paper/assets/link.js").is_err());
        }
    }
}
