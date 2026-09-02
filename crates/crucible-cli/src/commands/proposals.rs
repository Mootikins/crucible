//! Reflection-pass proposal review commands.
//!
//! Proposals are markdown files the reflection pass stages in
//! `KILN/.crucible/proposals/`, deliberately outside the indexed kiln so
//! unreviewed suggestions never surface in search or precognition. These
//! commands are the human disposition surface: list, show, accept, reject.
//!
//! A proposal has a `kind`: `create` (the default) moves the file into the
//! kiln, `update` replaces the full body of an existing note, and `skill`
//! lands a `SKILL.md` under `.crucible/skills/`. Accepting strips the
//! provenance frontmatter so the daemon's file watcher indexes a clean note.
//! Rejecting moves the file into the `rejected/` directory, so the reflection
//! reviewer does not propose the same note again. Neither needs daemon RPC:
//! the staging area is plain files under the kiln the CLI already knows.
//!
//! This loop never edits an installed skill. An `update` may not target
//! anything under `.crucible/` or any file named `SKILL.md`, and a `skill`
//! proposal whose name is taken is refused.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::cli::ProposalsCommands;
use crate::config::CliConfig;
use crate::formatting::OutputFormat;

/// Frontmatter keys the reflection pass adds for provenance (and `target` and
/// `kind`, which only direct placement). All are dropped when a proposal is
/// accepted so the promoted note is clean.
const PROVENANCE_KEYS: &[&str] = &[
    "source", "status", "session", "created", "model", "kind", "target",
];

/// Where accepted skills land, relative to the kiln. The daemon's discovery
/// searches this directory (see `skills/discovery.rs`).
const KILN_SKILLS_DIR: &str = ".crucible/skills";

/// What a proposal does when a human accepts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProposalKind {
    /// Move the file into the kiln as a new note.
    Create,
    /// Replace the full body of an existing note named by `target`.
    Update,
    /// Land a `SKILL.md` under `.crucible/skills/<name>/`.
    Skill,
}

impl ProposalKind {
    fn from_frontmatter(fm: Option<&crucible_core::parser::Frontmatter>) -> Result<Self> {
        match fm.and_then(|f| f.get_string("kind")).as_deref() {
            None | Some("create") => Ok(Self::Create),
            Some("update") => Ok(Self::Update),
            Some("skill") => Ok(Self::Skill),
            Some(other) => bail!("unknown proposal kind: {other}"),
        }
    }
}

/// Where rejected proposals are kept. The reflection reviewer lists this
/// directory so it does not propose the same note twice. A human who moves a
/// file here by hand has rejected it just as well.
const REJECTED_DIR: &str = "rejected";

#[derive(Debug, Serialize)]
struct ProposalSummary {
    id: String,
    title: String,
    created: Option<String>,
    session: Option<String>,
}

