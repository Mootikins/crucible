//! Reflection-pass proposal review commands.
//!
//! Proposals are markdown files the reflection pass stages in
//! `KILN/.crucible/proposals/`, deliberately outside the indexed kiln so
//! unreviewed suggestions never surface in search or precognition. These
//! commands are the human disposition surface: list, show, accept, reject.
//!
//! Accepting moves the file into the kiln (stripping provenance frontmatter)
//! so the daemon's file watcher indexes it. Rejecting deletes it. Neither
//! needs daemon RPC — the staging area is plain files under the kiln the CLI
//! already knows.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::cli::ProposalsCommands;
use crate::config::CliConfig;
use crate::formatting::OutputFormat;

/// Frontmatter keys the reflection pass adds for provenance. They are dropped
/// when a proposal is accepted so the promoted note is clean.
const PROVENANCE_KEYS: &[&str] = &["source", "status", "session", "created"];

#[derive(Debug, Serialize)]
struct ProposalSummary {
    id: String,
    title: String,
    created: Option<String>,
    session: Option<String>,
}

pub async fn execute(config: CliConfig, command: ProposalsCommands) -> Result<()> {
    match command {
        ProposalsCommands::List { format } => list(&config, &format),
        ProposalsCommands::Show { id } => show(&config, &id),
        ProposalsCommands::Accept { id } => accept(&config, &id),
        ProposalsCommands::Reject { id } => reject(&config, &id),
    }
}

/// `KILN/.crucible/proposals/`
fn proposals_dir(config: &CliConfig) -> PathBuf {
    config.kiln_path.join(".crucible").join("proposals")
}

/// Resolve a proposal id to its file, erroring if it does not exist. The id is
/// the file stem; `.md` is assumed. Reject ids containing path separators so a
/// caller cannot escape the staging directory.
fn proposal_path(config: &CliConfig, id: &str) -> Result<PathBuf> {
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        bail!("invalid proposal id: {id}");
    }
    let path = proposals_dir(config).join(format!("{id}.md"));
    if !path.is_file() {
        bail!("proposal not found: {id}");
    }
    Ok(path)
}

fn collect_proposals(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading proposals dir {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();
    Ok(files)
}

fn summarize(path: &Path) -> ProposalSummary {
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let fm = crucible_core::parser::extract_frontmatter(&content)
        .ok()
        .and_then(|r| r.frontmatter);

    let title = fm
        .as_ref()
        .and_then(|f| f.get_string("title"))
        .unwrap_or_else(|| id.clone());
    let created = fm.as_ref().and_then(|f| f.get_string("created"));
    let session = fm.as_ref().and_then(|f| f.get_string("session"));

    ProposalSummary {
        id,
        title,
        created,
        session,
    }
}

fn list(config: &CliConfig, format: &str) -> Result<()> {
    let dir = proposals_dir(config);
    let files = collect_proposals(&dir)?;

    if files.is_empty() {
        println!("No pending proposals.");
        println!("\nThe reflection pass stages proposals in:");
        println!("  {}", dir.display());
        return Ok(());
    }

    let summaries: Vec<ProposalSummary> = files.iter().map(|p| summarize(p)).collect();

    match OutputFormat::from(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&summaries)?),
        _ => {
            println!("{} pending proposal(s):\n", summaries.len());
            for s in summaries {
                println!("  {} — {}", s.id, s.title);
                if let Some(created) = s.created {
                    println!("    created: {created}");
                }
                if let Some(session) = s.session {
                    println!("    session: {session}");
                }
                println!();
            }
            println!("Review with `cru proposals show <id>`, then accept or reject.");
        }
    }
    Ok(())
}

fn show(config: &CliConfig, id: &str) -> Result<()> {
    let path = proposal_path(config, id)?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading proposal {}", path.display()))?;
    println!("{content}");
    Ok(())
}

fn accept(config: &CliConfig, id: &str) -> Result<()> {
    let path = proposal_path(config, id)?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading proposal {}", path.display()))?;

    let result = crucible_core::parser::extract_frontmatter(&content)
        .with_context(|| format!("parsing proposal {id}"))?;

    // Where the promoted note lands: an explicit `target` (relative to the
    // kiln) or the kiln root under the proposal id.
    let target_rel = result
        .frontmatter
        .as_ref()
        .and_then(|f| f.get_string("target"))
        .unwrap_or_else(|| format!("{id}.md"));
    if target_rel.contains("..") {
        bail!("proposal target escapes the kiln: {target_rel}");
    }
    let dest = config.kiln_path.join(&target_rel);
    if dest.exists() {
        bail!(
            "refusing to overwrite existing note: {} (edit the proposal's `target` or move the note aside)",
            dest.display()
        );
    }

    let promoted = strip_provenance(result.frontmatter.as_ref(), &result.body);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(&dest, promoted).with_context(|| format!("writing {}", dest.display()))?;
    std::fs::remove_file(&path).with_context(|| format!("removing proposal {}", path.display()))?;

    println!("Accepted proposal '{id}' -> {}", dest.display());
    println!("The daemon will index it on its next scan.");
    Ok(())
}

