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
            let projects = client.project_list().await.context("listing projects")?;
            if projects.is_empty() {
                println!("No projects. Register one with `cru project register [path]`.");
                return Ok(());
            }
            let width = projects
                .iter()
                .map(|p| p.name.len())
                .max()
                .unwrap_or_default();
            for project in projects {
                println!("{:<width$}  {}", project.name, project.path.display());
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
