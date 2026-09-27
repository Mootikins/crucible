use super::*;
use std::sync::Arc;
use tempfile::TempDir;

fn session(id: &str) -> SessionId {
    SessionId::parse(id).unwrap()
}

fn plugin(name: &str) -> ProposalAuthor {
    ProposalAuthor::Plugin { name: name.into() }
}

fn root() -> PhysicalRoot {
    PhysicalRoot::from_top_level("/kiln")
}

fn text_base(text: &str) -> ExpectedBase {
    ExpectedBase::Text {
        text: text.into(),
        hash: format!("hash-of-{text}"),
    }
}

struct Fixture {
    dir: TempDir,
    store: ProposalStore,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let store = ProposalStore::new(proposals_root(dir.path()));
        Self { dir, store }
    }

    /// A second store over the same files, as a restarted daemon sees them.
    fn reopened(&self) -> ProposalStore {
        ProposalStore::new(proposals_root(self.dir.path()))
    }

    fn file_of(&self, id: &ProposalId) -> PathBuf {
        proposals_root(self.dir.path()).join(format!("{id}.json"))
    }

    fn write(&self, author: ProposalAuthor, session_id: &str, path: &str, text: &str) -> Proposal {
        self.store
            .record_write(
                author,
                &session(session_id),
                root(),
                path,
                text_base("old"),
                text.into(),
            )
            .unwrap()
    }
}

#[test]
fn a_proposal_round_trips_through_its_file() {
    let fx = Fixture::new();
    let made = fx
        .store
        .record_write(
            ProposalAuthor::Session {
                id: session("chat-1"),
            },
            &session("chat-1"),
            root(),
            "notes/a.md",
            ExpectedBase::Absent,
            "new text\n".into(),
        )
        .unwrap();

    assert!(fx.file_of(&made.id).is_file());
    let read = fx.reopened().get(&made.id).unwrap();
    assert_eq!(read, made);
    assert_eq!(read.state, ProposalState::Open);
    assert_eq!(read.session, Some(session("chat-1")));
    assert_eq!(read.title, "Change notes/a.md");
    assert_eq!(
        read.writes,
        vec![ProposedWrite {
            root: root(),
            path: "notes/a.md".into(),
            base: ExpectedBase::Absent,
            new_text: "new text\n".into(),
            remove: false,
            moved_from: None,
        }]
    );
    assert_eq!(fx.reopened().list(false).unwrap(), vec![made]);
}

#[test]
fn a_second_write_extends_the_turn_proposal() {
    let fx = Fixture::new();
    let first = fx.write(plugin("reflection"), "aux-1", "a.md", "a1");
    let second = fx.write(plugin("reflection"), "aux-1", "b.md", "b1");
    let third = fx.write(plugin("reflection"), "aux-1", "a.md", "a2");

    assert_eq!(first.id, second.id);
    assert_eq!(second.id, third.id);
    let stored = fx.store.get(&first.id).unwrap();
    assert_eq!(stored.state, ProposalState::Open);
    assert_eq!(stored.title, "Change 2 notes");
    let texts: Vec<_> = stored
        .writes
        .iter()
        .map(|w| (w.path.as_str(), w.new_text.as_str(), &w.base))
        .collect();
    // The second write of `a.md` keeps the first base.
    assert_eq!(
        texts,
        vec![
            ("a.md", "a2", &text_base("old")),
            ("b.md", "b1", &text_base("old"))
        ]
    );
    assert_eq!(fx.store.list(false).unwrap().len(), 1);

    // A new turn starts a new proposal.
    fx.store.end_turn(&session("aux-1"));
    let next = fx.write(plugin("reflection"), "aux-1", "c.md", "c1");
    assert_ne!(next.id, first.id);
}

#[test]
fn a_new_proposal_supersedes_an_older_one_for_the_same_path() {
    let fx = Fixture::new();
    let older = fx.write(plugin("reflection"), "aux-1", "a.md", "a1");
    // Another author and another path do not supersede.
    let other_author = fx.write(plugin("consolidation"), "aux-2", "a.md", "x");
    let other_path = fx.write(plugin("reflection"), "aux-3", "b.md", "b");
    // Each pass is a new session, so the author, not the session, matches.
    let newer = fx.write(plugin("reflection"), "aux-4", "a.md", "a2");

    assert_ne!(older.id, newer.id);
    assert_eq!(
        fx.store.get(&older.id).unwrap().state,
        ProposalState::Superseded { by: newer.id }
    );
    assert_eq!(
        fx.store.get(&other_author.id).unwrap().state,
        ProposalState::Open
    );
    assert_eq!(
        fx.store.get(&other_path.id).unwrap().state,
        ProposalState::Open
    );
    assert_eq!(fx.store.get(&newer.id).unwrap().state, ProposalState::Open);
}

