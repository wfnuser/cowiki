use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;

pub async fn run(
    client: &CowikiClient,
    query: &str,
    limit: u32,
    branch: &str,
    json: bool,
) -> Result<(), CliError> {
    let results = client.search(query, Some(limit), Some(branch)).await?;

    if results.is_empty() {
        output::print_info(&format!("no results for \"{query}\""));
        return Ok(());
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&results).map_err(|e| {
            CliError::Unexpected(format!("failed to serialize results: {e}"))
        })?);
        return Ok(());
    }

    let mut table = comfy_table::Table::new();
    table.set_header(vec!["SLUG", "TITLE", "SUMMARY", "SIMILARITY"]);
    for r in &results {
        table.add_row(vec![
            &r.slug,
            &r.title,
            &r.summary,
            &format!("{:.4}", r.similarity),
        ]);
    }
    output::print_table(table);

    Ok(())
}