fn reject(config: &CliConfig, id: &str) -> Result<()> {
    let path = proposal_path(config, id)?;
    std::fs::remove_file(&path).with_context(|| format!("removing proposal {}", path.display()))?;
    println!("Rejected proposal '{id}'.");
    Ok(())
}

/// Re-render a note without the reflection provenance keys, preserving the
/// user's remaining frontmatter verbatim (order and formatting intact). Works
/// line-wise on the raw YAML: a top-level provenance key drops its line and any
/// indented continuation lines belonging to it. If no frontmatter remains, the
/// body is returned alone.
fn strip_provenance(
    frontmatter: Option<&crucible_core::parser::Frontmatter>,
    body: &str,
) -> String {
    let Some(fm) = frontmatter else {
        return body.to_string();
    };

    let mut kept_lines: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in fm.raw.lines() {
        let is_top_level_key = line
            .chars()
            .next()
            .is_some_and(|c| !c.is_whitespace() && c != '#')
            && line.contains(':');

        if is_top_level_key {
            let key = line.split(':').next().unwrap_or("").trim();
            skipping = PROVENANCE_KEYS.contains(&key);
        } else if skipping {
            // Indented/continuation line under a provenance key: keep skipping.
            // A blank line ends the skipped block.
            if line.trim().is_empty() {
                skipping = false;
            }
            continue;
        }

        if !skipping {
            kept_lines.push(line);
        }
    }

    if kept_lines.iter().all(|l| l.trim().is_empty()) {
        return body.to_string();
    }

    let yaml = kept_lines.join("\n");
    format!("---\n{yaml}\n---\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(kiln: &Path) -> CliConfig {
        crate::config::CliConfigBuilder::new()
            .kiln_path(kiln)
            .build()
            .unwrap()
    }

    fn write_proposal(dir: &Path, id: &str, content: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(format!("{id}.md")), content).unwrap();
    }

    #[test]
    fn accept_moves_note_into_kiln_and_strips_provenance() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        write_proposal(
            &proposals_dir(&config),
            "insight-1",
            "---\nsource: reflection\nstatus: proposed\nsession: \"[[s]]\"\ncreated: 2026-07-02\ntitle: Insight\ntags:\n  - learned\n---\n# Insight\n\nBody here.\n",
        );

        accept(&config, "insight-1").unwrap();

        let dest = kiln.join("insight-1.md");
        assert!(dest.is_file(), "note should be promoted to kiln root");
        assert!(
            !proposals_dir(&config).join("insight-1.md").exists(),
            "proposal should be removed after accept"
        );

        let promoted = std::fs::read_to_string(&dest).unwrap();
        assert!(!promoted.contains("status:"), "provenance stripped");
        assert!(!promoted.contains("source:"), "provenance stripped");
        assert!(!promoted.contains("session:"), "provenance stripped");
        assert!(promoted.contains("title: Insight"), "user fields kept");
        assert!(promoted.contains("- learned"), "tags kept");
        assert!(promoted.contains("# Insight"), "body kept");
    }

    #[test]
    fn accept_respects_target_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        write_proposal(
            &proposals_dir(&config),
            "p2",
            "---\nsource: reflection\nstatus: proposed\ntarget: Notes/Nested/thing.md\n---\nBody\n",
        );

        accept(&config, "p2").unwrap();

        assert!(kiln.join("Notes/Nested/thing.md").is_file());
    }

    #[test]
    fn accept_refuses_to_overwrite_existing_note() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        std::fs::write(kiln.join("dup.md"), "existing").unwrap();
        write_proposal(
            &proposals_dir(&config),
            "dup",
            "---\nstatus: proposed\n---\nnew body\n",
        );

        let err = accept(&config, "dup").unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"));
        // Proposal is left in place for the user to resolve.
        assert!(proposals_dir(&config).join("dup.md").exists());
    }

    #[test]
    fn reject_deletes_proposal() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        write_proposal(
            &proposals_dir(&config),
            "junk",
            "---\nstatus: proposed\n---\nnope\n",
        );

        reject(&config, "junk").unwrap();
        assert!(!proposals_dir(&config).join("junk.md").exists());
    }

    #[test]
    fn missing_proposal_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(tmp.path());
        assert!(proposal_path(&config, "ghost").is_err());
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(tmp.path());
        assert!(proposal_path(&config, "../secret").is_err());
    }

    #[test]
    fn list_reports_empty_without_error() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(tmp.path());
        list(&config, "table").unwrap();
    }
}
