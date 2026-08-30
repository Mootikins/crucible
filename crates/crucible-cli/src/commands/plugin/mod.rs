//! Plugin management CLI commands
//!
//! Provides CLI commands for developing, testing, and managing Lua plugins.

use anyhow::Result;
use clap::Subcommand;

use crate::config::CliConfig;

mod add;
mod check;
mod health;
mod list;
mod new;
mod remove;
mod stubs;
mod test;
mod update;

pub use add::AddArgs;
pub use list::ListArgs;
pub use remove::RemoveArgs;
pub use update::UpdateArgs;

/// Where a git-hosted plugin entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntrySource {
    /// `plugins.declare.<name>` in the config.
    Declared,
    /// `<data_home>/plugins.installed.json`.
    Installed,
}

impl EntrySource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            EntrySource::Declared => "declared",
            EntrySource::Installed => "installed",
        }
    }
}

/// Every configured git-hosted plugin: the config-declared entries plus the
/// installed manifest, name-deduplicated with the declaration winning (the
/// same rule the daemon's bootstrap union applies).
///
/// Best-effort on the config side: an unreadable config yields the manifest
/// alone, with the reason surfaced to the caller.
pub(crate) async fn configured_plugin_entries() -> Result<(
    Vec<(String, crucible_core::config::PluginEntry, EntrySource)>,
    Vec<String>,
)> {
    let mut notes = Vec::new();

    let declared = match crate::config::fetch_effective_config(None, None, None).await {
        Ok(config) => {
            let (entries, warnings) = crucible_core::config::declared_plugins(&config.plugins);
            notes.extend(warnings);
            entries
        }
        Err(e) => {
            notes.push(format!(
                "could not read the config for declared plugins: {e}"
            ));
            Vec::new()
        }
    };

    let manifest_path = crucible_daemon::plugin_ops::installed_manifest_path(
        &crucible_core::config::crucible_home(),
    );
    let installed = match crucible_daemon::plugin_ops::installed_entries(&manifest_path) {
        Ok(entries) => entries,
        Err(e) => {
            notes.push(format!("could not read {}: {e}", manifest_path.display()));
            Vec::new()
        }
    };

    let declared_names: std::collections::BTreeSet<String> =
        declared.iter().map(|(name, _)| name.clone()).collect();
    let mut entries: Vec<_> = declared
        .into_iter()
        .map(|(name, entry)| (name, entry, EntrySource::Declared))
        .collect();
    for (name, entry) in installed {
        if declared_names.contains(&name) {
            notes.push(format!(
                "plugin '{name}': the config declaration supersedes the installed manifest entry"
            ));
        } else {
            entries.push((name, entry, EntrySource::Installed));
        }
    }
    Ok((entries, notes))
}

#[derive(Debug, Subcommand)]
pub enum PluginCommands {
    /// Run plugin tests in a sandboxed Lua runtime
    Test(TestArgs),
    /// Scaffold a new plugin from template
    New(NewArgs),
    /// Generate LuaLS type stubs for IDE autocomplete
    Stubs(StubsArgs),
    /// Check a plugin: it parses, its declarations are readable, and (with
    /// `luau-analyze` installed) it typechecks
    Check(CheckArgs),
    /// Run plugin health checks
    Health(HealthArgs),
    /// Add a plugin from a git URL
    Add(AddArgs),
    /// List declared plugins and their clone status
    List(ListArgs),
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
    /// Build the plugin VM in this process instead of asking a daemon.
    /// For CI, and for anyone who wants declarations without a running
    /// daemon.
    #[arg(long)]
    pub offline: bool,
    /// Verify stubs match committed version (for CI)
    #[arg(long)]
    pub verify: bool,
}

#[derive(Debug, clap::Parser)]
pub struct CheckArgs {
    /// Path to the plugin directory
    pub path: std::path::PathBuf,
    /// Typecheck the plugin's test suite too. Off by default: a suite
    /// monkey-patches the host on purpose, and every stub is a type error
    /// against declarations that describe the real host. Tests are always
    /// parse-checked.
    #[arg(long)]
    pub include_tests: bool,
    /// Luau declaration file to check against (defaults to the generated
    /// `cru.d.luau` in the stub directory)
    #[arg(long)]
    pub definitions: Option<std::path::PathBuf>,
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
        PluginCommands::Check(args) => check::execute(config, args).await,
        PluginCommands::Health(args) => health::execute(config, args).await,
        PluginCommands::Add(args) => add::execute(args).await,
        PluginCommands::List(args) => list::execute(args).await,
        PluginCommands::Remove(args) => remove::execute(args).await,
        PluginCommands::Update(args) => update::execute(args).await,
    }
}
