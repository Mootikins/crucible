//! `cru proposal`: read the proposals and send the decision of the user.
//!
//! The daemon owns each proposal, its state and every write. This module
//! asks for a proposal, draws it, and sends accept, reject, dismiss or
//! resolve. The TUI `:proposals` view uses the same projection of a proposal
//! to a diffset, so the two frontends show one diff.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use crucible_core::diff::{DiffFileEntry, DiffFileText, Diffset, DiffsetSource, FileStatus};
use crucible_core::file_write::ExpectedBase;
use crucible_core::proposal::{FileConflict, Proposal, ProposalAuthor, ProposalState};
use crucible_core::types::acp::MAX_DIFF_BYTES;
use crucible_oil::node::{col, text, Node};

use super::diff::{print_node, stdout_diff_options};
use crate::cli::ProposalCommands;
use crate::formatting::TextFormat;
use crate::tui::oil::components::diff_view::{count_changes, render_diffset_file, DiffOptions};

/// The marker line above the proposed side of a conflict.
pub(crate) const OURS_MARKER: &str = "<<<<<<< proposal";
/// The marker line between the two sides of a conflict.
pub(crate) const SPLIT_MARKER: &str = "=======";
/// The marker line below the disk side of a conflict.
pub(crate) const THEIRS_MARKER: &str = ">>>>>>> disk";

pub async fn handle(cmd: ProposalCommands) -> Result<()> {
    let client = crate::common::daemon_client().await?;
    match cmd {
        ProposalCommands::List { all, format } => {
            let proposals = client
                .proposal_list(all)
                .await
                .context("listing the proposals")?;
            match format {
                TextFormat::Json => println!("{}", serde_json::to_string_pretty(&proposals)?),
                TextFormat::Text => print!("{}", list_text(&proposals)),
            }
        }
        ProposalCommands::Show {
            id,
            conflict,
            format,
        } => {
            let proposal = client
                .proposal_get(&id)
                .await
                .with_context(|| format!("reading the proposal {id}"))?;
            match (conflict, format) {
                (Some(path), _) => print!("{}", conflict_text_of(&proposal, &path)?),
                (None, TextFormat::Json) => {
                    println!("{}", serde_json::to_string_pretty(&proposal)?)
                }
                (None, TextFormat::Text) => {
                    print_node(&show_view(&proposal, &stdout_diff_options()))
                }
            }
        }
        ProposalCommands::Accept { id } => {
            let proposal = client
                .proposal_accept(&id)
                .await
                .with_context(|| format!("accepting the proposal {id}"))?;
            if let ProposalState::Conflicted { files } = &proposal.state {
                anyhow::bail!(
                    "{} with the disk in the proposal {id}, so the daemon wrote no file. \
                     Run `cru proposal show {id}`, then `cru proposal resolve` for each file",
                    count_noun(files.len(), "file conflicts", "files conflict")
                );
            }
            println!("{}", decision_line(&proposal));
        }
        ProposalCommands::Reject { id, reason } => {
            let proposal = client
                .proposal_reject(&id, reason.as_deref())
                .await
                .with_context(|| format!("rejecting the proposal {id}"))?;
            println!("{}", decision_line(&proposal));
        }
        ProposalCommands::Dismiss { id } => {
            let proposal = client
                .proposal_dismiss(&id)
                .await
                .with_context(|| format!("dismissing the proposal {id}"))?;
            println!("{}", decision_line(&proposal));
        }
        ProposalCommands::Resolve { id, path, from } => {
            let settled = read_settled_text(&from)?;
            let proposal = client
                .proposal_resolve(&id, &path, &settled)
                .await
                .with_context(|| format!("resolving {path} of the proposal {id}"))?;
            println!("{}", decision_line(&proposal));
        }
    }
    Ok(())
}

/// Read the settled text from `from`, or from stdin for `-`.
///
/// A text that still holds a marker line is refused, because the daemon
/// writes the text as it is.
fn read_settled_text(from: &Path) -> Result<String> {
    let text = if from == Path::new("-") {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("reading the settled text from stdin")?;
        text
    } else {
        std::fs::read_to_string(from)
            .with_context(|| format!("reading the settled text from {}", from.display()))?
    };
    anyhow::ensure!(
        !has_marker_line(&text),
        "the text still holds a `{OURS_MARKER}` or `{THEIRS_MARKER}` line. \
         Settle each conflict, then remove the markers"
    );
    Ok(text)
}

/// Whether `text` holds a marker line that `show` writes.
///
/// The split marker is not checked, because `=======` is also the underline
/// of a Markdown heading.
pub(crate) fn has_marker_line(text: &str) -> bool {
    text.lines()
        .any(|line| line == OURS_MARKER || line == THEIRS_MARKER)
}

