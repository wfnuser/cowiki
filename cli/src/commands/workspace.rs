use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;

pub async fn run(
    client: &CowikiClient,
    json: bool,
) -> Result<(), CliError> {
    let workspaces = client.list_workspaces().await?;

    if workspaces.is_empty() {
        output::print_info("no workspaces found");
        return Ok(());
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&workspaces)
                .map_err(|e| CliError::Unexpected(format!("failed to serialize: {e}")))?
        );
        return Ok(());
    }

    let mut table = comfy_table::Table::new();
    table.set_header(vec!["NAME", "SLUG", "ROLE", "VISIBILITY"]);
    for w in &workspaces {
        table.add_row(vec![&w.name, &w.slug, &w.role, &w.visibility]);
    }
    output::print_table(table);

    Ok(())
}
