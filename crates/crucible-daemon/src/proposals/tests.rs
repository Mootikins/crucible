use super::*;
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
    let (tx, mut events) = tokio::sync::broadcast::channel(16);
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
    let (tx, mut events) = tokio::sync::broadcast::channel(16);
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