/// The name of a state, as the list and the TUI show it.
pub(crate) fn state_label(state: &ProposalState) -> &'static str {
    match state {
        ProposalState::Open => "open",
        ProposalState::Stale => "stale",
        ProposalState::Conflicted { .. } => "conflicted",
        ProposalState::Accepted => "accepted",
        ProposalState::Rejected { .. } => "rejected",
        ProposalState::Superseded { .. } => "superseded",
        ProposalState::Dismissed => "dismissed",
    }
}

/// The writer of a proposal: the plugin name, or the session.
pub(crate) fn author_label(author: &ProposalAuthor) -> String {
    match author {
        ProposalAuthor::Plugin { name } => name.clone(),
        ProposalAuthor::Session { id } => format!("session {id}"),
    }
}

/// "1 file" or "2 files".
pub(crate) fn count_noun(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// One line for each proposal: the id, the state, the file count, the
/// author and the title.
pub(crate) fn list_text(proposals: &[Proposal]) -> String {
    if proposals.is_empty() {
        return "No proposals wait for a decision.\n".to_string();
    }
    proposals
        .iter()
        .map(|p| {
            format!(
                "{}  {:<10}  {:<8}  {}  {}\n",
                p.id,
                state_label(&p.state),
                count_noun(p.writes.len(), "file", "files"),
                author_label(&p.author),
                p.title
            )
        })
        .collect()
}

/// The state of a proposal after a decision, in one line.
fn decision_line(proposal: &Proposal) -> String {
    let id = proposal.id;
    match &proposal.state {
        ProposalState::Accepted => format!(
            "Accepted {id}: the daemon wrote {}.",
            count_noun(proposal.writes.len(), "file", "files")
        ),
        ProposalState::Rejected { .. } => format!("Rejected {id}. No file changed."),
        ProposalState::Dismissed => format!("Dismissed {id}. No file changed."),
        ProposalState::Conflicted { files } => format!(
            "The proposal {id} is conflicted: {} still {} a settled text.",
            count_noun(files.len(), "file", "files"),
            if files.len() == 1 { "needs" } else { "need" }
        ),
        ProposalState::Open | ProposalState::Stale | ProposalState::Superseded { .. } => {
            format!("The proposal {id} is {}.", state_label(&proposal.state))
        }
    }
}

/// The diffset of a proposal, and the two texts of each file.
///
/// The base side is the text that the writer read. A base with no text
/// (an absent file, or a base known only by its hash) has no old side.
pub(crate) fn proposal_diffset(proposal: &Proposal) -> (Diffset, Vec<Option<DiffFileText>>) {
    let source = DiffsetSource::Proposal { id: proposal.id };
    let mut files = Vec::with_capacity(proposal.writes.len());
    let mut texts = Vec::with_capacity(proposal.writes.len());
    for write in &proposal.writes {
        let base_text = match &write.base {
            ExpectedBase::Text { text, .. } => Some(text.clone()),
            ExpectedBase::Absent | ExpectedBase::Hash { .. } | ExpectedBase::Unchecked => None,
        };
        let status = match write.base {
            ExpectedBase::Absent => FileStatus::Added,
            ExpectedBase::Text { .. } | ExpectedBase::Hash { .. } | ExpectedBase::Unchecked => {
                FileStatus::Modified
            }
        };
        let old = base_text.as_deref().unwrap_or_default();
        let too_large = old.len() > MAX_DIFF_BYTES || write.new_text.len() > MAX_DIFF_BYTES;
        let (added, removed) = if too_large {
            (0, 0)
        } else {
            count_changes(old, &write.new_text)
        };
        files.push(DiffFileEntry {
            root: write.root.clone(),
            path: write.path.clone(),
            status,
            added: u32::try_from(added).unwrap_or(u32::MAX),
            removed: u32::try_from(removed).unwrap_or(u32::MAX),
            binary: false,
            too_large,
        });
        texts.push((!too_large).then(|| DiffFileText {
            base_text,
            current_text: Some(write.new_text.clone()),
        }));
    }
    let diffset = Diffset {
        id: source.id(),
        source,
        files,
    };
    (diffset, texts)
}

/// The merged text of a conflicted file, with markers around each region.
///
/// The proposed side goes first, then the disk side, as `git merge` writes
/// ours and theirs.
pub(crate) fn conflict_text(conflict: &FileConflict) -> String {
    let lines: Vec<&str> = conflict.merged_text.split_inclusive('\n').collect();
    let mut regions: Vec<_> = conflict.regions.iter().collect();
    regions.sort_by_key(|r| r.start_line);

    let mut out = String::new();
    let mut next = 0usize;
    for region in regions {
        let start = (region.start_line as usize).saturating_sub(1).max(next);
        let end = (region.end_line as usize).saturating_sub(1).max(start);
        for line in lines.get(next..start.min(lines.len())).unwrap_or_default() {
            out.push_str(line);
        }
        push_line(&mut out, OURS_MARKER);
        push_side(&mut out, &region.ours);
        push_line(&mut out, SPLIT_MARKER);
        push_side(&mut out, &region.theirs);
        push_line(&mut out, THEIRS_MARKER);
        next = end.min(lines.len());
    }
    for line in lines.get(next..).unwrap_or_default() {
        out.push_str(line);
    }
    out
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

/// Add one side of a region. A side at the end of a note can end with no
/// line end, and the next marker must start its own line.
fn push_side(out: &mut String, side: &str) {
    out.push_str(side);
    if !side.is_empty() && !side.ends_with('\n') {
        out.push('\n');
    }
}

/// The text with markers of the conflicted file `path`.
fn conflict_text_of(proposal: &Proposal, path: &str) -> Result<String> {
    let ProposalState::Conflicted { files } = &proposal.state else {
        anyhow::bail!(
            "the proposal {} is {}, not conflicted",
            proposal.id,
            state_label(&proposal.state)
        );
    };
    let conflict = files.iter().find(|c| c.path == path).with_context(|| {
        let paths: Vec<&str> = files.iter().map(|c| c.path.as_str()).collect();
        format!(
            "{path} has no conflict in the proposal {}. The conflicted files: {}",
            proposal.id,
            paths.join(", ")
        )
    })?;
    Ok(conflict_text(conflict))
}

/// An empty row. A text node with no text has no height, so the row holds
/// one space.
fn blank_line() -> Node {
    text(" ")
}

/// The summary of a proposal, the diff of each file and, for a conflicted
/// proposal, each conflicted file with its markers.
pub(crate) fn show_view(proposal: &Proposal, opts: &DiffOptions) -> Node {
    let mut rows = vec![text(proposal.title.clone())];
    let mut facts = format!(
        "{}  {}  by {}  {}",
        proposal.id,
        state_label(&proposal.state),
        author_label(&proposal.author),
        proposal.created_at.format("%Y-%m-%d %H:%M UTC")
    );
    match &proposal.state {
        ProposalState::Superseded { by } => facts.push_str(&format!("  (by {by})")),
        ProposalState::Rejected {
            reason: Some(reason),
        } => facts.push_str(&format!("  (reason: {reason})")),
        ProposalState::Open
        | ProposalState::Stale
        | ProposalState::Conflicted { .. }
        | ProposalState::Accepted
        | ProposalState::Rejected { reason: None }
        | ProposalState::Dismissed => {}
    }
    rows.push(text(facts));
    if let Some(rationale) = &proposal.rationale {
        rows.extend(rationale.lines().map(|line| text(line.to_string())));
    }

    let (diffset, texts) = proposal_diffset(proposal);
    for (entry, file_text) in diffset.files.iter().zip(&texts) {
        rows.push(blank_line());
        rows.push(render_diffset_file(entry, file_text.as_ref(), opts));
    }

    if let ProposalState::Conflicted { files } = &proposal.state {
        rows.push(blank_line());
        rows.push(text(format!(
            "{} with the disk. Settle each one, then run:",
            count_noun(files.len(), "file conflicts", "files conflict"),
        )));
        rows.push(text(format!(
            "  cru proposal resolve {} <path> --from <file>",
            proposal.id
        )));
        for conflict in files {
            rows.push(blank_line());
            rows.push(text(format!("conflict: {}", conflict.path)));
            rows.extend(
                conflict_text(conflict)
                    .lines()
                    .map(|line| text(line.to_string())),
            );
        }
    }
    col(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use crucible_core::note_merge::merge3;
    use crucible_core::proposal::ProposedWrite;
    use crucible_core::session::PhysicalRoot;
    use crucible_oil::render::render_to_plain_text;

    fn write(path: &str, base: ExpectedBase, new_text: &str) -> ProposedWrite {
        ProposedWrite {
            root: PhysicalRoot::from_top_level("/kiln"),
            path: path.into(),
            base,
            new_text: new_text.into(),
        }
    }

    fn base_text(text: &str) -> ExpectedBase {
        ExpectedBase::Text {
            text: text.into(),
            hash: String::new(),
        }
    }

    fn proposal(state: ProposalState, writes: Vec<ProposedWrite>) -> Proposal {
        Proposal {
            id: "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f".parse().unwrap(),
            author: ProposalAuthor::Plugin {
                name: "consolidation".into(),
            },
            session: None,
            title: "Merge the two notes on links".into(),
            rationale: Some("Both notes say the same thing.".into()),
            created_at: chrono::Utc.with_ymd_and_hms(2026, 9, 21, 8, 30, 0).unwrap(),
            state,
            writes,
        }
    }

    fn conflict(base: &str, ours: &str, disk: &str) -> FileConflict {
        let merge = merge3(base, ours, disk);
        assert!(!merge.regions.is_empty(), "the fixture must conflict");
        FileConflict {
            root: PhysicalRoot::from_top_level("/kiln"),
            path: "links.md".into(),
            disk_text: disk.into(),
            merged_text: merge.text,
            regions: merge.regions,
        }
    }

    #[test]
    fn show_prints_conflict_markers() {
        let base = "# Links\none\ntwo\nthree\n";
        let ours = "# Links\none\nTWO from the pass\nthree\n";
        let disk = "# Links\none\ntwo, edited by hand\nthree\n";
        let state = ProposalState::Conflicted {
            files: vec![conflict(base, ours, disk)],
        };
        let p = proposal(state, vec![write("links.md", base_text(base), ours)]);

        let out = render_to_plain_text(&show_view(&p, &DiffOptions::for_width(100)), 100);
        let expected = "# Links\none\n<<<<<<< proposal\nTWO from the pass\n=======\n\
                        two, edited by hand\n>>>>>>> disk\nthree\n";
        assert!(
            out.contains(expected.trim_end()),
            "no marked conflict in:\n{out}"
        );
        assert!(out.contains("1 file conflicts with the disk"), "{out}");
        assert!(out.contains("conflict: links.md"), "{out}");
        assert!(
            out.contains("cru proposal resolve 6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f"),
            "{out}"
        );
        // `--conflict` prints the same text and nothing else.
        assert_eq!(conflict_text_of(&p, "links.md").unwrap(), expected);
        assert!(conflict_text_of(&p, "other.md").is_err());
    }

    #[test]
    fn a_conflict_at_the_end_of_a_note_keeps_each_marker_on_its_own_line() {
        let c = conflict("a\nb", "a\nours", "a\ntheirs");
        assert_eq!(
            conflict_text(&c),
            "a\n<<<<<<< proposal\nours\n=======\ntheirs\n>>>>>>> disk\n"
        );
    }

    #[test]
    fn a_proposal_that_is_not_conflicted_has_no_conflict_text() {
        let p = proposal(ProposalState::Open, vec![]);
        assert!(conflict_text_of(&p, "links.md").is_err());
    }

    #[test]
    fn a_settled_text_with_a_marker_line_is_refused() {
        assert!(has_marker_line("a\n<<<<<<< proposal\nb\n"));
        assert!(has_marker_line("a\n>>>>>>> disk"));
        // A Markdown heading underline is not a marker.
        assert!(!has_marker_line("Title\n=======\ntext\n"));
    }

    #[test]
    fn the_proposal_diffset_holds_the_base_and_the_new_text() {
        let p = proposal(
            ProposalState::Open,
            vec![
                write("a.md", base_text("one\ntwo\n"), "one\n2\n"),
                write("new.md", ExpectedBase::Absent, "fresh\n"),
            ],
        );
        let (diffset, texts) = proposal_diffset(&p);
        assert_eq!(diffset.id.as_str(), format!("proposal-{}", p.id));
        assert_eq!(diffset.files[0].status, FileStatus::Modified);
        assert_eq!((diffset.files[0].added, diffset.files[0].removed), (1, 1));
        assert_eq!(diffset.files[1].status, FileStatus::Added);
        assert_eq!(
            texts[0].as_ref().unwrap().base_text.as_deref(),
            Some("one\ntwo\n")
        );
        assert_eq!(texts[1].as_ref().unwrap().base_text, None, "an absent base");
        assert_eq!(
            texts[1].as_ref().unwrap().current_text.as_deref(),
            Some("fresh\n")
        );
    }

    #[test]
    fn the_list_names_each_proposal_in_one_line() {
        let p = proposal(
            ProposalState::Stale,
            vec![write("a.md", base_text(""), "x\n")],
        );
        assert_eq!(
            list_text(std::slice::from_ref(&p)),
            format!(
                "{}  stale       1 file    consolidation  Merge the two notes on links\n",
                p.id
            )
        );
        assert_eq!(list_text(&[]), "No proposals wait for a decision.\n");
    }
}