#[test]
fn a_rejected_proposal_keeps_its_reason() {
    let fx = Fixture::new();
    let made = fx.write(plugin("reflection"), "aux-1", "a.md", "a1");

    let rejected = fx
        .store
        .reject(&made.id, Some("not a real pattern".into()))
        .unwrap();

    let expected = ProposalState::Rejected {
        reason: Some("not a real pattern".into()),
    };
    assert_eq!(rejected.state, expected);
    assert_eq!(fx.reopened().get(&made.id).unwrap().state, expected);
    assert!(fx.store.list(false).unwrap().is_empty());
    assert_eq!(fx.store.list(true).unwrap(), vec![rejected]);
    // A decided proposal cannot be decided again.
    assert!(matches!(
        fx.store.dismiss(&made.id),
        Err(ProposalError::Settled(_, "rejected"))
    ));
    assert!(matches!(
        fx.store.reject(&ProposalId::generate(), None),
        Err(ProposalError::NotFound(_))
    ));
}

#[test]
fn a_superseded_proposal_stays_listed_until_dismissed() {
    let fx = Fixture::new();
    let older = fx.write(plugin("reflection"), "aux-1", "a.md", "a1");
    let newer = fx.write(plugin("reflection"), "aux-2", "a.md", "a2");

    let listed: Vec<_> = fx
        .store
        .list(false)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(listed, vec![older.id, newer.id]);

    fx.store.dismiss(&older.id).unwrap();
    let listed: Vec<_> = fx
        .store
        .list(false)
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(listed, vec![newer.id]);
}

#[test]
fn a_dismissed_proposal_file_is_kept() {
    let fx = Fixture::new();
    let made = fx.write(plugin("reflection"), "aux-1", "a.md", "a1");

    let dismissed = fx.store.dismiss(&made.id).unwrap();

    assert_eq!(dismissed.state, ProposalState::Dismissed);
    assert!(fx.file_of(&made.id).is_file());
    assert_eq!(fx.reopened().get(&made.id).unwrap(), dismissed);
    assert!(fx.store.list(false).unwrap().is_empty());
    // A dismissed turn proposal is not extended by a later write.
    let next = fx.write(plugin("reflection"), "aux-1", "a.md", "a2");
    assert_ne!(next.id, made.id);
}

/// Take every event that the store sent.
fn drain(
    events: &mut tokio::sync::broadcast::Receiver<crucible_core::protocol::SessionEventMessage>,
) -> Vec<crucible_core::protocol::SessionEventMessage> {
    let mut out = Vec::new();
    while let Ok(event) = events.try_recv() {
        out.push(event);
    }
    out
}

/// The id that one `proposal_changed` event names.
fn changed_id(event: &crucible_core::protocol::SessionEventMessage) -> String {
    assert_eq!(
        event.session_id,
        crate::event_map::SYSTEM_SESSION,
        "{event:?}"
    );
    assert_eq!(
        event.event,
        crucible_core::protocol::SystemPayload::PROPOSAL_CHANGED
    );
    event.data["id"].as_str().expect("an id").to_string()
}

#[test]
fn a_reject_emits_proposal_changed_on_the_system_channel() {
    let fx = Fixture::new();
    let (tx, mut events) = crate::EventBus::channel(16);
    fx.store.set_events(tx);
    let made = fx.write(plugin("reflection"), "aux-1", "a.md", "a1");
    let written: Vec<_> = drain(&mut events).iter().map(changed_id).collect();
    assert_eq!(written, vec![made.id.to_string()]);

    fx.store.reject(&made.id, None).unwrap();

    let rejected: Vec<_> = drain(&mut events).iter().map(changed_id).collect();
    assert_eq!(rejected, vec![made.id.to_string()]);
    // A refused decision changes nothing, so the store sends nothing.
    assert!(fx.store.dismiss(&made.id).is_err());
    assert!(drain(&mut events).is_empty());
}

#[test]
fn a_supersede_emits_proposal_changed_for_both_proposals() {
    let fx = Fixture::new();
    let (tx, mut events) = crate::EventBus::channel(16);
    fx.store.set_events(tx);
    let older = fx.write(plugin("reflection"), "aux-1", "a.md", "a1");
    drain(&mut events);

    let newer = fx.write(plugin("reflection"), "aux-2", "a.md", "a2");

    let mut changed: Vec<_> = drain(&mut events).iter().map(changed_id).collect();
    changed.sort();
    let mut expected = vec![older.id.to_string(), newer.id.to_string()];
    expected.sort();
    assert_eq!(changed, expected);
}

