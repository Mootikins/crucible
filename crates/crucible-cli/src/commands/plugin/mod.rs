//! Plugin management CLI commands
//!
//! Provides CLI commands for developing, testing, and managing Lua plugins.

use anyhow::Result;
use clap::Subcommand;

use crate::config::CliConfig;

mod add;
mod health;
mod new;
mod remove;
mod stubs;
mod test;
mod update;

pub use add::AddArgs;
pub use remove::RemoveArgs;
pub use update::UpdateArgs;

#[derive(Debug, Subcommand)]
pub enum PluginCommands {
    /// Run plugin tests in a sandboxed Lua runtime
    Test(TestArgs),
    /// Scaffold a new plugin from template
    New(NewArgs),
    /// Generate LuaLS type stubs for IDE autocomplete
    Stubs(StubsArgs),
    /// Run plugin health checks
    Health(HealthArgs),
    /// Add a plugin from a git URL
    Add(AddArgs),
    /// Remove a plugin declaration
    Remove(RemoveArgs),
    /// Update installed plugins (git pull)
    Update(UpdateArgs),
}

#[derive(Debug, clap::Parser)]
pub struct TestArgs {
    /// Path to plugin directory or test file
    pub path: std::path::PathBuf,
    /// Filter: only run tests matching this pattern
    #[arg(long)]
    pub filter: Option<String>,
}

#[derive(Debug, clap::Parser)]
pub struct NewArgs {
    /// Plugin name (used as directory name and plugin identifier)
    pub name: String,
    /// Output directory (defaults to current directory)
    #[arg(long, short = 'o')]
    pub output: Option<std::path::PathBuf>,
    /// Overwrite if directory already exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, clap::Parser)]
pub struct StubsArgs {
    /// Output directory for generated stubs (defaults to ./stubs)
    #[arg(long, short = 'o')]
    pub output: Option<std::path::PathBuf>,
    /// Verify stubs match committed version (for CI)
    #[arg(long)]
    pub verify: bool,
}

#[derive(Debug, clap::Parser)]
pub struct HealthArgs {
    /// Path to plugin directory
    pub path: std::path::PathBuf,
    /// Output results as JSON
    #[arg(long)]
    pub json: bool,
}

/// Execute plugin subcommand
pub async fn execute(config: CliConfig, cmd: PluginCommands) -> Result<()> {
    match cmd {
        PluginCommands::Test(args) => test::execute(config, args).await,
        PluginCommands::New(args) => new::execute(config, args).await,
        PluginCommands::Stubs(args) => stubs::execute(config, args).await,
        PluginCommands::Health(args) => health::execute(config, args).await,
        PluginCommands::Add(args) => add::execute(args).await,
        PluginCommands::Remove(args) => remove::execute(args).await,
        PluginCommands::Update(args) => update::execute(args).await,
    }
}
