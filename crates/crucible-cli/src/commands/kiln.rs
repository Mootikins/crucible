//! `cru kiln` — giving a directory a name.
//!
//! Thin by design: the daemon owns the rule, the floor and the file. This
//! module is only the shell around the RPC — parse, call, print.
//!
//! It calls the daemon rather than editing a file itself because **a
//! registration is state, not config**. `<data_home>/kilns.json` has one
//! writer, and `DaemonClient::connect_or_start` guarantees that writer exists.
//! The old line telling the user to run `cru daemon restart` is gone with the
//! old writer: the daemon that writes the entry serves it at once.

use anyhow::{Context, Result};

use crate::cli::KilnCommands;

pub async fn handle(cmd: KilnCommands) -> Result<()> {
    match cmd {
        KilnCommands::Register { name, path } => {
            let client = crate::common::daemon_client().await?;
            let response = client
                .kiln_register(
                    &name, &path, /* auto */ false, /* make_default */ false,
                )
                .await
                .with_context(|| format!("registering kiln '{name}'"))?;

            let name = response["name"].as_str().unwrap_or(&name);
            let path = response["path"].as_str().unwrap_or_default();
            match response["outcome"].as_str() {
                Some("already_present") => {
                    println!("Kiln '{name}' is already registered at {path}");
                }
                _ => println!("Registered kiln '{name}' at {path}"),
            }
            if let Some(file) = response["state_file"].as_str() {
                println!("  in {file}");
            }
            println!("\nAttach it with `cru acp --kiln {name}`.");
            Ok(())
        }
    }
}