/// A kiln directory in the fixture, with `a.md` on disk, and a proposal
/// that changes the first line of `a.md`.
fn kiln_proposal(fx: &Fixture) -> (PathBuf, Proposal) {
    let kiln = fx.dir.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();
    let base = "one\ntwo\n";
    std::fs::write(kiln.join("a.md"), base).unwrap();
    let proposal = fx
        .store
        .record_write(
            plugin("reflection"),
            &session("aux-1"),
            PhysicalRoot::from_top_level(&kiln),
            "a.md",
            ExpectedBase::Text {
                text: base.into(),
                hash: crucible_core::note_edit::disk_hash(base),
            },
            "uno\ntwo\n".into(),
        )
        .unwrap();
    (kiln, proposal)
}

#[tokio::test]
async fn accept_and_resolve_emit_proposal_changed() {
    let fx = Fixture::new();
    let (tx, mut events) = crate::EventBus::channel(16);
    fx.store.set_events(tx);
    let (kiln, made) = kiln_proposal(&fx);
    std::fs::write(kiln.join("a.md"), "eins\ntwo\n").unwrap();
    drain(&mut events);

    let conflicted = fx
        .store
        .accept(&made.id, std::slice::from_ref(&kiln))
        .await
        .unwrap();

    assert!(matches!(conflicted.state, ProposalState::Conflicted { .. }));
    let changed: Vec<_> = drain(&mut events).iter().map(changed_id).collect();
    // Settlement owns the proposal and announces its checked result directly.
    assert_eq!(changed, vec![made.id.to_string()]);

    let resolved = fx
        .store
        .resolve(&made.id, "a.md", "uno\ntwo\n", std::slice::from_ref(&kiln))
        .await
        .unwrap();

    assert_eq!(resolved.state, ProposalState::Accepted);
    let changed: Vec<_> = drain(&mut events).iter().map(changed_id).collect();
    assert_eq!(changed, vec![made.id.to_string()]);
    assert_eq!(
        std::fs::read_to_string(kiln.join("a.md")).unwrap(),
        "uno\ntwo\n"
    );
}

#[tokio::test]
async fn a_file_event_on_a_proposed_path_makes_the_proposal_stale() {
    let fx = Fixture::new();
    let (tx, mut events) = crate::EventBus::channel(16);
    fx.store.set_events(tx.clone());
    let (kiln, made) = kiln_proposal(&fx);
    drain(&mut events);
    let store = Arc::new(ProposalStore::new(proposals_root(fx.dir.path())));
    store.set_events(tx.clone());
    spawn_stale_watch(tx.subscribe(), store);

    std::fs::write(kiln.join("a.md"), "one\ntwo\nthree\n").unwrap();
    let event = crate::event_map::message_for(
        &crucible_core::events::session_event::InternalSessionEvent::FileChanged {
            path: kiln.join("a.md"),
            kind: Default::default(),
        },
    );
    assert!(tx.emit(event));

    // The watcher announces the change after it writes the state.
    let announced = loop {
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .expect("the watcher announces the stale proposal")
            .unwrap();
        if event.event == crucible_core::protocol::SystemPayload::PROPOSAL_CHANGED {
            break event;
        }
    };
    assert_eq!(changed_id(&announced), made.id.to_string());
    assert_eq!(fx.store.get(&made.id).unwrap().state, ProposalState::Stale);
}

/// Pause acceptance at a participating writer's lock, after it read the
/// proposal. Polling the future makes the interleaving deterministic.
fn pending_acceptance() -> (Fixture, PathBuf, Proposal) {
    let fx = Fixture::new();
    let kiln = fx.dir.path().join("kiln");
    std::fs::create_dir(&kiln).unwrap();
    let proposal = fx
        .store
        .record_write(
            plugin("reflection"),
            &session("aux-1"),
            PhysicalRoot::from_top_level(&kiln),
            "a.md",
            ExpectedBase::Absent,
            "first\n".into(),
        )
        .unwrap();
    (fx, kiln, proposal)
}