pub async fn execute(config: CliConfig, command: ProposalsCommands) -> Result<()> {
    match command {
        ProposalsCommands::List { format } => list(&config, OutputFormat::for_stdout(format)),
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
        .filter(|p| p.is_file() && crucible_core::is_note_file(p))
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

fn list(config: &CliConfig, format: OutputFormat) -> Result<()> {
    let dir = proposals_dir(config);
    let files = collect_proposals(&dir)?;

    if files.is_empty() {
        println!("No pending proposals.");
        println!("\nThe reflection pass stages proposals in:");
        println!("  {}", dir.display());
        return Ok(());
    }

    let summaries: Vec<ProposalSummary> = files.iter().map(|p| summarize(p)).collect();

    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&summaries)?),
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = summaries
                .iter()
                .map(|s| {
                    vec![
                        s.id.clone(),
                        s.title.clone(),
                        s.created.clone().unwrap_or_default(),
                        s.session.clone().unwrap_or_default(),
                    ]
                })
                .collect();
            println!(
                "{}",
                crate::output::records_table(&["ID", "Title", "Created", "Session"], &rows)
            );
            println!("\nReview with `cru proposals show <id>`, then accept or reject.");
        }
        OutputFormat::Plain => {
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
    let fm = result.frontmatter.as_ref();
    let kiln = &config.kiln_path;

    let (dest, text) = match ProposalKind::from_frontmatter(fm)? {
        ProposalKind::Create => {
            // Where the promoted note lands: an explicit `target` (relative to
            // the kiln) or the kiln root under the proposal id.
            let target_rel = fm
                .and_then(|f| f.get_string("target"))
                .unwrap_or_else(|| format!("{id}.md"));
            let dest = resolve_target_within_kiln(kiln, &target_rel)?;
            if dest.exists() {
                bail!(
                    "refusing to overwrite existing note: {} (edit the proposal's `target` or move the note aside)",
                    dest.display()
                );
            }
            (dest, strip_provenance(fm, &result.body))
        }
        ProposalKind::Update => {
            let target_rel = fm
                .and_then(|f| f.get_string("target"))
                .ok_or_else(|| anyhow::anyhow!("an update proposal needs a target"))?;
            if is_skill_path(&target_rel) {
                bail!("an update may not target a skill: {target_rel}");
            }
            let dest = resolve_target_within_kiln(kiln, &target_rel)?;
            if !dest.is_file() {
                bail!("update target does not exist: {}", dest.display());
            }
            // The lexical check above cannot see a symlink. `fs::write` follows
            // one, so resolve the real file and apply both guards again.
            let real_rel = real_path_within_kiln(kiln, &dest, &target_rel)?;
            if is_skill_path(&real_rel) {
                bail!("an update may not target a skill: {target_rel} resolves to {real_rel}");
            }
            (dest, strip_provenance(fm, &result.body))
        }
        ProposalKind::Skill => {
            let fm = fm.ok_or_else(|| anyhow::anyhow!("a skill proposal needs frontmatter"))?;
            let text = render_skill(fm, &result.body)?;
            // `render_skill` validated the name, so it cannot leave the skills dir.
            let name = fm.get_string("name").unwrap_or_default();
            let dest =
                resolve_target_within_kiln(kiln, &format!("{KILN_SKILLS_DIR}/{name}/SKILL.md"))?;
            if dest.exists() {
                bail!("a skill named {name} already exists; pick a new name or edit it by hand");
            }
            (dest, text)
        }
    };

    std::fs::write(&dest, text).with_context(|| format!("writing {}", dest.display()))?;
    std::fs::remove_file(&path).with_context(|| format!("removing proposal {}", path.display()))?;

    println!("Accepted proposal '{id}' -> {}", dest.display());
    println!("The daemon will index it on its next scan.");
    Ok(())
}

/// The spec's `name` rule: 1-64 chars, `a-z`, `0-9` and `-`, no leading or
/// trailing hyphen, no `--`. The directory takes the same name, so this rule
/// also keeps the path inside `.crucible/skills/`.
fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// Render a spec-shaped `SKILL.md`. This is a whitelist, not a strip list:
/// only the six agentskills.io fields are written. Provenance goes under
/// `metadata`, the map the spec reserves for client data, with a `crucible-`
/// prefix so it cannot collide with another tool's key.
fn render_skill(fm: &crucible_core::parser::Frontmatter, body: &str) -> Result<String> {
    let name = fm
        .get_string("name")
        .ok_or_else(|| anyhow::anyhow!("a skill proposal needs a name"))?;
    if !valid_skill_name(&name) {
        bail!(
            "invalid skill name: {name} (1-64 chars, a-z 0-9 and single hyphens, none at either end)"
        );
    }
    let description = fm
        .get_string("description")
        .filter(|d| !d.is_empty() && d.len() <= 1024)
        .ok_or_else(|| {
            anyhow::anyhow!("a skill proposal needs a description of 1 to 1024 characters")
        })?;
    let mut out = String::from("---\n");
    out.push_str(&format!("name: {name}\n"));
    out.push_str(&format!("description: {}\n", yaml_string(&description)));
    for key in ["license", "compatibility", "allowed-tools"] {
        if let Some(v) = fm.get_string(key) {
            out.push_str(&format!("{key}: {}\n", yaml_string(&v)));
        }
    }
    out.push_str("metadata:\n  crucible-source: reflection\n");
    out.push_str("---\n");
    out.push_str(body.trim_start_matches('\n'));
    Ok(out)
}

