use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;

pub async fn run(
    client: &CowikiClient,
    branch: &str,
    json: bool,
) -> Result<(), CliError> {
    let pages = client.list_pages(branch).await?;

    if pages.is_empty() {
        output::print_info(&format!("no pages on branch \"{branch}\""));
        return Ok(());
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&pages)
                .map_err(|e| CliError::Unexpected(format!("failed to serialize pages: {e}")))?
        );
        return Ok(());
    }

    let mut table = comfy_table::Table::new();
    table.set_header(vec!["SLUG", "TITLE", "UPDATED"]);
    for p in &pages {
        table.add_row(vec![&p.slug, &p.title, &p.updated_at]);
    }
    output::print_table(table);

    Ok(())
}