#[tokio::test]
async fn accepting_a_proposal_preserves_a_successful_concurrent_write() {
    let (fx, kiln, proposal) = pending_acceptance();
    let guard = crate::file_write::lock(&kiln.join("a.md")).await;
    let roots = vec![kiln.clone()];
    let accepting = fx.store.accept(&proposal.id, &roots);
    tokio::pin!(accepting);
    assert!(futures::poll!(&mut accepting).is_pending());

    let recorded = fx.store.record_write(
        plugin("reflection"),
        &session("aux-1"),
        PhysicalRoot::from_top_level(&kiln),
        "b.md",
        ExpectedBase::Absent,
        "second\n".into(),
    );
    drop(guard);
    let accepted = accepting.await;

    // Refusing the competing operation is valid. A successful operation,
    // however, must survive the other writer's completion and a reopen.
    assert!(
        accepted.is_ok() || recorded.is_ok(),
        "neither operation succeeded"
    );
    if let Ok(recorded) = recorded {
        let stored = fx.reopened().get(&recorded.id).unwrap();
        assert!(
            stored
                .writes
                .iter()
                .any(|w| w.path == "b.md" && w.new_text == "second\n"),
            "a successful concurrent write vanished: {stored:?}"
        );
    }
}

#[tokio::test]
async fn accepting_and_rejecting_one_proposal_cannot_both_succeed() {
    let (fx, kiln, proposal) = pending_acceptance();
    let guard = crate::file_write::lock(&kiln.join("a.md")).await;
    let roots = vec![kiln.clone()];
    let accepting = fx.store.accept(&proposal.id, &roots);
    tokio::pin!(accepting);
    assert!(futures::poll!(&mut accepting).is_pending());

    let rejected = fx
        .store
        .reject(&proposal.id, Some("keep the file absent".into()));
    drop(guard);
    let accepted = accepting.await;
    let stored = fx.reopened().get(&proposal.id).unwrap();

    assert!(
        accepted.is_ok() ^ rejected.is_ok(),
        "exactly one decision must succeed: accept={accepted:?}, reject={rejected:?}, stored={stored:?}"
    );
    if rejected.is_ok() {
        assert!(matches!(stored.state, ProposalState::Rejected { .. }));
        assert!(
            !kiln.join("a.md").exists(),
            "a rejected proposal wrote its file"
        );
    } else {
        assert_eq!(stored.state, ProposalState::Accepted);
        assert_eq!(
            std::fs::read_to_string(kiln.join("a.md")).unwrap(),
            "first\n"
        );
    }
}

#[tokio::test]
async fn cancelling_accept_releases_its_reservation() {
    let (fx, kiln, proposal) = pending_acceptance();
    let guard = crate::file_write::lock(&kiln.join("a.md")).await;
    let roots = vec![kiln.clone()];
    {
        let accepting = fx.store.accept(&proposal.id, &roots);
        tokio::pin!(accepting);
        assert!(futures::poll!(&mut accepting).is_pending());
        assert!(matches!(
            fx.store.dismiss(&proposal.id),
            Err(ProposalError::Busy(_))
        ));
    }
    fx.store.reject(&proposal.id, None).unwrap();
    drop(guard);
    assert!(!kiln.join("a.md").exists());
}

/// A partial accept holds both halves. A decision refuses a second decision
/// on either half, but it never refuses a note write: the write of a newer
/// pass succeeds, and its supersede of the held half waits for the release.
/// This replaced a `Busy` answer to the write, which failed the tool call of
/// an agent or a plugin only because the user was deciding at that moment.
#[tokio::test]
async fn partial_accept_reserves_both_halves_and_a_supersede_waits_for_the_release() {
    let (fx, kiln, proposal) = pending_acceptance();
    fx.store
        .record_write(
            plugin("reflection"),
            &session("aux-1"),
            PhysicalRoot::from_top_level(&kiln),
            "b.md",
            ExpectedBase::Absent,
            "b\n".into(),
        )
        .unwrap();
    let guard = crate::file_write::lock(&kiln.join("a.md")).await;
    let roots = vec![kiln.clone()];
    let paths = vec!["a.md".to_string()];
    let (before, newer) = {
        let accepting = fx.store.accept_paths(&proposal.id, &paths, &roots);
        tokio::pin!(accepting);
        assert!(futures::poll!(&mut accepting).is_pending());
        let before = fx.store.list(true).unwrap();
        assert_eq!(before.len(), 2);
        for p in &before {
            assert!(matches!(
                fx.store.reject(&p.id, None),
                Err(ProposalError::Busy(_))
            ));
            assert!(matches!(
                fx.store.reject_paths(&p.id, &[], None),
                Err(ProposalError::Busy(_))
            ));
        }
        fx.store.end_turn(&session("aux-1"));
        let newer = fx
            .store
            .record_write(
                plugin("reflection"),
                &session("aux-2"),
                PhysicalRoot::from_top_level(&kiln),
                "a.md",
                ExpectedBase::Absent,
                "newer\n".into(),
            )
            .expect("a decision does not fail a note write");
        // The held halves do not change while the decision holds them.
        for p in &before {
            assert_eq!(fx.store.get(&p.id).unwrap(), *p);
        }
        assert_eq!(fx.store.list(true).unwrap().len(), 3);
        (before, newer)
        // The accept drops here, before it wrote: a cancel.
    };
    let split = before
        .iter()
        .find(|p| p.id != proposal.id)
        .expect("the selection moved into its own proposal");
    assert_eq!(
        fx.store.get(&split.id).unwrap().state,
        ProposalState::Superseded { by: newer.id },
        "the cancelled accept left the split pending, so the newer write superseded it"
    );
    assert_eq!(
        fx.store.get(&proposal.id).unwrap().state,
        ProposalState::Open
    );
    for p in fx.store.list(true).unwrap() {
        fx.store.dismiss(&p.id).unwrap();
    }
    drop(guard);
    assert!(!kiln.join("a.md").exists());
}

