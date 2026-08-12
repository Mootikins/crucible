use crate::formatting::OutputFormat;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ToolsCommands {
    /// List available tools
    List {
        /// Output in permission rule format (tool:pattern)
        #[arg(long)]
        permissions: bool,
        /// Output format. Defaults to a table on a terminal, plain lines when
        /// piped or redirected.
        #[arg(short = 'f', long)]
        format: Option<OutputFormat>,
    },
}
