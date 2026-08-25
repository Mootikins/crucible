use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

use crate::cli::ConfigCommands;
use crate::config::CliConfig;
use crate::output;

/// Execute config subcommand
///
/// `config` takes the already-resolved config rather than re-loading it: the
/// resolution in `async_main` is what honours `--config/-C`, the embedding
/// overrides, and the first-run wizard's rewrite.
pub async fn execute(
    config: CliConfig,
    cmd: ConfigCommands,
    config_path_flag: Option<PathBuf>,
) -> Result<()> {
    match cmd {
        ConfigCommands::Init { path, force } => init(path, force).await,
        ConfigCommands::Show { format, sources } => {
            println!("{}", render(&config, &format, sources)?);
            Ok(())
        }
        ConfigCommands::Migrate => migrate::run(config_path_flag),
        ConfigCommands::Dump { format } => dump(format).await,
    }
}

mod migrate;

/// The example `init.lua` that `cru config init` writes. Every line is a
/// comment except the empty `cru.config.set` call, so the file evaluates to
/// pure defaults until the user uncomments something.
const EXAMPLE_INIT_LUA: &str = r#"-- Crucible configuration.
-- The daemon evaluates this file once at boot; `cru config show` renders
-- the effective result. Values deep-merge over the defaults.

cru.config.set({
    -- Named kilns, and which one commands use by default:
    -- kilns = { notes = "~/vault/notes" },
    -- default_kiln = "notes",

    -- Chat settings:
    -- chat = { model = "claude-sonnet-4-5", show_thinking = true },

    -- Extra plugin/theme/skill directories:
    -- runtimepath = { "~/crucible-runtime" },

    -- Per-plugin configuration:
    -- plugins = { reflection = { min_turns = 4 } },
})

-- Plugins can also be configured directly:
-- require("reflection").setup({ min_turns = 4 })
"#;

/// Initialize a new config file: an example `init.lua`.
async fn init(path: Option<PathBuf>, force: bool) -> Result<()> {
    let config_path = path.unwrap_or_else(|| {
        let toml_path = CliConfig::default_config_path();
        toml_path
            .parent()
            .map(|dir| dir.join("init.lua"))
            .unwrap_or_else(|| PathBuf::from("init.lua"))
    });

    // Check if file already exists
    if config_path.exists() && !force {
        output::warning(&format!(
            "Config file already exists at: {}",
            config_path.display()
        ));
        println!(
            "  {} Try: `cru config init --force` to overwrite",
            "→".cyan()
        );
        return Ok(());
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, EXAMPLE_INIT_LUA)?;

    println!(
        "{} Created config file at: {}",
        "Success:".green().bold(),
        config_path.display()
    );
    println!(
        "\n{}",
        "Edit this file to configure your kiln settings.".dimmed()
    );
    println!(
        "{}",
        "Default values will be used until you customize the config.".dimmed()
    );

    Ok(())
}

/// Render the effective configuration for `config show`
fn render(config: &CliConfig, format: &str, sources: bool) -> Result<String> {
    Ok(match (format, sources) {
        ("json", true) => config.display_as_json_with_sources()?,
        ("json", false) => config.display_as_json()?,
        (_, true) => config.display_as_toml_with_sources()?,
        (_, false) => config.display_as_toml()?,
    })
}

/// Dump default configuration to stdout
async fn dump(format: String) -> Result<()> {
    println!("{}", render(&CliConfig::default(), &format, false)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example must evaluate to pure defaults — a template the boot
    /// rejects would break the very first `cru` after `config init`.
    #[test]
    fn the_example_init_lua_evaluates_to_defaults() {
        let config = crucible_lua::evaluate_config_source(EXAMPLE_INIT_LUA)
            .expect("the example must evaluate");
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            serde_json::to_value(crucible_core::config::CliAppConfig::default()).unwrap(),
            "every value line in the example must be commented out"
        );
    }
}