/// A write of the same turn while the user accepts the turn proposal starts
/// the next proposal of the turn. It never joins the proposal under decision,
/// and it keeps the first base of its path, because an update builds on the
/// text of the held proposal.
#[tokio::test]
async fn a_write_during_an_accept_of_its_turn_starts_the_next_proposal() {
    let (fx, kiln, proposal) = pending_acceptance();
    let guard = crate::file_write::lock(&kiln.join("a.md")).await;
    let roots = vec![kiln.clone()];
    let accepting = fx.store.accept(&proposal.id, &roots);
    tokio::pin!(accepting);
    assert!(futures::poll!(&mut accepting).is_pending());

    let root = PhysicalRoot::from_top_level(&kiln);
    let chained = fx
        .store
        .record_write(
            plugin("reflection"),
            &session("aux-1"),
            root.clone(),
            "a.md",
            ExpectedBase::Unchecked,
            "first\nsecond\n".into(),
        )
        .expect("a decision does not fail a note write");
    assert_ne!(chained.id, proposal.id);
    assert_eq!(chained.writes[0].base, ExpectedBase::Absent);
    let other = fx
        .store
        .record_write(
            plugin("reflection"),
            &session("aux-1"),
            root.clone(),
            "b.md",
            ExpectedBase::Absent,
            "b\n".into(),
        )
        .unwrap();
    assert_eq!(other.id, chained.id, "the next proposal is the turn now");
    assert_eq!(fx.store.get(&proposal.id).unwrap(), proposal);

    drop(guard);
    let accepted = accepting.await.unwrap();
    assert_eq!(accepted.state, ProposalState::Accepted);
    assert_eq!(accepted.writes, proposal.writes);
    assert_eq!(
        std::fs::read_to_string(kiln.join("a.md")).unwrap(),
        "first\n"
    );
    let next = fx.reopened().get(&chained.id).unwrap();
    assert!(next.state.is_pending(), "{next:?}");
    assert_eq!(next.writes.len(), 2);
}

#[tokio::test]
async fn resolve_reserves_its_proposal_and_write_errors_release_it() {
    let (fx, kiln, proposal) = pending_acceptance();
    std::fs::write(kiln.join("a.md"), "outside\n").unwrap();
    let roots = vec![kiln.clone()];
    let conflict = fx.store.accept(&proposal.id, &roots).await.unwrap();
    assert!(matches!(conflict.state, ProposalState::Conflicted { .. }));
    let guard = crate::file_write::lock(&kiln.join("a.md")).await;
    let resolving = fx.store.resolve(&proposal.id, "a.md", "settled\n", &roots);
    tokio::pin!(resolving);
    assert!(futures::poll!(&mut resolving).is_pending());
    assert!(matches!(
        fx.store.reject(&proposal.id, None),
        Err(ProposalError::Busy(_))
    ));
    assert!(fx.store.check_stale().unwrap().is_empty());
    drop(guard);
    assert_eq!(resolving.await.unwrap().state, ProposalState::Accepted);

    let (other, _, p) = pending_acceptance();
    assert!(matches!(
        other.store.accept(&p.id, &[]).await,
        Err(ProposalError::WriteFailed(_))
    ));
    other.store.reject(&p.id, None).unwrap();
}
