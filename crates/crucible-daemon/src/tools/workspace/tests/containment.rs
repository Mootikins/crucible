//! Filesystem containment for the workspace tool set.
//!
//! Split out of the tool tests because these are read as a set: each one is a
//! shape an adversarial pass found, and the file is the record of what has
//! been closed. `read_file`, `write_file`, `glob` and `grep` all reach the
//! filesystem through one [`crate::tools::fs_scope::FsScope`], so what is
//! asserted here about one of them is asserted about the door all four share.

use super::super::*;
use crate::tools::containment::RootSet;

/// The transcript leak the flat-kiln work closes.
///
/// A kiln-less session's kiln is the daemon's data root, which *contains* the
/// sessions root — so granting the kiln root alone put every session ever
/// recorded inside containment. The deny root removes that subtree; the
/// session's own storage dir, granted as a deeper allowed root, survives it.
#[test]
fn a_session_cannot_read_another_sessions_transcript() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    let own_dir = sessions_root.join("chat-mine");
    let other_dir = sessions_root.join("chat-theirs");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(data_root.join("Note.md"), "kiln note").unwrap();
    std::fs::write(own_dir.join("session.jsonl"), "mine").unwrap();
    std::fs::write(other_dir.join("session.jsonl"), "theirs").unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(
        RootSet::scoped(
            vec![workspace.clone(), data_root.clone()],
            vec![sessions_root.clone()],
        )
        .carve_out(vec![own_dir.clone()]),
    );

    assert!(
        tools
            .resolve_path(&other_dir.join("session.jsonl").to_string_lossy())
            .is_err(),
        "another session's transcript must be outside containment"
    );
    assert!(
        tools
            .resolve_path(&sessions_root.to_string_lossy())
            .is_err(),
        "the sessions root itself must not be enumerable"
    );
    assert!(
        tools
            .resolve_path(&own_dir.join("session.jsonl").to_string_lossy())
            .is_ok(),
        "a session must still reach its own storage"
    );
    assert!(
        tools
            .resolve_path(&data_root.join("Note.md").to_string_lossy())
            .is_ok(),
        "denying the sessions root must not cost the kiln around it"
    );
}

/// An empty kiln set degrades capabilities, never containment.
///
/// A kiln-less session whose workspace was also detached carries `""` in both
/// roles, and the builder used to seed the root list with it.
/// `Path::starts_with("")` is true for every path and `"".components()` counts
/// zero, so the deepest-match rule answered "permitted, depth 0" for the whole
/// filesystem — containment was off, not merely narrow. Resolution alone does
/// not fix it either: `""` anchors at the daemon's working directory, which is
/// a grant nobody chose.
#[test]
fn an_empty_root_denies_everything_instead_of_permitting_it() {
    let tools = WorkspaceTools::new("")
        .with_containment(RootSet::scoped(vec![std::path::PathBuf::new()], vec![]));

    assert!(
        tools.resolve_path("/etc/shadow").is_err(),
        "an empty allowed root must deny, not permit the whole filesystem"
    );
    assert!(
        tools.resolve_path("/").is_err(),
        "an empty allowed root must not admit the filesystem root"
    );
}

/// Same rule, reached the other way: an empty string handed in as an *extra*
/// root must not widen a set that was otherwise contained.
#[test]
fn an_empty_extra_root_does_not_widen_containment() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(RootSet::scoped(
        vec![workspace.clone(), std::path::PathBuf::new()],
        vec![],
    ));

    assert!(
        tools.resolve_path("/etc/shadow").is_err(),
        "an empty extra root must not disable containment"
    );
    assert!(
        tools
            .resolve_path(&workspace.join("f.txt").to_string_lossy())
            .is_ok(),
        "the real workspace root must still be reachable"
    );
}

