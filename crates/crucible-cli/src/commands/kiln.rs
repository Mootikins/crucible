//! `cru kiln` — giving a directory a name.
//!
//! Thin by design: [`CliKilnRegistry`] owns the rule and the floor, and this
//! module is only the shell around it — parse, call, print. The one thing it
//! adds is the line about restarting the daemon, which is a real consequence of
//! the registry being built once, from the config the daemon was handed at
//! bind (`Server::bind_with_plugin_config`). A daemon that is already running
//! has not read the entry we just wrote, and its refusal — "Unknown kiln
//! 'notes'" — reads like the registration failed when it did not.

use anyhow::Result;
use std::path::PathBuf;

use crate::cli::KilnCommands;
use crate::config::CliConfig;
use crate::kiln_attach::CliKilnRegistry;

pub fn handle(cmd: KilnCommands, config: CliConfig, config_path: Option<PathBuf>) -> Result<()> {
    match cmd {
        KilnCommands::Register { name, path } => {
            // The same file this process loaded, so what we write is what the
            // next run reads. `-C` has to be honoured here or `cru -C x kiln
            // register` writes the default config instead of `x`.
            let config_path = config_path
                .unwrap_or_else(crucible_core::config::CliAppConfig::default_config_path);
            let mut registry = CliKilnRegistry::for_cli(&config, config_path.clone())?;
            let attached = registry.register(&name, &path)?;

            println!(
                "Registered kiln '{}' at {}",
                attached.name,
                attached.path.display()
            );
            println!("  in {}", config_path.display());
            println!(
                "\nAttach it with `cru acp --kiln {}`. A daemon that is already running read \
                 its kilns at startup, so run `cru daemon restart` before it will answer to \
                 the new name.",
                attached.name
            );
            Ok(())
        }
    }
}
