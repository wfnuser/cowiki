use std::io::{self, BufRead, Write};

use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;
use crate::types::SubmitRequest;

pub async fn run(
    client: &CowikiClient,
    branch: String,
    slugs: Vec<String>,
    all: bool,
    yes: bool,
    json: bool,
) -> Result<(), CliError> {
    let page_slugs = if all {
        let pages = client.list_pages(&branch).await?;
        if pages.is_empty() {
            output::print_info(&format!("no pages on branch \"{branch}\""));
            return Ok(());
        }
        pages.into_iter().map(|p| p.slug).collect()
    } else if slugs.is_empty() {
        return Err(CliError::Config(
            "No slugs specified. Provide slugs or use --all.".into(),
        ));
    } else {
        slugs
    };

    // Show summary
    let count = page_slugs.len();
    let slug_list = page_slugs.join(", ");
    output::print_info(&format!(
        "Submitting {count} page(s) from branch \"{branch}\": {slug_list}"
    ));

    // Confirm
    if !yes {
        print!("Proceed? [y/N] ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin()
            .lock()
            .read_line(&mut input)
            .map_err(|e| CliError::Unexpected(format!("Failed to read input: {e}")))?;
        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            output::print_info("Cancelled.");
            return Ok(());
        }
    }

    let req = SubmitRequest {
        branch,
        page_slugs,
    };

    let resp = client.submit(req).await?;

    if json {
        let j = serde_json::to_string_pretty(&resp)
            .map_err(|e| CliError::Unexpected(format!("JSON serialize: {e}")))?;
        println!("{j}");
    } else {
        output::print_success(&format!(
            "Submission created: {} — {}",
            resp.submission_id, resp.summary
        ));

        if !resp.duplicates.is_empty() {
            eprintln!();
            output::print_info("Duplicate warnings:");
            for dup in &resp.duplicates {
                eprintln!(
                    "  ⚠ {} → {} (similarity: {:.1}%)",
                    dup.new_slug,
                    dup.existing_slug,
                    dup.similarity * 100.0
                );
            }
        }
    }

    Ok(())
}
