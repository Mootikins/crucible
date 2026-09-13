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
    /// A `cru.plugin.setup` entry with a git source in the operator's
    /// `init.lua`.
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

/// A git-hosted plugin as `cru plugin list` and `cru plugin update` see it.
#[derive(Debug, Clone)]
pub(crate) struct GitPlugin {
    pub name: String,
    pub url: String,
    pub branch: Option<String>,
    pub pin: Option<String>,
    /// The merged `enabled`, with `true` for an entry that does not say.
    pub enabled: bool,
    pub source: EntrySource,
}

/// Every configured git-hosted plugin, from the spec the daemon holds.
///
/// The spec lives on the daemon's plugin VM: the operator's `init.lua`
/// entries and the installed manifest are merged there, with the
/// declaration winning by rank. With no daemon (`client` is `None`) the
/// manifest alone is read, and a note says the `init.lua` entries are not
/// shown.
pub(crate) async fn configured_plugin_entries(
    client: Option<&crucible_daemon::DaemonClient>,
) -> Result<(Vec<GitPlugin>, Vec<String>)> {
    let mut notes = Vec::new();

    if let Some(client) = client {
        let rows = client.plugin_list_spec().await?;
        let entries = rows
            .into_iter()
            .filter_map(|row| {
                let crucible_core::config::SpecSource::Git { url, branch, pin } = row.entry.source
                else {
                    return None;
                };
                Some(GitPlugin {
                    name: row.entry.name,
                    url,
                    branch,
                    pin,
                    enabled: row.entry.enabled.unwrap_or(true),
                    source: if row.declared {
                        EntrySource::Declared
                    } else {
                        EntrySource::Installed
                    },
                })
            })
            .collect();
        return Ok((entries, notes));
    }

    notes.push(
        "daemon not running: entries declared in init.lua are not shown, and an entry there \
         that disables an installed plugin is not applied"
            .to_string(),
    );
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
    let entries = installed
        .into_iter()
        .map(|(name, entry)| GitPlugin {
            name,
            url: entry.url,
            branch: entry.branch,
            pin: entry.pin,
            enabled: entry.enabled,
            source: EntrySource::Installed,
        })
        .collect();
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
    /// List git-hosted plugins and their clone status
    List(ListArgs),
    /// Remove an installed plugin
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
    /// Skip typechecking the plugin's test suite.
    ///
    /// The suite IS checked by default. It used to be excluded because a suite
    /// monkey-patches the host on purpose and every stub read as a type error —
    /// but excluding it hid 46 real diagnostics in the shipped suites, among
    /// them 45 calls that passed a multi-return expression as the last
    /// argument and silently filled the next parameter with a match position.
    ///
    /// `mock(...)` marks the monkey-patch instead, so the boundary is one
    /// greppable word and everything around it is still checked. Use this flag
    /// for a suite that has not been through that yet. Tests are always
    /// parse-checked either way.
    #[arg(long)]
    pub skip_tests: bool,
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
