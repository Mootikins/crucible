use clap::Subcommand;

#[derive(Subcommand)]
pub enum ToolsCommands {
    /// List available tools
    List {
        /// Output in permission rule format (tool:pattern)
        #[arg(long)]
        permissions: bool,
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },
}
