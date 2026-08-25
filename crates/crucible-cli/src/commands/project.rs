//! `cru project` — the project registry, over the daemon's existing RPCs.
//!
//! The counterpart to `cru kiln`. A project is where work OUTPUT goes; a kiln
//! is where knowledge goes. The daemon keeps them in separate files with
//! separate writers, so they get separate commands: `cru project forget` reads
//! better than a generic state tool, and the domain noun is what the user
//! thinks in.
//!
//! `project.register`, `project.unregister` and `project.list` already existed.
//! Nothing new is written here — this is the surface that was missing.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::cli::ProjectCommands;

pub async fn handle(cmd: ProjectCommands) -> Result<()> {
    let client = crate::common::daemon_client().await?;
    match cmd {
        ProjectCommands::List => {
            let response = client
                .project_registry_list()
                .await
                .context("listing the project registry")?;
            let rows = response["projects"].as_array().cloned().unwrap_or_default();
            if rows.is_empty() {
                println!("No projects. Register one with `cru project register [path]`.");
                return Ok(());
            }
            print!("{}", render_project_table(&rows));

            // The conflict case, said once and named. A project both layers
            // claim for different directories is invisible from either side
            // alone, which is the whole reason the listing reports the layer.
            for shadowed in response["shadowed"].as_array().into_iter().flatten() {
                println!(
                    "\nNote: your config declares '{}' at {}, so the registered {} is not used.",
                    shadowed["name"].as_str().unwrap_or_default(),
                    shadowed["config_path"].as_str().unwrap_or_default(),
                    shadowed["registered_path"].as_str().unwrap_or_default(),
                );
            }
            Ok(())
        }

        ProjectCommands::Register { path } => {
            // The working directory when none is named: that is the directory
            // the user is standing in, and the daemon resolves it to the git
            // root itself.
            let path = match path {
                Some(path) => path,
                None => std::env::current_dir().context("reading the working directory")?,
            };
            let project = client
                .project_register(&path)
                .await
                .with_context(|| format!("registering project at {}", path.display()))?;
            println!(
                "Registered project '{}' at {}",
                project.name,
                project.path.display()
            );
            Ok(())
        }

        ProjectCommands::Forget { path } => {
            let path = absolutize(path)?;
            client
                .project_unregister(&path)
                .await
                .with_context(|| format!("forgetting project at {}", path.display()))?;
            println!("Forgot project at {}", path.display());
            println!("The directory itself is untouched.");
            Ok(())
        }
    }
}

/// A relative path means something different from every working directory, and
/// the registry stores absolute paths — so anchor it here rather than sending
/// the daemon a path that resolves against ITS working directory.
fn absolutize(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    Ok(std::env::current_dir()
        .context("reading the working directory")?
        .join(path))
}

/// Render the project listing, one row per name, with the layer that owns it.
///
/// A free function over the reply so each situation has a test of its own —
/// the same reason `cru kiln list` has one. The situations are the point: two
/// layers hold project names and the user can only act on a conflict they can
/// see.
fn render_project_table(rows: &[serde_json::Value]) -> String {
    let cells: Vec<(String, String, String)> = rows
        .iter()
        .map(|row| {
            let name = row["name"].as_str().unwrap_or_default().to_string();
            let path = row["path"].as_str().unwrap_or_default().to_string();
            let origin = row["origin"].as_str().unwrap_or("unknown").to_string();
            let kilns = row["kilns"].as_array().map(Vec::len).unwrap_or(0);
            let origin = if kilns > 0 {
                format!(
                    "{origin} ({kilns} kiln{})",
                    if kilns == 1 { "" } else { "s" }
                )
            } else {
                origin
            };
            (name, path, origin)
        })
        .collect();

    let name_width = cells.iter().map(|(n, _, _)| n.len()).max().unwrap_or(0);
    let path_width = cells.iter().map(|(_, p, _)| p.len()).max().unwrap_or(0);

    let mut out = String::new();
    for (name, path, origin) in cells {
        out.push_str(
            format!("{name:<name_width$}  {path:<path_width$}  {origin}")
                .trim_end()
                .as_ref(),
        );
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(name: &str, path: &str, origin: &str, kilns: usize) -> serde_json::Value {
        json!({
            "name": name,
            "path": path,
            "origin": origin,
            "kilns": vec![json!({"path": "k"}); kilns],
        })
    }

    fn rendered(row: serde_json::Value) -> String {
        render_project_table(&[row]).trim_end().to_string()
    }

    /// A project the daemon registered. The ordinary case after `cru init`.
    #[test]
    fn a_registered_project_reads_as_registered() {
        assert_eq!(
            rendered(row("repo", "/w/repo", "registered", 0)),
            "repo  /w/repo  registered"
        );
    }

    /// A `[projects.*]` entry the daemon has no registration for. It used to be
    /// invisible: `cru project list` showed only the daemon's registry, while
    /// `cru chat` matched against the config. A row is what makes the
    /// difference actionable.
    #[test]
    fn a_config_declared_project_gets_a_row_of_its_own() {
        assert_eq!(
            rendered(row("legacy", "/w/legacy", "config", 0)),
            "legacy  /w/legacy  config"
        );
    }

    /// The kilns a project carries are what `cru chat` opens on entry, so the
    /// count belongs in the listing: a project with none opens none.
    #[test]
    fn the_kiln_count_is_shown_when_there_is_one() {
        assert_eq!(
            rendered(row("repo", "/w/repo", "registered", 1)),
            "repo  /w/repo  registered (1 kiln)"
        );
        assert_eq!(
            rendered(row("repo", "/w/repo", "registered", 3)),
            "repo  /w/repo  registered (3 kilns)"
        );
    }

    #[test]
    fn the_columns_are_padded_to_the_widest_row() {
        let table = render_project_table(&[
            row("a", "/short", "registered", 0),
            row("longer-name", "/a/much/longer/path", "config", 0),
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
