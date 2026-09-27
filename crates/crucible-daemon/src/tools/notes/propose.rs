//! Where the note writes of one session go: to the disk, or to a proposal.
//!
//! The write mode belongs to the mode of the session, and the turn reads it
//! when it starts. The session slot holds the value, and the note tools read
//! it at each write. A mode change during a turn does not change the turn,
//! as for the other mode rules.

use std::sync::{Arc, RwLock};

use crucible_core::file_write::ExpectedBase;
use crucible_core::proposal::{Proposal, ProposalAuthor};
use crucible_core::session::{PhysicalRoot, Session, SessionId, SessionType};
use crucible_core::types::WriteMode;

use crate::proposals::ProposalStore;

/// The write mode of the current turn of one session.
///
/// The turn start writes it, and the note tools of the session read it. A
/// new cell holds [`WriteMode::Apply`], the behavior of a session that ran no
/// turn yet.
#[derive(Debug, Clone, Default)]
pub struct TurnWriteMode(Arc<RwLock<WriteMode>>);

impl TurnWriteMode {
    /// The write mode of the current turn.
    pub fn get(&self) -> WriteMode {
        *self.0.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Set the write mode for the turn that starts now.
    pub fn set(&self, mode: WriteMode) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = mode;
    }
}

/// The note-write target of one session: the write mode of its turn, and
/// the proposal store that a `propose` write goes to.
#[derive(Debug, Clone)]
pub struct NoteWrites {
    mode: TurnWriteMode,
    proposals: Arc<ProposalStore>,
    session: SessionId,
    author: ProposalAuthor,
}

impl NoteWrites {
    /// The note-write target of `session`.
    pub fn new(mode: TurnWriteMode, proposals: Arc<ProposalStore>, session: &Session) -> Self {
        Self {
            mode,
            proposals,
            session: session.id.clone(),
            author: author_of(session),
        }
    }

    /// The effective write mode of the current turn.
    pub fn mode(&self) -> WriteMode {
        self.mode.get()
    }

    /// The text that the current turn already proposes for `root`/`path`,
    /// or `None` when the turn proposes no write of that path.
    pub(crate) fn proposed_text(
        &self,
        root: &PhysicalRoot,
        path: &str,
    ) -> Result<Option<String>, rmcp::ErrorData> {
        self.proposals
            .turn_text(&self.author, &self.session, root, path)
            .map_err(|e| {
                rmcp::ErrorData::internal_error(format!("Failed to read the proposal: {e}"), None)
            })
    }

    /// Record one proposed write. The disk does not change.
    pub(crate) fn propose(
        &self,
        root: PhysicalRoot,
        path: &str,
        base: ExpectedBase,
        new_text: String,
    ) -> Result<Proposal, rmcp::ErrorData> {
        self.proposals
            .record_write(
                self.author.clone(),
                &self.session,
                root,
                path,
                base,
                new_text,
            )
            .map_err(|e| {
                rmcp::ErrorData::internal_error(format!("Failed to record the proposal: {e}"), None)
            })
    }
}

/// The author of a proposal that `session` makes.
///
/// A plugin pass is a new session each time, so a session author never
/// matches the proposal of an earlier pass. The plugin name does, so a newer
/// pass supersedes the older proposal. A plugin session with no plugin name
/// names itself.
pub fn author_of(session: &Session) -> ProposalAuthor {
    match (&session.session_type, &session.plugin) {
        (SessionType::Plugin, Some(name)) => ProposalAuthor::Plugin { name: name.clone() },
        _ => ProposalAuthor::Session {
            id: session.id.clone(),
        },
    }
}
