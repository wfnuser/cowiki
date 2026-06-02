use std::io::{IsTerminal, Read};

use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;
use crate::types::IngestRequest;

pub async fn run(
    client: &CowikiClient,
    branch: &str,
    workspace: Option<&str>,
    source_type: String,
    content_arg: Option<String>,
    json: bool,
) -> Result<(), CliError> {
    let content = resolve_content(&source_type, content_arg)?;

    let req = IngestRequest {
        source_type,
        content,
        filename: None,
        branch: branch.to_string(),
    };

    let resp = if let Some(ws) = workspace {
        client.ingest_ws(ws, req).await?
    } else {
        client.ingest(req).await?
    };

    if json {
        let j = serde_json::to_string_pretty(&resp)
            .map_err(|e| CliError::Unexpected(format!("JSON serialize: {e}")))?;
        println!("{j}");
    } else {
        output::print_success(&format!(
            "Ingested: {} (hash: {})",
            resp.filename, resp.content_hash
        ));
    }

    Ok(())
}

fn resolve_content(source_type: &str, content_arg: Option<String>) -> Result<String, CliError> {
    if let Some(c) = content_arg {
        match source_type {
            "url" => {
                if !c.starts_with("http://") && !c.starts_with("https://") {
                    return Err(CliError::Config(
                        "URL must start with http:// or https://".into(),
                    ));
                }
                Ok(c)
            }
            "text" => Ok(c),
            "file" => {
                std::fs::read_to_string(&c)
                    .map_err(|e| CliError::Unexpected(format!("Cannot read file '{c}': {e}")))
            }
            other => Err(CliError::Config(format!(
                "Unknown source type '{other}'. Use url, text, or file."
            ))),
        }
    } else {
        // Read from stdin
        if std::io::stdin().is_terminal() {
            return Err(CliError::Config(
                "No --content provided and no stdin pipe. Provide --content or pipe input.".into(),
            ));
        }
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| CliError::Unexpected(format!("Failed to read stdin: {e}")))?;
        if buf.trim().is_empty() {
            return Err(CliError::Config("Stdin was empty.".into()));
        }
        Ok(buf.trim().to_string())
    }
}