/// `glob` is pointed at an allowed root and then walks; the results are where
/// containment has to be re-applied. Pointing it at the data root of a
/// kiln-less session and asking for `**/*` enumerates every session directory
/// on the machine unless what it YIELDS is filtered.
#[tokio::test]
async fn glob_does_not_yield_paths_from_a_denied_subtree() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    let own_dir = sessions_root.join("chat-mine");
    let other_dir = sessions_root.join("chat-theirs");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(data_root.join("Note.md"), "kiln note").unwrap();
    std::fs::write(own_dir.join("session.jsonl"), "mine").unwrap();
    std::fs::write(other_dir.join("session.jsonl"), "theirs").unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(
        RootSet::scoped(
            vec![workspace.clone(), data_root.clone()],
            vec![sessions_root.clone()],
        )
        .carve_out(vec![own_dir.clone()]),
    );

    let result = tools
        .glob(
            "**/*".to_string(),
            Some(data_root.to_string_lossy().to_string()),
            None,
        )
        .expect("the data root is an allowed root, so the glob itself is permitted");
    let content = format!("{:?}", result.content);

    assert!(
        !content.contains("chat-theirs"),
        "another session's storage must not be enumerable through glob: {content}"
    );
    assert!(
        content.contains("Note.md"),
        "the kiln around the denied subtree must still be globbable: {content}"
    );
    assert!(
        content.contains("chat-mine"),
        "a session must still see its own storage: {content}"
    );
}

/// The `grep` result filter, tested on ripgrep's wire format rather than on
/// ripgrep.
///
/// The end-to-end version of this (`grep_does_not_yield_matches_from_a_denied_
/// subtree`) is `#[ignore]`d behind the ripgrep prerequisite, so it does not
/// run in the default suite — which means the filter it exists to prove had no
/// running coverage at all. The filter itself needs no external binary: it
/// reads `path\0line:text` and asks containment. A path with no NUL carries no
/// path, so it is dropped rather than attributed to a permitted file.
#[test]
fn a_grep_output_line_from_a_denied_subtree_is_dropped() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    let own_dir = sessions_root.join("chat-mine");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::create_dir_all(sessions_root.join("chat-theirs")).unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(
        RootSet::scoped(
            vec![workspace.clone(), data_root.clone()],
            vec![sessions_root.clone()],
        )
        .carve_out(vec![own_dir.clone()]),
    );

    let line = |path: std::path::PathBuf| format!("{}\u{0}1:password", path.display());

    assert!(
        !tools.grep_line_is_permitted(&line(
            sessions_root.join("chat-theirs").join("session.jsonl")
        )),
        "a hit inside the denied sessions root must be dropped"
    );
    assert!(
        tools.grep_line_is_permitted(&line(own_dir.join("session.jsonl"))),
        "a session must still see hits in its own storage"
    );
    assert!(
        tools.grep_line_is_permitted(&line(data_root.join("Note.md"))),
        "and in the kiln around the denied subtree"
    );
    assert!(
        !tools.grep_line_is_permitted("rg: some diagnostic with no NUL"),
        "a line carrying no path cannot be attributed to a permitted file"
    );
}

/// `grep` containment-checked only the directory it was pointed at and then
/// let ripgrep recurse, so `grep(path = <data root>)` printed every other
/// session's transcript verbatim — the exact subtree `denied_roots` exists to
/// close. `glob` post-filters what it yields; `grep` must too.
#[tokio::test]
#[ignore = "requires: ripgrep"]
async fn grep_does_not_yield_matches_from_a_denied_subtree() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    let own_dir = sessions_root.join("chat-mine");
    let other_dir = sessions_root.join("chat-theirs");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(data_root.join("Note.md"), "password hunting notes").unwrap();
    std::fs::write(own_dir.join("session.jsonl"), "password mine").unwrap();
    std::fs::write(other_dir.join("session.jsonl"), "password theirs").unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(
        RootSet::scoped(
            vec![workspace.clone(), data_root.clone()],
            vec![sessions_root.clone()],
        )
        .carve_out(vec![own_dir.clone()]),
    );

    let result = tools
        .grep(
            "password".to_string(),
            Some(data_root.to_string_lossy().to_string()),
            None,
            None,
        )
        .await
        .expect("the data root is an allowed root, so the search itself is permitted");
    let content = format!("{:?}", result.content);

    assert!(
        !content.contains("theirs"),
        "another session's transcript must not be yielded by grep: {content}"
    );
    assert!(
        content.contains("hunting"),
        "the kiln around the denied subtree must still be searchable: {content}"
    );
    assert!(
        content.contains("mine"),
        "a session must still grep its own storage: {content}"
    );
}

