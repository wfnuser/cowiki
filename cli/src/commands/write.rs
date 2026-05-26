use std::io::{IsTerminal, Read};

use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;
use crate::types::WritePageRequest;

pub async fn run(
    client: &CowikiClient,
    branch: &str,
    workspace: Option<&str>,
    slug: String,
    title: Option<String>,
    body_arg: Option<String>,
    _summary: Option<String>,
    json: bool,
) -> Result<(), CliError> {
    let body = resolve_body(&slug, title.as_deref(), body_arg)?;

    let req = WritePageRequest {
        slug: slug.clone(),
        body,
        branch: branch.to_string(),
    };

    let resp = if let Some(ws) = workspace {
        client.write_page_ws(ws, req).await?
    } else {
        client.write_page(req).await?
    };

    if json {
        let j = serde_json::to_string_pretty(&resp)
            .map_err(|e| CliError::Unexpected(format!("JSON serialize: {e}")))?;
        println!("{j}");
    } else {
        if resp.ok {
            output::print_success(&format!("Page created/updated: {}", resp.slug));
        } else {
            output::print_error(&format!("Page write returned ok=false for: {}", resp.slug));
        }
    }

    Ok(())
}

fn resolve_body(
    slug: &str,
    title: Option<&str>,
    body_arg: Option<String>,
) -> Result<String, CliError> {
    if let Some(b) = body_arg {
        return Ok(b);
    }

    // Try reading from stdin pipe
    if !std::io::stdin().is_terminal() {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| CliError::Unexpected(format!("Failed to read stdin: {e}")))?;
        let trimmed = buf.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    // Stdin is a TTY and no --body: open $EDITOR
    edit_in_editor(slug, title)
}

fn edit_in_editor(slug: &str, title: Option<&str>) -> Result<String, CliError> {
    let editor = find_editor();
    let display_title = title.unwrap_or(slug);

    let template = format!(
        "---\ntitle: {display_title}\n---\n\n# {display_title}\n\nStart writing...\n"
    );

    let tmp_path = std::env::temp_dir().join(format!("cowiki-edit-{slug}.md"));
    std::fs::write(&tmp_path, &template)
        .map_err(|e| CliError::Unexpected(format!("Cannot create temp file: {e}")))?;

    let status = std::process::Command::new(&editor)
        .arg(&tmp_path)
        .status()
        .map_err(|e| {
            CliError::Unexpected(format!("Failed to open editor '{editor}': {e}"))
        })?;

    if !status.success() {
        return Err(CliError::Unexpected(format!(
            "Editor '{editor}' exited with error"
        )));
    }

    let content = std::fs::read_to_string(&tmp_path)
        .map_err(|e| CliError::Unexpected(format!("Cannot read edited file: {e}")))?;

    // Best-effort cleanup
    let _ = std::fs::remove_file(&tmp_path);

    let body = extract_body(&content);
    if body.is_empty() {
        return Err(CliError::Config("Editor content is empty, aborting.".into()));
    }
    Ok(body)
}

/// Extract the body from editor content, skipping YAML frontmatter.
fn extract_body(content: &str) -> String {
    let content = content.trim();
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(idx) = rest.find("\n---\n") {
            return rest[idx + 5..].trim().to_string();
        }
        // Malformed frontmatter: no closing ---, return as-is
        return content.to_string();
    }
    content.to_string()
}

fn find_editor() -> String {
    if let Ok(editor) = std::env::var("EDITOR") {
        if !editor.is_empty() {
            return editor;
        }
    }
    // Fallback: try common editors
    for cmd in &["vim", "nano", "vi"] {
        if editor_exists(cmd) {
            return cmd.to_string();
        }
    }
    "vi".to_string()
}

fn editor_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