/// Quote a scalar for one YAML line.
fn yaml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// True for a path an `update` proposal may never touch: anything under
/// `.crucible/` (skills and staging live there) and any `SKILL.md`. This
/// loop never edits an installed skill.
///
/// `Path::components` keeps a leading `.` as `CurDir`, so the check looks at
/// every component and not only the first one.
fn is_skill_path(target_rel: &str) -> bool {
    let p = Path::new(target_rel);
    p.components().any(|c| c.as_os_str() == ".crucible")
        || p.file_name().is_some_and(|n| n == "SKILL.md")
}

/// Canonicalize an existing `dest` and return its path relative to the
/// canonical kiln root. A symlink that leaves the kiln is refused here.
fn real_path_within_kiln(kiln: &Path, dest: &Path, target_rel: &str) -> Result<String> {
    let kiln_root = kiln
        .canonicalize()
        .with_context(|| format!("resolving kiln root {}", kiln.display()))?;
    let real = dest
        .canonicalize()
        .with_context(|| format!("resolving {}", dest.display()))?;
    let rel = real
        .strip_prefix(&kiln_root)
        .map_err(|_| anyhow::anyhow!("proposal target escapes the kiln: {target_rel}"))?;
    Ok(rel.to_string_lossy().into_owned())
}

/// Resolve a proposal's `target` to an absolute destination guaranteed to live
/// inside the kiln, creating its parent directory. Rejects absolute targets and
/// any `..` component lexically, then canonicalizes the created parent and
/// asserts it is under the (canonicalized) kiln root — so neither a crafted
/// path nor a symlink inside the kiln can escape it.
fn resolve_target_within_kiln(kiln: &Path, target_rel: &str) -> Result<PathBuf> {
    use std::path::Component;

    let target = Path::new(target_rel);
    for component in target.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                bail!("proposal target must be relative to the kiln: {target_rel}")
            }
            Component::ParentDir => {
                bail!("proposal target must not contain '..': {target_rel}")
            }
            _ => {}
        }
    }

    let dest = kiln.join(target);
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow::anyhow!("proposal target has no parent: {target_rel}"))?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;

    let kiln_root = kiln
        .canonicalize()
        .with_context(|| format!("resolving kiln root {}", kiln.display()))?;
    let parent_canon = parent
        .canonicalize()
        .with_context(|| format!("resolving {}", parent.display()))?;
    if !parent_canon.starts_with(&kiln_root) {
        bail!("proposal target escapes the kiln: {target_rel}");
    }

    Ok(dest)
}

fn reject(config: &CliConfig, id: &str) -> Result<()> {
    let path = proposal_path(config, id)?;
    let rejected_dir = proposals_dir(config).join(REJECTED_DIR);
    std::fs::create_dir_all(&rejected_dir)
        .with_context(|| format!("creating {}", rejected_dir.display()))?;
    let dest = rejected_dir.join(format!("{id}.md"));
    std::fs::rename(&path, &dest)
        .with_context(|| format!("moving proposal {} to {}", path.display(), dest.display()))?;
    println!("Rejected proposal '{id}' (kept in {})", dest.display());
    Ok(())
}

/// Re-render a note without the reflection provenance keys.
fn strip_provenance(
    frontmatter: Option<&crucible_core::parser::Frontmatter>,
    body: &str,
) -> String {
    strip_keys(frontmatter, body, PROVENANCE_KEYS)
}