/// Legacy transcripts left inside a kiln by an incomplete migration are the
/// same secret in a different place. A kiln appearing mid-run is never
/// scanned and `migrate_one` skips on failure, so `{kiln}/.crucible/sessions`
/// has to be denied rather than assumed empty.
#[test]
fn a_kilns_own_legacy_session_directory_is_not_readable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    let legacy = kiln.join(".crucible").join("sessions").join("chat-old");
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(kiln.join("Note.md"), "kiln note").unwrap();
    std::fs::write(legacy.join("session.jsonl"), "legacy").unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(RootSet::scoped(
        vec![workspace.clone(), kiln.clone()],
        vec![kiln.join(".crucible").join("sessions")],
    ));

    assert!(
        tools
            .resolve_path(&legacy.join("session.jsonl").to_string_lossy())
            .is_err(),
        "a legacy in-kiln transcript must be outside containment"
    );
    assert!(
        tools
            .resolve_path(&kiln.join("Note.md").to_string_lossy())
            .is_ok(),
        "denying the legacy sessions dir must not cost the kiln around it"
    );
}

// =============================================================================
// Resolution: both forms, and symlink escape as an outcome
// =============================================================================

/// The escape that made a lenient canonicalize worthless.
///
/// Canonicalizing the deepest EXISTING ancestor and re-appending the remainder
/// verbatim leaves `..` in the string. `{workspace}/not-yet/../../secret` then
/// `starts_with` the workspace and is waved through — while the kernel, which
/// gets the same string a moment later, walks up out of it. No write is needed
/// to complete this one; the read alone leaves containment.
#[tokio::test]
async fn a_traversal_through_a_missing_directory_stays_inside_the_workspace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let secret = tmp.path().join("secret");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::write(secret.join("key.txt"), "SECRET-KEY").unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_containment(RootSet::scoped(vec![workspace.clone()], vec![]));
    let escape = "not-yet/../../secret/key.txt";

    assert!(
        tools.resolve_path(escape).is_err(),
        "'{escape}' resolves outside the workspace and must be refused"
    );

    let read = format!(
        "{:?}",
        tools.read_file(escape.to_string(), None, None).await
    );
    assert!(
        !read.contains("SECRET-KEY"),
        "read_file walked out of the workspace: {read}"
    );
}

/// Same shape, aimed at the denied subtree rather than at the workspace edge.
///
/// `{data}/not-yet/../sessions/{victim}` keeps its `..` through the check, so
/// it fails `starts_with(sessions_root)` and the denial never fires — while
/// the allowed data root around it still matches. The agent's own `write_file`
/// can then create `not-yet`, but it does not have to: the read is already
/// permitted.
#[test]
fn a_traversal_through_a_missing_directory_cannot_reach_another_sessions_transcript() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    let own_dir = sessions_root.join("chat-mine");
    let other_dir = sessions_root.join("chat-theirs");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(other_dir.join("session.jsonl"), "theirs").unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(
        RootSet::scoped(
            vec![workspace.clone(), data_root.clone()],
            vec![sessions_root.clone()],
        )
        .carve_out(vec![own_dir.clone()]),
    );

    let dodge = data_root
        .join("not-yet")
        .join("..")
        .join("sessions")
        .join("chat-theirs")
        .join("session.jsonl");

    assert!(
        tools.resolve_path(&dodge.to_string_lossy()).is_err(),
        "a `..` through a missing directory reached another session's transcript: {}",
        dodge.display()
    );
}

