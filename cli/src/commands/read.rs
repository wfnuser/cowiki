use std::io::Write;
use std::process::{Command, Stdio};

use crate::client::CowikiClient;
use crate::error::CliError;

pub async fn run(
    client: &CowikiClient,
    slug: &str,
    branch: &str,
    no_pager: bool,
    json: bool,
) -> Result<(), CliError> {
    let page = client.get_page(slug, branch).await.map_err(|e| match &e {
        CliError::Api { status: 404, .. } => {
            CliError::Unexpected(format!("page not found: \"{slug}\" on branch \"{branch}\""))
        }
        _ => e,
    })?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&page)
                .map_err(|e| CliError::Unexpected(format!("failed to serialize page: {e}")))?
        );
        return Ok(());
    }

    let title_line = format!("\n  {}\n", colored::Colorize::bold(page.title.as_str()));
    let info_line = format!(
        "  slug: {} | branch: {}\n\n",
        page.slug, page.branch
    );

    let full_output = format!("{title_line}{info_line}{}", page.body);

    if no_pager {
        println!("{full_output}");
    } else {
        let pager = std::env::var("PAGER").unwrap_or_else(|_| "less -R".into());
        let pager_parts: Vec<&str> = pager.split_whitespace().collect();
        let (pager_cmd, pager_args) = pager_parts.split_first().unwrap_or((&"less", &["-R"][..]));

        let mut child = Command::new(pager_cmd)
            .args(pager_args)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| {
                CliError::Unexpected(format!("failed to spawn pager \"{pager}\": {e}"))
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(full_output.as_bytes())
                .map_err(|e| CliError::Unexpected(format!("failed to write to pager: {e}")))?;
        }

        child
            .wait()
            .map_err(|e| CliError::Unexpected(format!("pager error: {e}")))?;
    }

    Ok(())
}
