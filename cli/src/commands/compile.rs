use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;
use crate::types::{CompileRequest, CompileResponse};
use indicatif::ProgressBar;

pub async fn run(
    client: &CowikiClient,
    branch: String,
    workspace: Option<&str>,
    timeout_secs: u64,
    json: bool,
) -> Result<(), CliError> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_message("Compiling sources...");
    spinner.enable_steady_tick(Duration::from_millis(120));

    let req = CompileRequest {
        branch: branch.clone(),
    };

    let compile_fut: Pin<Box<dyn Future<Output = Result<CompileResponse, CliError>>>> = if let Some(ws) = workspace {
        Box::pin(client.compile_ws(ws, req))
    } else {
        Box::pin(client.compile(req))
    };

    let result = tokio::select! {
        r = tokio::time::timeout(Duration::from_secs(timeout_secs), compile_fut) => {
            match r {
                Ok(Ok(resp)) => Some(resp),
                Ok(Err(e)) => return Err(e),
                Err(_elapsed) => None,
            }
        }
        _ = tokio::signal::ctrl_c() => {
            spinner.finish_and_clear();
            output::print_info("Compilation request sent, server may still be processing");
            return Ok(());
        }
    };

    spinner.finish_and_clear();

    match result {
        Some(resp) => {
            if json {
                let j = serde_json::to_string_pretty(&resp.pages)
                    .map_err(|e| CliError::Unexpected(format!("JSON serialize: {e}")))?;
                println!("{j}");
            } else {
                if resp.pages.is_empty() {
                    output::print_info("No sources to compile or all already compiled.");
                    if resp.skipped > 0 {
                        output::print_info(&format!(
                            "{} source(s) skipped (already compiled).",
                            resp.skipped
                        ));
                    }
                } else {
                    let mut table = comfy_table::Table::new();
                    table.set_header(vec!["SLUG", "TITLE", "SUMMARY"]);
                    for p in &resp.pages {
                        table.add_row(vec![&p.slug, &p.title, &p.summary]);
                    }
                    output::print_table(table);
                    if resp.skipped > 0 {
                        output::print_info(&format!(
                            "{} source(s) skipped (already compiled).",
                            resp.skipped
                        ));
                    }
                }
            }
        }
        None => {
            output::print_info("Compilation still in progress, check server later");
        }
    }

    Ok(())
}