/// A symlink leaving containment is a distinct resolved state, not a silent
/// refusal. Silent refusal is what makes symlink handling untestable, and is
/// where CVE-2026-39861 and CVE-2026-50549 both lived.
#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_workspace_is_reported_as_an_escape_naming_its_target() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let secret = tmp.path().join("secret");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::write(secret.join("key.txt"), "SECRET-KEY").unwrap();
    let link = workspace.join("innocent");
    std::os::unix::fs::symlink(&secret, &link).unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_containment(RootSet::scoped(vec![workspace.clone()], vec![]));
    let target = link.join("key.txt");

    let err = tools
        .resolve_path(&target.to_string_lossy())
        .expect_err("a symlink out of the workspace must be refused");
    assert!(
        err.message.contains("symlink"),
        "the refusal must say a symlink was followed: {}",
        err.message
    );
    assert!(
        err.message.contains("secret/key.txt"),
        "and must name where it lands, so the escape is a fact the caller can \
         act on rather than a generic no: {}",
        err.message
    );
}

/// The escape outcome must be about leaving containment, not about symlinks
/// existing: a link that stays inside the workspace is ordinary and permitted.
#[cfg(unix)]
#[test]
fn a_symlink_that_stays_inside_the_workspace_is_permitted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let real = workspace.join("real");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(real.join("note.md"), "note").unwrap();
    let link = workspace.join("shortcut");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_containment(RootSet::scoped(vec![workspace.clone()], vec![]));

    assert!(
        tools
            .resolve_path(&link.join("note.md").to_string_lossy())
            .is_ok(),
        "a symlink that lands inside containment is not an escape"
    );
}

/// Containment is judged on the lexical form AND on the resolved one, because
/// neither alone is trustworthy: the lexical form cannot see a symlink, and
/// the resolved form of a path whose parents do not exist is only as good as
/// the ancestor walk that produced it. OpenClaw's GHSA-575v-8hfq-m3mc fix was
/// exactly this pairing.
#[cfg(unix)]
#[test]
fn a_symlink_reached_through_a_missing_directory_is_still_refused() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let secret = tmp.path().join("secret");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    let link = workspace.join("innocent");
    std::os::unix::fs::symlink(&secret, &link).unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_containment(RootSet::scoped(vec![workspace.clone()], vec![]));
    let target = workspace
        .join("not-yet")
        .join("..")
        .join("innocent")
        .join("key.txt");

    assert!(
        tools.resolve_path(&target.to_string_lossy()).is_err(),
        "a `..` past a missing directory onto a symlink must not escape: {}",
        target.display()
    );
}

/// The escape outcome must stay a statement of fact. A path that is outside
/// containment under every reading is `Outside`, and saying "resolves through
/// a symlink" about it would be a security message that misdescribes what
/// happened — the one kind of message a reviewer cannot afford to distrust.
#[test]
fn a_path_plainly_outside_containment_is_not_reported_as_a_symlink_escape() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_containment(RootSet::scoped(vec![workspace.clone()], vec![]));

    let err = tools
        .resolve_path("/etc/shadow")
        .expect_err("a path outside the roots must be refused");
    assert!(
        !err.message.contains("symlink"),
        "no symlink was involved, so the refusal must not claim one: {}",
        err.message
    );
    assert!(
        err.message.contains("outside this session's allowed roots"),
        "{}",
        err.message
    );
}

