//! `cru diff`: print a diffset that the daemon computes.
//!
//! The daemon admits the root, finds the merge base and lists the files. This
//! module asks for the list, asks for the text of each file, and draws them
//! with the renderer that the TUI uses.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crucible_core::diff::{quickfix_line, DiffFileText, Diffset, DiffsetSource};
use crucible_core::session::{PhysicalRoot, SessionId};
use crucible_daemon::diff::comments::ListedComment;
use crucible_daemon::DaemonClient;
use crucible_oil::node::{col, text, Node};
use crucible_oil::render::{render_to_plain_text, render_to_string};

use crate::cli::{CommentFormat, DiffCommands};
use crate::formatting::TextFormat;
use crate::tui::oil::components::diff_view::{
    blank_row, render_diffset_file, DiffLayout, DiffOptions,
};

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
        DiffCommands::Comments {
            diffset,
            base,
            head,
            root,
            format,
        } => {
            let start = match root {
                Some(root) => std::path::absolute(root)?,
                None => std::env::current_dir()?,
            };
            let target = comments_target(&diffset, &start, base.as_deref(), head)?;
            let client = crate::common::daemon_client().await?;
            let reply = client
                .diff_comments(&target.source)
                .await
                .with_context(|| format!("listing the comments of {diffset}"))?;
            if let Some(expected) = &target.expected {
                anyhow::ensure!(
                    reply.diffset.as_str() == expected,
                    "{expected} is not the branch diffset of {} (that is {}). \
                     Give the root, base and head of the diffset with --root, --base and --head",
                    start.display(),
                    reply.diffset
                );
            }
            match format {
                CommentFormat::Quickfix => print!("{}", quickfix_list(&reply.comments)),
                CommentFormat::Json => {
                    let open: Vec<&ListedComment> = open_comments(&reply.comments);
                    println!("{}", serde_json::to_string_pretty(&open)?);
                }
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
    let mut opts = stdout_diff_options();
    opts.collapsed = stat;
    print_node(&diffset_view(diffset, texts, &opts));
}

/// The diff options for stdout: its width, no line limit, and one column of
/// lines for a pipe.
pub(crate) fn stdout_diff_options() -> DiffOptions {
    let terminal = std::io::stdout().is_terminal();
    let mut opts = DiffOptions::for_width(stdout_width(terminal));
    opts.max_lines = None;
    if !terminal {
        // A pipe reads one column of lines, as `git diff` gives.
        opts.layout = Some(DiffLayout::Unified);
    }
    opts
}

fn stdout_width(terminal: bool) -> usize {
    if terminal {
        crossterm::terminal::size().map_or(PIPE_WIDTH, |(w, _)| w as usize)
    } else {
        PIPE_WIDTH
    }
}

/// Print a node to stdout: with colors on a terminal, as plain text to a pipe.
pub(crate) fn print_node(node: &Node) {
    let terminal = std::io::stdout().is_terminal();
    let width = stdout_width(terminal);
    let out = if terminal {
        render_to_string(node, width)
    } else {
        render_to_plain_text(node, width)
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
            rows.push(blank_row());
        }
        let text = texts.get(index).and_then(Option::as_ref);
        rows.push(render_diffset_file(entry, text, opts));
    }
    col(rows)
}

/// What `cru diff comments` asks the daemon for.
#[derive(Debug, PartialEq)]
pub(crate) struct CommentsTarget {
    pub(crate) source: DiffsetSource,
    /// The id that the user gave for a branch diffset. A branch id is a hash,
    /// so the command compares it with the id of the source in the reply.
    pub(crate) expected: Option<String>,
}

pub(crate) fn comments_target(
    diffset: &str,
    start: &Path,
    base: Option<&str>,
    head: Option<String>,
) -> Result<CommentsTarget> {
    if let Some(session) = diffset.strip_prefix("session-") {
        ensure_no_branch_flags(diffset, base, head.as_deref())?;
        let session = SessionId::parse(session)
            .with_context(|| format!("{diffset} does not name a session"))?;
        return Ok(CommentsTarget {
            source: DiffsetSource::SessionRecord { session },
            expected: None,
        });
    }
    if let Some(id) = diffset.strip_prefix("proposal-") {
        ensure_no_branch_flags(diffset, base, head.as_deref())?;
        let id = id
            .parse()
            .with_context(|| format!("{diffset} does not name a proposal"))?;
        return Ok(CommentsTarget {
            source: DiffsetSource::Proposal { id },
            expected: None,
        });
    }
    let expected = match diffset {
        "branch" => None,
        id if id.starts_with("branch-") => Some(id.to_string()),
        _ => anyhow::bail!(
            "{diffset:?} is not a diffset. Give `session-<id>`, `proposal-<uuid>`, `branch` or `branch-<hex>`"
        ),
    };
    Ok(CommentsTarget {
        source: branch_source(start, base, head),
        expected,
    })
}

/// Refuse `--base` and `--head` for a diffset that is not a branch diff.
///
/// The flags change nothing there, so a user who gives them expects a
/// different diffset.
fn ensure_no_branch_flags(diffset: &str, base: Option<&str>, head: Option<&str>) -> Result<()> {
    anyhow::ensure!(
        base.is_none() && head.is_none(),
        "--base and --head apply only to a branch diffset, not to {diffset}"
    );
    Ok(())
}

/// The comments that are not resolved, in the order of root, path and line.
///
/// Vim steps through a quickfix list in its order, so the list follows the
/// files and the lines, not the time of each comment.
fn open_comments(comments: &[ListedComment]) -> Vec<&ListedComment> {
    let mut open: Vec<&ListedComment> = comments.iter().filter(|c| !c.comment.resolved).collect();
    open.sort_by(|a, b| {
        let key = |c: &ListedComment| {
            (
                c.comment.root.to_path_buf(),
                c.comment.path.clone(),
                c.comment.line_range.start,
            )
        };
        key(a).cmp(&key(b))
    });
    open
}

