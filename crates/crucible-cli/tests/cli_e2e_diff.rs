//! `cru diff branch` prints the branch diff of a real repository.
//!
//! This crosses the process boundary on purpose: a real `cru` asks a real
//! daemon over a real socket, and the daemon runs git on a temp repository.
//! The daemon handlers have their own tests. What nothing else proves is that
//! the command sends a source that the daemon admits and prints its answer.

mod cli_e2e_helpers;

use std::path::Path;

use cli_e2e_helpers::TestDaemon;

fn git(dir: &Path, args: &[&str]) {
    let status = crucible_core::git::command()
        .current_dir(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn diff_branch_prints_the_changes_of_a_registered_repository() {
    let daemon = TestDaemon::start();
    let workspace = tempfile::tempdir().expect("workspace");
    let repo = workspace.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("repo dir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/kept.rs"), "fn one() {}\n").unwrap();
    std::fs::write(repo.join("src/gone.rs"), "fn doomed() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);

    // The working tree changes one file and deletes another.
    std::fs::write(repo.join("src/kept.rs"), "fn two() {}\n").unwrap();
    std::fs::remove_file(repo.join("src/gone.rs")).unwrap();

    // A repository that no admission names is refused.
    let refused = daemon
        .command()
        .args(["diff", "branch", "--root"])
        .arg(&repo)
        .output()
        .expect("run cru diff branch");
    assert!(
        !refused.status.success(),
        "an unregistered root: {}",
        stdout_of(&refused)
    );
    assert!(
        stderr_of(&refused).contains("registered project"),
        "the refusal names the remedy: {}",
        stderr_of(&refused)
    );

    let registered = daemon
        .command()
        .args(["project", "register"])
        .arg(&repo)
        .output()
        .expect("run cru project register");
    assert!(registered.status.success(), "{}", stderr_of(&registered));

    // From a subdirectory: the command finds the top level of the repository.
    let output = daemon
        .command()
        .current_dir(repo.join("src"))
        .args(["diff", "branch"])
        .output()
        .expect("run cru diff branch");
    let stdout = stdout_of(&output);
    assert!(
        output.status.success(),
        "cru diff branch failed:\nstdout: {stdout}\nstderr: {}",
        stderr_of(&output)
    );
    assert!(stdout.contains("2 files changed since main"), "{stdout}");
    assert!(stdout.contains("edit src/kept.rs"), "{stdout}");
    assert!(stdout.contains("-fn one() {}"), "{stdout}");
    assert!(stdout.contains("+fn two() {}"), "{stdout}");
    assert!(stdout.contains("delete src/gone.rs"), "{stdout}");
    assert!(stdout.contains("-fn doomed() {}"), "{stdout}");
    assert!(
        !stdout.contains('\u{1b}'),
        "a pipe gets no escape codes: {stdout:?}"
    );
}