/// The lexical verdict decides on its own, and this is the case where it has
/// to: a session must not be able to *address* the denied subtree, not merely
/// be stopped from reading what is in it.
///
/// A symlink at `{sessions_root}/{id}` pointing somewhere allowed resolves to
/// a permitted file, so a canonical-only check hands it over — and answering
/// at all turns the denied root into an enumeration oracle for which session
/// ids exist. Judging the name as written closes that; judging only where it
/// lands cannot.
#[cfg(unix)]
#[test]
fn a_denied_name_is_refused_even_when_it_resolves_somewhere_allowed() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&sessions_root).unwrap();
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(data_root.join("Note.md"), "kiln note").unwrap();
    std::os::unix::fs::symlink(&data_root, sessions_root.join("chat-probe")).unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(RootSet::scoped(
        vec![workspace.clone(), data_root.clone()],
        vec![sessions_root.clone()],
    ));

    assert!(
        tools
            .resolve_path(
                &sessions_root
                    .join("chat-probe")
                    .join("Note.md")
                    .to_string_lossy()
            )
            .is_err(),
        "a name inside the denied sessions root must be refused however it resolves"
    );
    assert!(
        tools
            .resolve_path(&data_root.join("Note.md").to_string_lossy())
            .is_ok(),
        "the same file under its own name stays readable"
    );
}

/// The tool, not the door — because "the check exists" and "the tool calls
/// it" are different claims, and the second is the one two adversarial passes
/// found broken. Asserted on the filesystem afterwards: a refusal that still
/// created the file is not a refusal.
#[tokio::test]
async fn write_file_refuses_to_create_a_config_the_daemon_would_execute() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let tools = WorkspaceTools::new(&workspace)
        .with_containment(RootSet::scoped(vec![workspace.clone()], vec![]));

    for path in [
        ".crucible/project.toml",
        ".crucible/plugins/evil/init.lua",
        ".git/hooks/post-checkout",
        ".claude/settings.json",
    ] {
        let err = tools
            .write_file(path.to_string(), "payload".to_string())
            .await
            .expect_err("a protected write must be refused");
        assert!(err.message.contains("protected"), "{path}: {}", err.message);
        assert!(
            !workspace.join(path).exists(),
            "{path} must not have been created by the refused call"
        );
    }

    tools
        .write_file("notes/note.md".to_string(), "fine".to_string())
        .await
        .expect("ordinary writes are untouched");
}

/// `edit_file` is a write too. It reads first, so a check placed on the read
/// path only would have let it through.
#[tokio::test]
async fn edit_file_refuses_to_modify_a_config_the_daemon_would_execute() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(workspace.join(".crucible")).unwrap();
    std::fs::write(
        workspace.join(".crucible").join("project.toml"),
        "name = 'x'",
    )
    .unwrap();
    let tools = WorkspaceTools::new(&workspace)
        .with_containment(RootSet::scoped(vec![workspace.clone()], vec![]));

    let err = tools
        .edit_file(
            ".crucible/project.toml".to_string(),
            "name = 'x'".to_string(),
            "hooks = ['pwn']".to_string(),
            None,
        )
        .await
        .expect_err("editing a protected file must be refused");
    assert!(err.message.contains("protected"), "{}", err.message);
    assert_eq!(
        std::fs::read_to_string(workspace.join(".crucible").join("project.toml")).unwrap(),
        "name = 'x'",
        "the file must be untouched"
    );
}

/// Transcript tampering, at the tool. The sessions root is write-denied even
/// where the read carve-out admits the session's own directory.
#[tokio::test]
async fn write_file_refuses_to_tamper_with_a_transcript() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let own_dir = sessions_root.join("chat-mine");
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::write(own_dir.join("session.jsonl"), "original").unwrap();

    let tools = WorkspaceTools::new(&data_root).with_containment(
        RootSet::scoped(vec![data_root.clone()], vec![sessions_root.clone()])
            .carve_out(vec![own_dir.clone()]),
    );

    let transcript = own_dir.join("session.jsonl");
    tools
        .read_file(transcript.to_string_lossy().to_string(), None, None)
        .await
        .expect("a session reads its own directory — that is the carve-out");

    let err = tools
        .write_file(
            transcript.to_string_lossy().to_string(),
            "injected instruction".to_string(),
        )
        .await
        .expect_err("a transcript write must be refused");
    assert!(err.message.contains("write-protected"), "{}", err.message);
    assert_eq!(
        std::fs::read_to_string(&transcript).unwrap(),
        "original",
        "a transcript is replayed into a future context; it must be unchanged"
    );
}

