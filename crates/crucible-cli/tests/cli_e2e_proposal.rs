//! `cru proposal accept` writes the note of a proposal.
//!
//! This crosses the process boundary on purpose: a real `cru` asks a real
//! daemon over a real socket, and the daemon writes a real file in a
//! registered kiln. The daemon store and its accept have their own tests.
//! What nothing else proves is that the command names the proposal that the
//! daemon holds and that the note on disk changes.

mod cli_e2e_helpers;

use std::path::Path;

use cli_e2e_helpers::TestDaemon;
use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::proposal::{Proposal, ProposalAuthor, ProposalId, ProposalState, ProposedWrite};
use crucible_core::session::PhysicalRoot;

fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Put one open proposal in the store of the daemon, as a pass in the
/// `propose` mode leaves it.
fn store_proposal(daemon: &TestDaemon, kiln: &Path, base: &str, new_text: &str) -> ProposalId {
    let id = ProposalId::generate();
    let proposal = Proposal {
        id,
        author: ProposalAuthor::Plugin {
            name: "reflection".into(),
        },
        session: None,
        title: "Say what a link is".into(),
        rationale: None,
        created_at: chrono::Utc::now(),
        state: ProposalState::Open,
        writes: vec![ProposedWrite {
            root: PhysicalRoot::from_top_level(kiln),
            path: "links.md".into(),
            base: ExpectedBase::Text {
                text: base.into(),
                hash: disk_hash(base),
            },
            new_text: new_text.into(),
        }],
    };
    let dir = daemon.data_root().join("proposals");
    std::fs::create_dir_all(&dir).expect("proposals dir");
    std::fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_string_pretty(&proposal).unwrap(),
    )
    .expect("write the proposal");
    id
}

#[test]
fn proposal_accept_writes_the_note() {
    let daemon = TestDaemon::start();
    let workspace = tempfile::tempdir().expect("workspace");
    let kiln = workspace.path().join("notes");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let kiln = kiln.canonicalize().expect("canonical kiln");
    let base = "# Links\n";
    let new_text = "# Links\n\nA link names another note.\n";
    std::fs::write(kiln.join("links.md"), base).unwrap();

    let registered = daemon
        .command()
        .args(["kiln", "register", "notes"])
        .arg(&kiln)
        .output()
        .expect("run cru kiln register");
    assert!(registered.status.success(), "{}", stderr_of(&registered));

    let id = store_proposal(&daemon, &kiln, base, new_text);

    let listed = daemon
        .command()
        .args(["proposal", "list"])
        .output()
        .expect("run cru proposal list");
    assert!(listed.status.success(), "{}", stderr_of(&listed));
    assert!(
        stdout_of(&listed).contains(&format!("{id}  open")),
        "the list names the proposal: {}",
        stdout_of(&listed)
    );

    let accepted = daemon
        .command()
        .args(["proposal", "accept"])
        .arg(id.to_string())
        .output()
        .expect("run cru proposal accept");
    assert!(
        accepted.status.success(),
        "cru proposal accept failed:\nstdout: {}\nstderr: {}",
        stdout_of(&accepted),
        stderr_of(&accepted)
    );
    assert!(
        stdout_of(&accepted).contains("the daemon wrote 1 file"),
        "{}",
        stdout_of(&accepted)
    );
    assert_eq!(
        std::fs::read_to_string(kiln.join("links.md")).unwrap(),
        new_text,
        "the accept wrote the note"
    );

    // The accepted proposal left the Inbox.
    let listed = daemon
        .command()
        .args(["proposal", "list"])
        .output()
        .expect("run cru proposal list");
    assert!(
        !stdout_of(&listed).contains(&id.to_string()),
        "{}",
        stdout_of(&listed)
    );
}
