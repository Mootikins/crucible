//! zellij-inbox CLI

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};
use zellij_inbox::{file, Inbox, InboxItem, Status};

#[derive(Parser)]
#[command(name = "zellij-inbox")]
#[command(about = "Agent inbox for Zellij")]
struct Cli {
    /// Override inbox file path
    #[arg(long, short = 'f', env = "ZELLIJ_INBOX_FILE")]
    file: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add or update an item
    Add {
        /// Display text (e.g., "claude-code: Waiting for input")
        text: String,

        /// Pane ID (unique key)
        #[arg(long, short = 'p', env = "ZELLIJ_PANE_ID")]
        pane: u32,

        /// Project name
        #[arg(long)]
        project: String,

        /// Status: wait or work
        #[arg(long, short = 's', default_value = "wait")]
        status: String,
    },

    /// Remove an item
    Remove {
        /// Pane ID to remove
        #[arg(long, short = 'p', env = "ZELLIJ_PANE_ID")]
        pane: u32,
    },

    /// List all items
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Clear all items
    Clear,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Determine inbox file path
    let path = match cli.file {
        Some(p) => p,
        None => file::inbox_path()?,
    };

    match cli.command {
        Commands::Add {
            text,
            pane,
            project,
            status,
        } => {
            let status = match status.as_str() {
                "wait" | "waiting" => Status::Waiting,
                "work" | "working" => Status::Working,
                other => {
                    return Err(format!("invalid status '{}': use 'wait' or 'work'", other).into())
                }
            };

            let mut inbox = file::load(&path)?;
            inbox.upsert(InboxItem {
                text,
                pane_id: pane,
                project,
                status,
            });
            file::save(&path, &inbox)?;
            println!("Added item for pane {}", pane);
        }

        Commands::Remove { pane } => {
            let mut inbox = file::load(&path)?;
            if inbox.remove(pane) {
                file::save(&path, &inbox)?;
                println!("Removed item for pane {}", pane);
            } else {
                println!("No item found for pane {}", pane);
            }
        }

        Commands::List { json } => {
            let inbox = file::load(&path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&inbox)?);
            } else if inbox.is_empty() {
                println!("(no items)");
            } else {
                for item in &inbox.items {
                    println!(
                        "[{}] {} ({}) pane:{}",
                        item.status.to_char(),
                        item.text,
                        item.project,
                        item.pane_id
                    );
                }
            }
        }

        Commands::Clear => {
            let inbox = Inbox::new();
            file::save(&path, &inbox)?;
            println!("Cleared inbox");
        }
    }

    Ok(())
}
