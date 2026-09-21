//! `cru diff`: print a diffset that the daemon computes.
//!
//! The daemon admits the root, finds the merge base and lists the files. This
//! module asks for the list, asks for the text of each file, and draws them
//! with the renderer that the TUI uses.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crucible_core::diff::{DiffFileText, Diffset, DiffsetSource};
use crucible_core::session::PhysicalRoot;
use crucible_daemon::DaemonClient;
use crucible_oil::node::{col, text, Node};
use crucible_oil::render::{render_to_plain_text, render_to_string};

use crate::cli::DiffCommands;
use crate::formatting::TextFormat;
use crate::tui::oil::components::diff_view::{render_diffset_file, DiffLayout, DiffOptions};

/// The width of the output when stdout is not a terminal.
const PIPE_WIDTH: usize = 100;

pub async fn handle(cmd: DiffCommands) -> Result<()> {
    match cmd {
        DiffCommands::Branch {
            base,
            head,
            root,
            stat,
            format,
        } => {
            let start = match root {
                Some(root) => std::path::absolute(root)?,
                None => std::env::current_dir()?,
            };
            let source = branch_source(&start, base.as_deref(), head);
            let client = crate::common::daemon_client().await?;
            let diffset = client
                .diff_get(&source)
                .await
                .with_context(|| format!("computing the branch diff of {}", start.display()))?;
            let texts = if stat {
                Vec::new()
            } else {
                file_texts(&client, &diffset).await?
            };
            match format {
                TextFormat::Json => {
                    let mut value = serde_json::json!({ "diffset": diffset });
                    if !stat {
                        value["texts"] = serde_json::to_value(&texts)?;
                    }
                    println!("{}", serde_json::to_string_pretty(&value)?);
                }
                TextFormat::Text => print_diffset(&diffset, &texts, stat),
            }
            Ok(())
        }
    }
}

/// The git top level at or above `start`, or `start` when none is found.
///
/// The daemon refuses a root below the top level, because git then lists
/// files outside the root. A user who runs the command in a subdirectory
/// thus means the repository that holds it. The daemon still admits the
/// root; this search gives no access.
pub(crate) fn repository_root(start: &Path) -> PathBuf {
    start
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .unwrap_or(start)
        .to_path_buf()
}

/// The branch source for the repository at or above `start`.
///
/// An absent base is empty on the wire, and the daemon then uses the
/// default branch of the repository.
pub(crate) fn branch_source(
    start: &Path,
    base: Option<&str>,
    head: Option<String>,
) -> DiffsetSource {
    DiffsetSource::Branch {
        root: PhysicalRoot::from_top_level(repository_root(start)),
        base: base.unwrap_or_default().to_string(),
        head,
    }
}

/// The two texts of each file that has text, in the order of the files.
///
/// A binary file and a file above the size limit have no text, so the
/// command does not ask for them.
async fn file_texts(client: &DaemonClient, diffset: &Diffset) -> Result<Vec<Option<DiffFileText>>> {
    let mut texts = Vec::with_capacity(diffset.files.len());
    for entry in &diffset.files {
        if entry.binary || entry.too_large {
            texts.push(None);
            continue;
        }
        let text = client
            .diff_file(&diffset.source, &entry.path, renamed_from(&entry.status))
            .await
            .with_context(|| format!("reading the texts of {}", entry.path))?;
        texts.push(Some(text));
    }
    Ok(texts)
}

/// The old path of a renamed file, which `diff.file` reads the base from.
pub(crate) fn renamed_from(status: &crucible_core::diff::FileStatus) -> Option<&str> {
    match status {
        crucible_core::diff::FileStatus::Renamed { from } => Some(from),
        crucible_core::diff::FileStatus::Added
        | crucible_core::diff::FileStatus::Modified
        | crucible_core::diff::FileStatus::Deleted => None,
    }
}

fn print_diffset(diffset: &Diffset, texts: &[Option<DiffFileText>], stat: bool) {
    let terminal = std::io::stdout().is_terminal();
    let width = if terminal {
        crossterm::terminal::size().map_or(PIPE_WIDTH, |(w, _)| w as usize)
    } else {
        PIPE_WIDTH
    };
    let mut opts = DiffOptions::for_width(width);
    opts.max_lines = None;
    opts.collapsed = stat;
    if !terminal {
        // A pipe reads one column of lines, as `git diff` gives.
        opts.layout = Some(DiffLayout::Unified);
    }
    let node = diffset_view(diffset, texts, &opts);
    let out = if terminal {
        render_to_string(&node, width)
    } else {
        render_to_plain_text(&node, width)
    };
    println!("{}", out.trim_end());
}

/// The summary line and each file of the diffset.
fn diffset_view(diffset: &Diffset, texts: &[Option<DiffFileText>], opts: &DiffOptions) -> Node {
    let base = match &diffset.source {
        DiffsetSource::Branch { base, .. } => base.as_str(),
        DiffsetSource::SessionRecord { .. } | DiffsetSource::Proposal { .. } => "",
    };
    let count = diffset.files.len();
    let noun = if count == 1 { "file" } else { "files" };
    let mut rows = vec![text(format!("{count} {noun} changed since {base}"))];
    for (index, entry) in diffset.files.iter().enumerate() {
        if !opts.collapsed {
            rows.push(text(""));
        }
        let text = texts.get(index).and_then(Option::as_ref);
        rows.push(render_diffset_file(entry, text, opts));
    }
    col(rows)
}
