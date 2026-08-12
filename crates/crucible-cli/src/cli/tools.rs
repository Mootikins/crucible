use crate::formatting::OutputFormat;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ToolsCommands {
    /// List available tools
    List {
        /// Output in permission rule format (tool:pattern)
        #[arg(long)]
        permissions: bool,
        /// Output format
        #[arg(short = 'f', long, default_value_t)]
        format: OutputFormat,
    },
}
