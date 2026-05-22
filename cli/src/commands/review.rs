use clap::Subcommand;

use crate::client::CowikiClient;
use crate::error::CliError;
use crate::output;
use colored::Colorize;

#[derive(Subcommand)]
pub enum ReviewCommand {
    /// List review submissions
    List {
        /// Filter by status (pending/approved/rejected)
        #[arg(long)]
        status: Option<String>,
    },
    /// Show review details with diffs
    Show {
        /// Submission ID
        id: String,
    },
    /// Approve a submission
    Approve {
        /// Submission ID
        id: String,
    },
    /// Reject a submission
    Reject {
        /// Submission ID
        id: String,
    },
}

pub async fn run(
    client: &CowikiClient,
    cmd: ReviewCommand,
    json: bool,
) -> Result<(), CliError> {
    match cmd {
        ReviewCommand::List { status } => list_reviews(client, status, json).await,
        ReviewCommand::Show { id } => show_review(client, &id, json).await,
        ReviewCommand::Approve { id } => approve_review(client, &id, json).await,
        ReviewCommand::Reject { id } => reject_review(client, &id, json).await,
    }
}

async fn list_reviews(
    client: &CowikiClient,
    status: Option<String>,
    json: bool,
) -> Result<(), CliError> {
    let mut submissions = client.list_reviews().await?;

    if let Some(ref s) = status {
        submissions.retain(|sub| sub.status.eq_ignore_ascii_case(s));
    }

    if submissions.is_empty() {
        output::print_info("no reviews found");
        return Ok(());
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&submissions).map_err(|e| {
                CliError::Unexpected(format!("failed to serialize reviews: {e}"))
            })?
        );
        return Ok(());
    }

    let mut table = comfy_table::Table::new();
    table.set_header(vec!["ID", "USER", "STATUS", "SUMMARY", "CREATED"]);
    for s in &submissions {
        let status_colored = match s.status.as_str() {
            "pending" => s.status.yellow().to_string(),
            "approved" => s.status.green().to_string(),
            "rejected" => s.status.red().to_string(),
            _ => s.status.clone(),
        };
        table.add_row(vec![
            &s.id,
            &s.user_id,
            &status_colored,
            &s.summary,
            &s.created_at,
        ]);
    }
    output::print_table(table);

    Ok(())
}

async fn show_review(client: &CowikiClient, id: &str, json: bool) -> Result<(), CliError> {
    let detail = client.get_review(id).await.map_err(|e| match &e {
        CliError::Api { status: 404, .. } => {
            CliError::Unexpected(format!("review not found: \"{id}\""))
        }
        _ => e,
    })?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&detail).map_err(|e| {
                CliError::Unexpected(format!("failed to serialize review: {e}"))
            })?
        );
        return Ok(());
    }

    let sub = &detail.submission;
    let status_colored = match sub.status.as_str() {
        "pending" => sub.status.yellow(),
        "approved" => sub.status.green(),
        "rejected" => sub.status.red(),
        _ => sub.status.normal(),
    };

    println!("{} {}", "Submission:".bold(), sub.id);
    println!("  User:     {}", sub.user_id);
    println!("  Status:   {status_colored}");
    println!("  Summary:  {}", sub.summary);
    println!("  Branch:   {}", sub.source_branch);
    println!("  Created:  {}", sub.created_at);
    if let Some(ref reviewer) = sub.reviewed_by {
        println!("  Reviewed by: {reviewer}");
    }
    if let Some(ref reviewed_at) = sub.reviewed_at {
        println!("  Reviewed at: {reviewed_at}");
    }
    println!();

    if detail.diffs.is_empty() {
        output::print_info("no diffs");
    } else {
        for diff in &detail.diffs {
            println!("{} {}", "---".yellow(), diff.path);
            if let Some(ref old) = diff.old_content {
                for line in old.lines() {
                    println!("{}{line}", "- ".red());
                }
            }
            if let Some(ref new) = diff.new_content {
                for line in new.lines() {
                    println!("{}{line}", "+ ".green());
                }
            }
            println!();
        }
    }

    Ok(())
}

async fn approve_review(client: &CowikiClient, id: &str, json: bool) -> Result<(), CliError> {
    client.approve_review(id).await?;

    if json {
        println!("{{\"ok\": true}}");
    } else {
        output::print_success(&format!("approved submission \"{id}\""));
    }

    Ok(())
}

async fn reject_review(client: &CowikiClient, id: &str, json: bool) -> Result<(), CliError> {
    client.reject_review(id).await?;

    if json {
        println!("{{\"ok\": true}}");
    } else {
        output::print_success(&format!("rejected submission \"{id}\""));
    }

    Ok(())
}