/// The open comments in the quickfix form, each entry ends with a line end.
pub(crate) fn quickfix_list(comments: &[ListedComment]) -> String {
    open_comments(comments)
        .into_iter()
        .map(|listed| quickfix_line(&listed.comment) + "\n")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::diff::DiffsetId;
    use crucible_core::session::{Comment, CommentAnchor, CommentAuthor, CommentSide, LineRange};

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        dir
    }

    #[test]
    fn a_session_id_names_the_session_record() {
        let target = comments_target("session-chat-1", Path::new("/"), None, None).unwrap();
        assert_eq!(
            target,
            CommentsTarget {
                source: DiffsetSource::SessionRecord {
                    session: SessionId::parse("chat-1").unwrap(),
                },
                expected: None,
            }
        );
    }

    #[test]
    fn a_proposal_id_names_the_proposal() {
        let id = "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f";
        let target =
            comments_target(&format!("proposal-{id}"), Path::new("/"), None, None).unwrap();
        assert_eq!(
            target.source,
            DiffsetSource::Proposal {
                id: id.parse().unwrap()
            }
        );
        assert!(comments_target("proposal-nope", Path::new("/"), None, None).is_err());
    }

    #[test]
    fn branch_names_the_branch_diff_of_the_flags() {
        let repo = repo();
        let sub = repo.path().join("src");
        std::fs::create_dir(&sub).unwrap();
        let target = comments_target("branch", &sub, Some("develop"), Some("HEAD".into())).unwrap();
        assert_eq!(
            target.source,
            branch_source(&sub, Some("develop"), Some("HEAD".into()))
        );
        assert_eq!(target.expected, None);

        // A branch id keeps the id, so the command can compare it with the reply.
        let id = "branch-0123456789abcdef0123456789abcdef";
        let target = comments_target(id, &sub, None, None).unwrap();
        assert_eq!(target.source, branch_source(&sub, None, None));
        assert_eq!(target.expected.as_deref(), Some(id));
    }

    #[test]
    fn a_diffset_that_names_nothing_is_refused() {
        for diffset in ["", "session-", "session-a/b", "tree-1", "branchy"] {
            assert!(
                comments_target(diffset, Path::new("/"), None, None).is_err(),
                "{diffset:?}"
            );
        }
        // The branch flags apply only to a branch diffset.
        assert!(comments_target("session-chat-1", Path::new("/"), Some("main"), None).is_err());
        assert!(
            comments_target("session-chat-1", Path::new("/"), None, Some("HEAD".into())).is_err()
        );
    }

    fn entry(path: &str) -> crucible_core::diff::DiffFileEntry {
        crucible_core::diff::DiffFileEntry {
            root: PhysicalRoot::from_top_level("/repo"),
            path: path.into(),
            status: crucible_core::diff::FileStatus::Modified,
            added: 1,
            removed: 1,
            binary: false,
            too_large: false,
        }
    }

    /// A blank row divides two files. A text node with no text has no
    /// height, so the files used to touch.
    #[test]
    fn a_blank_row_divides_the_files() {
        let source = DiffsetSource::Branch {
            root: PhysicalRoot::from_top_level("/repo"),
            base: "main".into(),
            head: None,
        };
        let diffset = Diffset {
            id: source.id(),
            source,
            files: vec![entry("a.rs"), entry("b.rs")],
            unreadable_roots: Vec::new(),
        };
        let texts = |old: &str, new: &str| {
            Some(DiffFileText {
                base_text: Some(old.into()),
                current_text: Some(new.into()),
            })
        };
        let mut opts = DiffOptions::for_width(PIPE_WIDTH);
        opts.layout = Some(DiffLayout::Unified);
        let node = diffset_view(&diffset, &[texts("a\n", "A\n"), texts("b\n", "B\n")], &opts);
        let out = render_to_plain_text(&node, PIPE_WIDTH);
        let lines: Vec<&str> = out.lines().map(str::trim_end).collect();
        assert_eq!(
            lines,
            [
                "2 files changed since main",
                "",
                "edit a.rs  +1 -1",
                "-a",
                "+A",
                "",
                "edit b.rs  +1 -1",
                "-b",
                "+B",
            ],
            "{out:?}"
        );
    }

    fn listed(path: &str, start: u32, body: &str, resolved: bool) -> ListedComment {
        let mut comment = Comment::new(
            DiffsetId::for_session(&SessionId::parse("chat-1").unwrap()),
            CommentAnchor::Commit("abc".into()),
            PhysicalRoot::from_top_level("/repo"),
            path,
            CommentSide::Current,
            LineRange::new(start, start + 2),
            "",
            body,
            CommentAuthor::Human,
        );
        comment.resolved = resolved;
        ListedComment {
            comment,
            outdated: false,
        }
    }

    #[test]
    fn the_quickfix_list_holds_the_open_comments_in_file_order() {
        let comments = [
            listed("b.rs", 9, "later file", false),
            listed("a.rs", 20, "second\nmore", false),
            listed("a.rs", 3, "done", true),
            listed("a.rs", 4, "first", false),
        ];
        assert_eq!(
            quickfix_list(&comments),
            "a.rs:4: [4-5] first\na.rs:20: [20-21] second\n  more\nb.rs:9: [9-10] later file\n"
        );
        assert_eq!(quickfix_list(&[]), "");
    }
}