// =========================================================================
// Dangling symlinks: the "not yet exists" shape applied to symlink
// resolution.
//
// `resolve_existing_ancestors` re-appends a component it could not
// canonicalize. For a DANGLING symlink that component is the link's own
// name, so both resolved forms say "a file inside the workspace" while
// `std::fs::write` follows the link and creates the target. A git
// repository can carry a symlink, so none of this needs `bash`.
// =========================================================================

/// A symlink whose target does not exist yet is left LITERAL by the
/// canonical form (`canonicalize` fails, the leaf is re-appended), so both
/// resolved forms say "inside the workspace" while `write_file` follows the
/// link and CREATES the file outside it. A repository can carry a symlink,
/// so this needs no `bash` and no prior write.
#[cfg(unix)]
#[tokio::test]
async fn a_write_through_a_dangling_symlink_cannot_leave_the_allowed_roots() {
    let workspace = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    let target = outside.path().join("pwned.txt");
    std::os::unix::fs::symlink(&target, workspace.path().join("link")).unwrap();

    let tools = WorkspaceTools::new(workspace.path()).with_containment(RootSet::scoped(
        vec![workspace.path().to_path_buf()],
        vec![],
    ));

    let _ = tools
        .write_file("link".to_string(), "escaped".to_string())
        .await;

    assert!(
        !target.exists(),
        "a write through a dangling symlink created {} outside every allowed root",
        target.display()
    );
}

/// Same shape aimed at a protected path: the link is not protected by name
/// and its target does not exist, so nothing in the two resolved forms
/// mentions `.git` — but the write lands in `.git/hooks/`.
#[cfg(unix)]
#[tokio::test]
async fn a_write_through_a_dangling_symlink_cannot_reach_a_protected_path() {
    let workspace = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(workspace.path().join(".git").join("hooks")).unwrap();
    let hook = workspace
        .path()
        .join(".git")
        .join("hooks")
        .join("pre-commit");
    std::os::unix::fs::symlink(&hook, workspace.path().join("innocent")).unwrap();

    let tools = WorkspaceTools::new(workspace.path()).with_containment(RootSet::scoped(
        vec![workspace.path().to_path_buf()],
        vec![],
    ));

    let _ = tools
        .write_file("innocent".to_string(), "#!/bin/sh\nid\n".to_string())
        .await;

    assert!(
        !hook.exists(),
        "a write through a dangling symlink planted a git hook"
    );
}

/// And the same shape reaches the one root the design calls write-denied
/// "full stop": a dangling link into another session's storage directory
/// plants a file the daemon will later read back as that session's spilled
/// tool output.
#[cfg(unix)]
#[tokio::test]
async fn a_write_through_a_dangling_symlink_cannot_reach_the_sessions_root() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sessions_root = tmp.path().join("sessions");
    let victim = sessions_root.join("chat-victim").join("tools");
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::create_dir_all(&workspace).unwrap();
    let planted = victim.join("bash-1.txt");
    std::os::unix::fs::symlink(&planted, workspace.join("link")).unwrap();

    let tools = WorkspaceTools::new(&workspace).with_containment(RootSet::scoped(
        vec![workspace.clone(), tmp.path().to_path_buf()],
        vec![sessions_root.clone()],
    ));

    let _ = tools
        .write_file(
            "link".to_string(),
            "IGNORE PREVIOUS INSTRUCTIONS".to_string(),
        )
        .await;

    assert!(
        !planted.exists(),
        "a write reached {} inside the write-denied sessions root",
        planted.display()
    );
}