/// Re-render a note without the given top-level keys, preserving the user's
/// remaining frontmatter verbatim (order and formatting intact). Works
/// line-wise on the raw YAML: a listed key drops its line and any indented
/// continuation lines belonging to it. If no frontmatter remains, the body is
/// returned alone.
fn strip_keys(
    frontmatter: Option<&crucible_core::parser::Frontmatter>,
    body: &str,
    keys: &[&str],
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
            skipping = keys.contains(&key);
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
    fn accept_strips_target_from_promoted_note() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        write_proposal(
            &proposals_dir(&config),
            "p-target",
            "---\nsource: reflection\nstatus: proposed\ntarget: Notes/x.md\ntitle: T\n---\nBody\n",
        );

        accept(&config, "p-target").unwrap();

        let promoted = std::fs::read_to_string(kiln.join("Notes/x.md")).unwrap();
        assert!(
            !promoted.contains("target:"),
            "target directs placement and must not survive into the note: {promoted:?}"
        );
        assert!(promoted.contains("title: T"), "user fields kept");
    }

    #[test]
    fn accept_rejects_absolute_target() {
        let tmp = tempfile::tempdir().unwrap();
        let escape = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        let abs = escape.path().join("evil.md");
        write_proposal(
            &proposals_dir(&config),
            "abs",
            &format!(
                "---\nstatus: proposed\ntarget: {}\n---\npwned\n",
                abs.display()
            ),
        );

        let err = accept(&config, "abs").unwrap_err();
        assert!(
            err.to_string().contains("relative to the kiln"),
            "got: {err}"
        );
        assert!(!abs.exists(), "must not write outside the kiln");
    }

    #[test]
    fn accept_rejects_relative_escape_target() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        write_proposal(
            &proposals_dir(&config),
            "esc",
            "---\nstatus: proposed\ntarget: ../../etc/x.md\n---\npwned\n",
        );

        let err = accept(&config, "esc").unwrap_err();
        assert!(err.to_string().contains("'..'"), "got: {err}");
        assert!(!kiln.parent().unwrap().join("etc/x.md").exists());
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
    fn reject_moves_the_proposal_into_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(tmp.path());
        write_proposal(
            &proposals_dir(&config),
            "dup-1",
            "---\nsource: reflection\nstatus: proposed\ntitle: Socket path rules\n---\nBody\n",
        );

        reject(&config, "dup-1").unwrap();

        assert!(!proposals_dir(&config).join("dup-1.md").exists());
        let kept = proposals_dir(&config).join("rejected").join("dup-1.md");
        assert!(kept.is_file(), "a rejected proposal is kept, not deleted");
        let text = std::fs::read_to_string(kept).unwrap();
        assert!(
            text.contains("title: Socket path rules"),
            "content is untouched"
        );
    }

    #[test]
    fn list_ignores_the_rejected_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(tmp.path());
        write_proposal(
            &proposals_dir(&config),
            "pending",
            "---\ntitle: P\n---\nB\n",
        );
        write_proposal(
            &proposals_dir(&config).join("rejected"),
            "old",
            "---\ntitle: Old\n---\nB\n",
        );

        let files = collect_proposals(&proposals_dir(&config)).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["pending.md"]);
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
        list(&config, OutputFormat::Table).unwrap();
    }

    #[test]
    fn accept_update_replaces_the_target_note() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        std::fs::create_dir_all(kiln.join("Notes")).unwrap();
        std::fs::write(
            kiln.join("Notes/sock.md"),
            "---\ntitle: Sock\n---\nold body\n",
        )
        .unwrap();
        write_proposal(
            &proposals_dir(&config),
            "u1",
            "---\nsource: reflection\nstatus: proposed\nkind: update\ntarget: Notes/sock.md\ntitle: Sock\n---\nnew body\n",
        );

        accept(&config, "u1").unwrap();

        let now = std::fs::read_to_string(kiln.join("Notes/sock.md")).unwrap();
        assert!(now.contains("new body"));
        assert!(!now.contains("old body"));
        assert!(!now.contains("kind:"), "kind is provenance: {now}");
        assert!(
            !proposals_dir(&config).join("u1.md").exists(),
            "the proposal is removed after accept"
        );
    }

    #[test]
    fn accept_update_refuses_a_missing_target() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(tmp.path());
        write_proposal(
            &proposals_dir(&config),
            "u2",
            "---\nkind: update\ntarget: Notes/none.md\n---\nbody\n",
        );
        let err = accept(&config, "u2").unwrap_err();
        assert!(err.to_string().contains("does not exist"), "got: {err}");
    }

    #[test]
    fn accept_skill_lands_a_skill_md_and_keeps_provenance() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        write_proposal(
            &proposals_dir(&config),
            "s1",
            "---\nsource: reflection\nstatus: proposed\nkind: skill\nname: fix-flaky-nextest\ndescription: How to isolate a flaky daemon test\nmodel: m1\nsession: \"[[chat-1]]\"\ncreated: 2026-09-01\n---\n# Steps\n\nRun with `-p`.\n",
        );

        accept(&config, "s1").unwrap();

        let dest = kiln.join(".crucible/skills/fix-flaky-nextest/SKILL.md");
        let text = std::fs::read_to_string(&dest).unwrap();
        assert!(text.contains("name: fix-flaky-nextest"));
        assert!(text.contains("description: \"How to isolate"));
        assert!(
            text.contains("  crucible-source: reflection"),
            "provenance lives under metadata: {text}"
        );
        for key in [
            "source:", "model:", "session:", "status:", "kind:", "created:", "target:", "title:",
        ] {
            assert!(
                !text.lines().any(|l| l.starts_with(key)),
                "{key} is not a spec field: {text}"
            );
        }
        assert!(text.contains("# Steps"));

        // The file must load through the daemon's own parser, or discovery
        // would refuse the skill the user just accepted.
        let parsed = crucible_daemon::skills::SkillParser::new()
            .parse(
                &text,
                crucible_daemon::skills::SkillSource {
                    agent: None,
                    scope: crucible_daemon::skills::SkillScope::Kiln,
                    path: dest.clone(),
                    content_hash: String::new(),
                },
            )
            .expect("accepted skill parses");
        assert_eq!(parsed.name, "fix-flaky-nextest");
        // The daemon flattens every non-spec key into `metadata`, so the
        // spec's own `metadata` map sits one level down.
        assert_eq!(
            parsed
                .metadata
                .get("metadata")
                .and_then(|m| m.get("crucible-source"))
                .and_then(|v| v.as_str()),
            Some("reflection")
        );
    }

    #[test]
    fn accept_skill_rejects_spec_invalid_names() {
        let long = "x".repeat(65);
        for bad in [
            "-lead",
            "trail-",
            "two--hyphens",
            "Upper",
            "has space",
            long.as_str(),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let config = test_config(tmp.path());
            write_proposal(
                &proposals_dir(&config),
                "bad",
                &format!("---\nkind: skill\nname: {bad}\ndescription: d\n---\nb\n"),
            );
            let err = accept(&config, "bad").unwrap_err();
            assert!(err.to_string().contains("skill name"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn accept_update_never_touches_a_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        let skill_dir = kiln.join(".crucible/skills/vendored");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: vendored\ndescription: d\n---\ntheirs\n",
        )
        .unwrap();
        write_proposal(
            &proposals_dir(&config),
            "u4",
            "---\nkind: update\ntarget: .crucible/skills/vendored/SKILL.md\ntitle: T\n---\nmine\n",
        );

        let err = accept(&config, "u4").unwrap_err();
        assert!(
            err.to_string().contains("may not target a skill"),
            "got: {err}"
        );
        let text = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(text.contains("theirs"), "the installed skill is untouched");
    }

    #[test]
    fn accept_skill_refuses_an_existing_name() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        let skill_dir = kiln.join(".crucible/skills/taken");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: taken\ndescription: d\n---\ntheirs\n",
        )
        .unwrap();
        write_proposal(
            &proposals_dir(&config),
            "s3",
            "---\nkind: skill\nname: taken\ndescription: d2\n---\nmine\n",
        );

        let err = accept(&config, "s3").unwrap_err();
        assert!(err.to_string().contains("exists"), "got: {err}");
        let text = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(text.contains("theirs"));
    }

    #[test]
    fn accept_skill_rejects_a_bad_name() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(tmp.path());
        write_proposal(
            &proposals_dir(&config),
            "s2",
            "---\nkind: skill\nname: ../Evil\ndescription: d\n---\nb\n",
        );
        let err = accept(&config, "s2").unwrap_err();
        assert!(err.to_string().contains("skill name"), "got: {err}");
    }

    #[test]
    fn accept_update_refuses_a_dot_slash_crucible_target() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        std::fs::create_dir_all(kiln.join(".crucible")).unwrap();
        std::fs::write(kiln.join(".crucible/kiln.toml"), "name = \"k\"\n").unwrap();
        write_proposal(
            &proposals_dir(&config),
            "u5",
            "---\nkind: update\ntarget: ./.crucible/kiln.toml\n---\nowned\n",
        );

        let err = accept(&config, "u5").unwrap_err();
        assert!(
            err.to_string().contains("may not target a skill"),
            "got: {err}"
        );
        let text = std::fs::read_to_string(kiln.join(".crucible/kiln.toml")).unwrap();
        assert_eq!(text, "name = \"k\"\n", "kiln.toml is untouched");
    }

    #[test]
    fn is_skill_path_ignores_a_leading_current_dir() {
        assert!(is_skill_path("./.crucible/kiln.toml"));
        assert!(is_skill_path("././.crucible/proposals/rejected/x.md"));
        assert!(is_skill_path("./Notes/SKILL.md"));
        assert!(!is_skill_path("./Notes/sock.md"));
    }

    #[cfg(unix)]
    #[test]
    fn accept_update_refuses_a_symlink_into_a_skill() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path();
        let config = test_config(kiln);
        let skill_dir = kiln.join(".crucible/skills/vendored");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: vendored\ndescription: d\n---\ntheirs\n",
        )
        .unwrap();
        std::fs::create_dir_all(kiln.join("Notes")).unwrap();
        std::os::unix::fs::symlink(skill_dir.join("SKILL.md"), kiln.join("Notes/link.md")).unwrap();
        write_proposal(
            &proposals_dir(&config),
            "u6",
            "---\nkind: update\ntarget: Notes/link.md\n---\nmine\n",
        );

        let err = accept(&config, "u6").unwrap_err();
        assert!(
            err.to_string().contains("may not target a skill"),
            "got: {err}"
        );
        let text = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(text.contains("theirs"), "the installed skill is untouched");
    }

    #[cfg(unix)]
    #[test]
    fn accept_update_refuses_a_symlink_that_leaves_the_kiln() {
        let tmp = tempfile::tempdir().unwrap();
        let kiln = tmp.path().join("kiln");
        let outside = tmp.path().join("outside.md");
        std::fs::create_dir_all(kiln.join("Notes")).unwrap();
        std::fs::write(&outside, "theirs\n").unwrap();
        std::os::unix::fs::symlink(&outside, kiln.join("Notes/link.md")).unwrap();
        let config = test_config(&kiln);
        write_proposal(
            &proposals_dir(&config),
            "u7",
            "---\nkind: update\ntarget: Notes/link.md\n---\nmine\n",
        );

        let err = accept(&config, "u7").unwrap_err();
        assert!(err.to_string().contains("escapes the kiln"), "got: {err}");
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "theirs\n");
    }
}
