//! Accept, resolve and the stale check of a proposal, through a running daemon.
//!
//! Each test makes its proposal through a second `ProposalStore` over the
//! daemon's proposal directory. The files are the store, so the daemon reads
//! the same proposal.

use std::path::{Path, PathBuf};

use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::proposal::{Proposal, ProposalAuthor, ProposalState};
use crucible_core::session::{PhysicalRoot, SessionId};
use crucible_daemon::proposals::{proposals_root, ProposalStore};
use crucible_daemon::{DaemonClient, Server};

const BASE: &str = "one\ntwo\nthree\n";

struct Daemon {
    _dir: tempfile::TempDir,
    kiln: PathBuf,
    data: PathBuf,
    client: DaemonClient,
    shutdown: tokio::sync::broadcast::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Daemon {
    /// A daemon with one registered kiln. With `open`, the client opens it.
    async fn start(open: bool) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let kiln = dir.path().join("kiln");
        std::fs::create_dir(&kiln).unwrap();
        let data = dir.path().join("data");
        let socket = dir.path().join("daemon.sock");
        let server =
            Server::bind_with_data_home_and_kilns(&socket, data.clone(), &[("notes", &kiln)])
                .await
                .unwrap();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        let client = DaemonClient::connect_to(&socket).await.unwrap();
        if open {
            client.kiln_open(&kiln).await.unwrap();
        }
        Self {
            _dir: dir,
            kiln,
            data,
            client,
            shutdown,
            task,
        }
    }

    fn file(&self, path: &str) -> PathBuf {
        self.kiln.join(path)
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.file(path)).unwrap()
    }

    /// Propose `new_text` for each path, from the base `BASE`.
    fn propose(&self, writes: &[(&str, &str)]) -> Proposal {
        let store = ProposalStore::new(proposals_root(&self.data));
        let session = SessionId::parse("aux-proposals").unwrap();
        let mut proposal = None;
        for (path, new_text) in writes {
            proposal = Some(
                store
                    .record_write(
                        ProposalAuthor::Plugin {
                            name: "consolidation".into(),
                        },
                        &session,
                        PhysicalRoot::from_top_level(&self.kiln),
                        path,
                        text_base(BASE),
                        new_text.to_string(),
                    )
                    .unwrap(),
            );
        }
        proposal.expect("at least one write")
    }

    async fn stop(self) {
        drop(self.client);
        self.shutdown.send(()).unwrap();
        self.task.await.unwrap().unwrap();
    }
}

fn text_base(text: &str) -> ExpectedBase {
    ExpectedBase::Text {
        text: text.into(),
        hash: disk_hash(text),
    }
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn accept_on_an_unchanged_file_writes_it() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n")]);

    let accepted = daemon.client.proposal_accept(&proposal.id).await.unwrap();

    assert_eq!(accepted.state, ProposalState::Accepted);
    assert_eq!(daemon.read("a.md"), "ONE\ntwo\nthree\n");
    // The file keeps the state, so a second read agrees.
    let again = daemon.client.proposal_get(&proposal.id).await.unwrap();
    assert_eq!(again.state, ProposalState::Accepted);
    daemon.stop().await;
}

#[tokio::test]
async fn accept_after_an_outside_edit_merges_cleanly() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "one\ntwo\nTHREE\n");

    let accepted = daemon.client.proposal_accept(&proposal.id).await.unwrap();

    assert_eq!(accepted.state, ProposalState::Accepted);
    assert_eq!(daemon.read("a.md"), "ONE\ntwo\nTHREE\n");
    daemon.stop().await;
}

#[tokio::test]
async fn accept_with_a_conflict_writes_nothing_and_is_conflicted() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    write(&daemon.file("b.md"), BASE);
    // `a.md` has no outside edit and writes cleanly alone. `b.md` conflicts,
    // so the accept must not write `a.md` either.
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n"), ("b.md", "uno\ntwo\nthree\n")]);
    write(&daemon.file("b.md"), "eins\ntwo\nthree\n");

    let conflicted = daemon.client.proposal_accept(&proposal.id).await.unwrap();

    let ProposalState::Conflicted { files } = &conflicted.state else {
        panic!("expected a conflict, got {:?}", conflicted.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].path, "b.md");
    assert_eq!(files[0].disk_text, "eins\ntwo\nthree\n");
    assert!(!files[0].regions.is_empty());
    assert_eq!(daemon.read("a.md"), BASE);
    assert_eq!(daemon.read("b.md"), "eins\ntwo\nthree\n");
    daemon.stop().await;
}

#[tokio::test]
async fn resolve_writes_the_settled_text() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "uno\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "eins\ntwo\nthree\n");
    let conflicted = daemon.client.proposal_accept(&proposal.id).await.unwrap();
    assert!(matches!(conflicted.state, ProposalState::Conflicted { .. }));

    let resolved = daemon
        .client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    assert_eq!(resolved.state, ProposalState::Accepted);
    assert_eq!(daemon.read("a.md"), "uno eins\ntwo\nthree\n");
    daemon.stop().await;
}

#[tokio::test]
async fn resolve_after_another_move_stays_conflicted() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "uno\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "eins\ntwo\nthree\n");
    daemon.client.proposal_accept(&proposal.id).await.unwrap();
    // The disk moves again on the same line before the user settles the text.
    write(&daemon.file("a.md"), "one again\ntwo\nthree\n");

    let resolved = daemon
        .client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    let ProposalState::Conflicted { files } = &resolved.state else {
        panic!("expected a conflict, got {:?}", resolved.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].disk_text, "one again\ntwo\nthree\n");
    assert!(!files[0].regions.is_empty());
    assert_eq!(daemon.read("a.md"), "one again\ntwo\nthree\n");
    daemon.stop().await;
}

#[tokio::test]
async fn a_proposal_on_a_closed_kiln_goes_stale_at_list() {
    // The client does not open the kiln, so no watcher sees the edit.
    let daemon = Daemon::start(false).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "one\ntwo\nTHREE\n");

    let listed = daemon.client.proposal_list(false).await.unwrap();

    let row = listed.iter().find(|p| p.id == proposal.id).unwrap();
    assert_eq!(row.state, ProposalState::Stale);
    // The check writes the state, so a read of the one proposal agrees.
    let got = daemon.client.proposal_get(&proposal.id).await.unwrap();
    assert_eq!(got.state, ProposalState::Stale);
    daemon.stop().await;
}
