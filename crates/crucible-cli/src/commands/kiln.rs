//! `cru kiln` — naming directories, and seeing who owns each name.
//!
//! Thin by design: the daemon owns the rule, the floor and the files. This
//! module parses, calls and prints.
//!
//! Every subcommand goes through the daemon because **a registration is state,
//! not config**. `<data_home>/kilns.json` has one writer, and
//! `DaemonClient::connect_or_start` guarantees that writer exists. The old line
//! telling the user to run `cru daemon restart` after registering is gone with
//! the old writer: the daemon that writes the entry serves it at once.

use anyhow::{Context, Result};

use crate::cli::KilnCommands;

pub async fn handle(cmd: KilnCommands) -> Result<()> {
    match cmd {
        KilnCommands::Register {
            name,
            path,
            make_default,
        } => {
            let client = crate::common::daemon_client().await?;
            let response = client
                .kiln_register(&name, &path, /* auto */ false, make_default)
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
            if make_default {
                println!("  It is now the kiln used when none is named.");
            }
            println!("\nAttach it with `cru acp --kiln {name}`.");
            Ok(())
        }

        KilnCommands::List => {
            let client = crate::common::daemon_client().await?;
            let response = client
                .kiln_registry_list()
                .await
                .context("listing the kiln registry")?;

            let rows = response["kilns"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                println!("No kilns. Name one with `cru kiln register <name> <path>`.");
                return Ok(());
            }
            print!("{}", render_registry_table(&rows));
            if let Some(file) = response["state_file"].as_str() {
                println!("\nRegistrations are in {file}.");
            }
            Ok(())
        }

        KilnCommands::Forget { name } => {
            let client = crate::common::daemon_client().await?;
            let response = client
                .kiln_forget(&name)
                .await
                .with_context(|| format!("forgetting kiln '{name}'"))?;

            println!("Forgot kiln '{name}'.");
            if let Some(file) = response["state_file"].as_str() {
                println!("  in {file}");
            }
            // A running daemon still answers to the name: removing one changes
            // what an already-persisted session reference means, so it waits
            // for the freeze that protects that.
            println!(
                "\nThe running daemon still resolves '{name}'. The removal takes effect at the \
                 next daemon start."
            );
            Ok(())
        }
    }
}

/// Render the registry listing as the table the user reads.
///
/// A free function over the reply rather than inline printing, so each of the
/// six situations the daemon reports has a test of its own. The situations are
/// the point of the command: two layers hold kiln names, and the user can only
/// act on a conflict they can see.
fn render_registry_table(rows: &[serde_json::Value]) -> String {
    let cells: Vec<(String, String, String)> = rows
        .iter()
        .map(|row| {
            let marker = if row["default"].as_bool().unwrap_or(false) {
                "*"
            } else {
                " "
            };
            let name = format!("{marker}{}", row["name"].as_str().unwrap_or_default());
            let path = row["path"].as_str().unwrap_or_default().to_string();
            (name, path, describe_origin(row))
        })
        .collect();

    let name_width = cells.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);
    let path_width = cells.iter().map(|(_, p, _)| p.len()).max().unwrap_or(0);

    let mut out = String::new();
    for (name, path, origin) in cells {
        // `trim_end` so a row with no qualifier carries no trailing spaces —
        // the difference is invisible on screen and load-bearing in a snapshot.
        out.push_str(
            format!("{name:<name_width$}  {path:<path_width$}  {origin}")
                .trim_end()
                .as_ref(),
        );
        out.push('\n');
    }
    out
}

/// The origin column: which layer owns the name, and what is unusual about it.
fn describe_origin(row: &serde_json::Value) -> String {
    let origin = row["origin"].as_str().unwrap_or("unknown");
    let mut qualifiers: Vec<String> = Vec::new();

    // Both layers, same directory: one registration written down twice.
    if row["also_registered"].as_bool().unwrap_or(false) {
        qualifiers.push("also registered".to_string());
    }
    // Both layers, different directories: the config wins and the state entry
    // does nothing. Naming the shadowed path is what makes `forget` actionable.
    if let Some(shadowed) = row["shadows"].as_str() {
        qualifiers.push(format!("shadows registered {shadowed}"));
    }
    if row["missing"].as_bool().unwrap_or(false) {
        qualifiers.push("missing".to_string());
    }
    if row["lazy"].as_bool().unwrap_or(false) {
        qualifiers.push("lazy".to_string());
    }

    if qualifiers.is_empty() {
        origin.to_string()
    } else {
        format!("{origin} ({})", qualifiers.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// One row per situation the daemon reports, and one test per row: this is
    /// the table the split ownership model is only tolerable because of.
    fn row(name: &str, path: &str, origin: &str) -> serde_json::Value {
        json!({
            "name": name,
            "path": path,
            "origin": origin,
            "default": false,
            "missing": false,
            "lazy": false,
            "also_registered": false,
            "shadows": serde_json::Value::Null,
        })
    }

    fn rendered(row: serde_json::Value) -> String {
        render_registry_table(&[row]).trim_end().to_string()
    }

    #[test]
    fn a_config_declared_kiln_reads_as_config() {
        assert_eq!(
            rendered(row("notes", "/a/notes", "config")),
            " notes  /a/notes  config"
        );
    }

    #[test]
    fn a_state_only_kiln_reads_as_registered() {
        assert_eq!(
            rendered(row("notes", "/a/notes", "registered")),
            " notes  /a/notes  registered"
        );
    }

    #[test]
    fn a_name_both_layers_agree_on_says_so() {
        let mut r = row("notes", "/a/notes", "config");
        r["also_registered"] = json!(true);
        assert_eq!(rendered(r), " notes  /a/notes  config (also registered)");
    }

    /// The conflict case. The config wins, and the row names the shadowed path
    /// so the user can decide whether to forget it.
    #[test]
    fn a_shadowed_registration_names_the_path_it_lost_to() {
        let mut r = row("notes", "/a/notes", "config");
        r["shadows"] = json!("/b/notes");
        assert_eq!(
            rendered(r),
            " notes  /a/notes  config (shadows registered /b/notes)"
        );
    }

    /// A directory this daemon opened carries a name it derived itself, and
    /// nothing wrote that name down. Listing it says so, because the name
    /// stops working at the next daemon start unless a session attaches it.
    #[test]
    fn an_opened_but_unregistered_directory_reads_as_discovered() {
        assert_eq!(
            rendered(row("scratch", "/p/kiln", "discovered")),
            " scratch  /p/kiln  discovered"
        );
    }

    #[test]
    fn a_registration_whose_directory_is_gone_is_marked_missing() {
        let mut r = row("notes", "/a/notes", "registered");
        r["missing"] = json!(true);
        assert_eq!(rendered(r), " notes  /a/notes  registered (missing)");
    }

    /// The default marker is a column, not a qualifier: it answers "which kiln
    /// do I get when I name none", which is a different question from "who owns
    /// this name".
    #[test]
    fn the_default_kiln_is_marked() {
        let mut r = row("notes", "/a/notes", "config");
        r["default"] = json!(true);
        assert!(
            rendered(r).starts_with("*notes"),
            "the default marker must lead the row"
        );
    }

    /// Columns line up across rows, or the table is a list of strings.
    #[test]
    fn the_columns_are_padded_to_the_widest_row() {
        let table = render_registry_table(&[
            row("a", "/short", "config"),
            row("longer-name", "/a/much/longer/path", "registered"),
        ]);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0].find("/short"),
            lines[1].find("/a/much/longer/path"),
            "the path column must start at one offset: {table}"
        );
    }
}
