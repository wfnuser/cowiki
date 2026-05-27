mod client;
mod commands;
mod completions;
mod config;
mod error;
mod output;
mod types;

use clap::{Parser, Subcommand};
#[derive(Parser)]
#[command(name = "cowiki", version, about = "CLI client for cowiki - collaborative wiki")]
struct Cli {
    /// Override server URL (default: http://localhost:3000 or from config)
    #[arg(long, global = true)]
    server: Option<String>,

    /// Target workspace slug for workspace-scoped operations
    #[arg(short = 'w', long, global = true)]
    workspace: Option<String>,

    /// Machine-readable JSON output
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest a source document into the wiki
    Ingest {
        /// Source type: url, text, or file
        #[arg(long, default_value = "url")]
        r#type: String,

        /// The content (URL, text, or file path)
        #[arg(long)]
        content: Option<String>,

        /// Branch to ingest into (default: user/<id> if authenticated, else "main")
        #[arg(long)]
        branch: Option<String>,
    },
    /// Compile sources into wiki pages
    Compile {
        /// Branch to compile (default: user/<id> if authenticated, else "main")
        #[arg(long)]
        branch: Option<String>,

        /// Timeout in seconds (default: 120)
        #[arg(long, default_value = "120")]
        timeout: u64,
    },
    /// Submit pages for review
    Submit {
        /// Page slugs to submit
        #[arg(required_unless_present = "all")]
        slugs: Vec<String>,

        /// Branch to submit from (default: user/<id> if authenticated, else "main")
        #[arg(long)]
        branch: Option<String>,

        /// Submit all pages on the branch
        #[arg(long, conflicts_with = "slugs")]
        all: bool,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Review submissions
    Review {
        #[command(subcommand)]
        cmd: commands::review::ReviewCommand,
    },
    /// Search wiki pages
    Search {
        /// Search query text
        query: String,

        /// Max results to return (default: 10)
        #[arg(long, default_value = "10")]
        limit: u32,

        /// Branch to search (default: user/<id> if authenticated, else "main")
        #[arg(long)]
        branch: Option<String>,
    },
    /// Read a wiki page
    Read {
        /// Page slug to read
        slug: String,

        /// Branch to read from (default: user/<id> if authenticated, else "main")
        #[arg(long)]
        branch: Option<String>,

        /// Print directly to stdout instead of using a pager
        #[arg(long)]
        no_pager: bool,
    },
    /// Write a wiki page
    Write {
        /// Page slug
        slug: String,

        /// Page title (used in editor template)
        #[arg(long)]
        title: Option<String>,

        /// Page body content (inline text)
        #[arg(long)]
        body: Option<String>,

        /// Branch to write to (default: user/<id> if authenticated, else "main")
        #[arg(long)]
        branch: Option<String>,

        /// Change summary
        #[arg(long)]
        summary: Option<String>,
    },
    /// List wiki pages
    List {
        /// Branch to list pages from (default: user/<id> if authenticated, else "main")
        #[arg(long)]
        branch: Option<String>,
    },
    /// List available workspaces
    Workspaces,
    /// Generate shell completions
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for
        shell: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let config = config::Config::load().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    let server_url = cli.server.clone().unwrap_or(config.server_url);

    let client = client::CowikiClient::new(server_url, config.api_key);

    // Always use user/<id> branch when authenticated; fall back to main.
    // Personal spaces only serve from user/<id>, and team spaces use user/<id>
    // for draft isolation, so this is the correct default for all operations.
    let default_branch = resolve_user_branch(&client).await;

    // Auto-resolve personal workspace when no -w is given.
    // Personal workspace slug = personal-{first_8_chars_of_user_id}.
    let effective_workspace = if cli.workspace.is_some() {
        cli.workspace
    } else {
        resolve_personal_workspace(&client).await
    };

    let result = match cli.command {
        Commands::Search {
            query,
            limit,
            branch,
        } => {
            let branch = branch.unwrap_or(default_branch);
            commands::search::run(&client, &query, limit, &branch, cli.json).await
        }
        Commands::Read {
            slug,
            branch,
            no_pager,
        } => {
            let branch = branch.unwrap_or(default_branch);
            commands::read::run(&client, &slug, &branch, effective_workspace.as_deref(), no_pager, cli.json).await
        }
        Commands::List { branch } => {
            let branch = branch.unwrap_or(default_branch);
            commands::list::run(&client, &branch, effective_workspace.as_deref(), cli.json).await
        }
        Commands::Workspaces => {
            commands::workspace::run(&client, cli.json).await
        }

        Commands::Ingest {
            r#type,
            content,
            branch,
        } => {
            let branch = branch.unwrap_or(default_branch);
            commands::ingest::run(&client, &branch, effective_workspace.as_deref(), r#type, content, cli.json).await
        }
        Commands::Compile { branch, timeout } => {
            let branch = branch.unwrap_or(default_branch);
            commands::compile::run(&client, branch, effective_workspace.as_deref(), timeout, cli.json).await
        }
        Commands::Submit {
            slugs,
            branch,
            all,
            yes,
        } => {
            let branch = branch.unwrap_or(default_branch);
            commands::submit::run(&client, branch, slugs, all, yes, cli.json).await
        }
        Commands::Review { cmd } => {
            commands::review::run(&client, cmd, cli.json).await
        }
        Commands::Write {
            slug,
            title,
            body,
            branch,
            summary,
        } => {
            let branch = branch.unwrap_or(default_branch);
            commands::write::run(&client, &branch, effective_workspace.as_deref(), slug, title, body, summary, cli.json).await
        }
        Commands::Completions { shell } => {
            completions::generate(&shell);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// Resolve user branch for shared workspace draft isolation.
/// Returns `user/{user_id}` if authenticated, else `"main"`.
async fn resolve_user_branch(client: &client::CowikiClient) -> String {
    match client.get_me().await {
        Ok(user) => format!("user/{}", user.id),
        Err(_) => "main".to_string(),
    }
}

/// Resolve the personal workspace slug automatically.
/// Returns `Some(slug)` if the authenticated user has a private workspace where they are owner.
async fn resolve_personal_workspace(client: &client::CowikiClient) -> Option<String> {
    match client.list_workspaces().await {
        Ok(ws_list) => ws_list
            .iter()
            .find(|w| w.visibility == "private" && w.role == "owner")
            .map(|w| w.slug.clone()),
        Err(_) => None,
    }
}
