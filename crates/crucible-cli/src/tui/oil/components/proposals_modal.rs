//! Full-screen list of the proposals in the Inbox, for `:proposals`.
//!
//! The list shows one row for each proposal. Enter opens the diff of the
//! proposal in a [`DiffModal`]. The proposal holds the base text and the new
//! text of each file, so the diff needs no second request to the daemon.
//! Escape in the diff goes back to the list; escape in the list closes it.
//!
//! The decisions stay in `cru proposal`, which the footer names. A decision
//! needs a reason or a settled text, and this view has no text input.

use crossterm::event::{KeyCode, KeyEvent};
use crucible_core::proposal::{Proposal, ProposalId};
use crucible_oil::node::{col, row, spacer, styled, Node};
use crucible_oil::style::{Color, Style};

use super::diff_modal::{DiffModal, DiffModalOutcome};
use crate::commands::proposal::{author_label, count_noun, proposal_diffset, state_label};

/// The rows that are not list rows: the header and the footer.
const CHROME_ROWS: usize = 2;

/// What a key did, for the app to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalsModalOutcome {
    /// The key moved the view or did nothing. Stay open.
    Handled,
    /// Close the modal.
    Close,
}

/// The proposals in the Inbox, open full-screen.
#[derive(Debug, Clone)]
pub struct ProposalsModal {
    proposals: Vec<Proposal>,
    cursor: usize,
    /// The diff of the proposal that the user opened, with its id.
    diff: Option<(ProposalId, DiffModal)>,
}

impl ProposalsModal {
    #[must_use]
    pub fn new(proposals: Vec<Proposal>) -> Self {
        Self {
            proposals,
            cursor: 0,
            diff: None,
        }
    }

    /// The row under the cursor.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The id of the proposal whose diff is open.
    #[must_use]
    pub fn open_diff(&self) -> Option<&ProposalId> {
        self.diff.as_ref().map(|(id, _)| id)
    }

    /// Replace the list after a refetch.
    ///
    /// The cursor stays on the same proposal. A diff stays open while its
    /// proposal is in the list; a proposal that left the Inbox closes it.
    pub fn update(&mut self, proposals: Vec<Proposal>) {
        let under_cursor = self.proposals.get(self.cursor).map(|p| p.id);
        self.proposals = proposals;
        self.cursor = under_cursor
            .and_then(|id| self.proposals.iter().position(|p| p.id == id))
            .unwrap_or(self.cursor)
            .min(self.proposals.len().saturating_sub(1));
        if let Some((id, _)) = &self.diff {
            if !self.proposals.iter().any(|p| p.id == *id) {
                self.diff = None;
            }
        }
    }

    fn open_selected(&mut self) {
        let Some(proposal) = self.proposals.get(self.cursor) else {
            return;
        };
        let (diffset, texts) = proposal_diffset(proposal);
        let modal = DiffModal::with_texts(diffset, texts).titled(format!(
            "{} ({})",
            proposal.title,
            state_label(&proposal.state)
        ));
        self.diff = Some((proposal.id, modal));
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ProposalsModalOutcome {
        if let Some((_, diff)) = self.diff.as_mut() {
            match diff.handle_key(key) {
                DiffModalOutcome::Close => self.diff = None,
                // Each text of a proposal is loaded when the view opens. A
                // file above the size limit has no text and asks for none.
                DiffModalOutcome::Handled | DiffModalOutcome::Load(_) => {}
            }
            return ProposalsModalOutcome::Handled;
        }
        let last = self.proposals.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return ProposalsModalOutcome::Close,
            KeyCode::Down | KeyCode::Char('j') => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Up | KeyCode::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => self.cursor = last,
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.open_selected(),
            _ => {}
        }
        ProposalsModalOutcome::Handled
    }

    pub fn view(&self, term_width: usize, term_height: usize) -> Node {
        if let Some((_, diff)) = &self.diff {
            return diff.view(term_width, term_height);
        }
        let t = crate::tui::oil::theme::active();
        let header_text = format!(
            " Proposals  {} ",
            count_noun(self.proposals.len(), "proposal", "proposals")
        );
        let header_padding = " ".repeat(term_width.saturating_sub(header_text.chars().count()));
        let header = styled(
            format!("{header_text}{header_padding}"),
            Style::new().bg(t.resolve_color(t.colors.popup_bg)).bold(),
        );

        let height = term_height.saturating_sub(CHROME_ROWS).max(1);
        let first = self.cursor.saturating_sub(height - 1);
        let body: Vec<Node> = if self.proposals.is_empty() {
            vec![styled(
                " no proposals wait for a decision ".to_string(),
                Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
            )]
        } else {
            self.proposals
                .iter()
                .enumerate()
                .skip(first)
                .take(height)
                .map(|(index, p)| self.row_view(p, index == self.cursor, t))
                .collect()
        };

        let footer = self.footer(t.resolve_color(t.colors.background), t);
        col([header, col(body), spacer(), footer])
    }

    fn row_view(
        &self,
        proposal: &Proposal,
        selected: bool,
        t: &crate::tui::oil::theme::ThemeConfig,
    ) -> Node {
        let label_style = if selected {
            Style::new().fg(t.resolve_color(t.colors.primary)).bold()
        } else {
            Style::new().fg(t.resolve_color(t.colors.text))
        };
        let muted = Style::new().fg(t.resolve_color(t.colors.text_muted)).dim();
        let (diffset, _) = proposal_diffset(proposal);
        let added: u32 = diffset.files.iter().map(|f| f.added).sum();
        let removed: u32 = diffset.files.iter().map(|f| f.removed).sum();
        row([
            styled(if selected { "› " } else { "  " }.to_string(), label_style),
            styled(format!("{:<10} ", state_label(&proposal.state)), muted),
            styled(proposal.title.clone(), label_style),
            styled(
                format!(
                    "  {} · {} · +{added} −{removed}",
                    author_label(&proposal.author),
                    count_noun(proposal.writes.len(), "file", "files")
                ),
                muted,
            ),
        ])
    }

    fn footer(&self, bg: Color, t: &crate::tui::oil::theme::ThemeConfig) -> Node {
        let key_style = Style::new().bg(bg).fg(t.resolve_color(t.colors.primary));
        let text_style = Style::new().bg(bg).fg(t.resolve_color(t.colors.text)).dim();
        row([
            styled(" j/k".to_string(), key_style),
            styled(" move  ".to_string(), text_style),
            styled("enter".to_string(), key_style),
            styled(" diff  ".to_string(), text_style),
            styled("esc".to_string(), key_style),
            styled(" close  ".to_string(), text_style),
            styled(
                "decide with `cru proposal accept|reject|dismiss <id>`".to_string(),
                text_style,
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use crucible_core::file_write::ExpectedBase;
    use crucible_core::proposal::{ProposalAuthor, ProposalState, ProposedWrite};
    use crucible_core::session::PhysicalRoot;
    use crucible_oil::render::render_to_plain_text;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn proposal(title: &str, state: ProposalState) -> Proposal {
        Proposal {
            id: ProposalId::generate(),
            author: ProposalAuthor::Plugin {
                name: "reflection".into(),
            },
            session: None,
            title: title.into(),
            rationale: None,
            created_at: chrono::Utc::now(),
            state,
            writes: vec![ProposedWrite {
                root: PhysicalRoot::from_top_level("/kiln"),
                path: "links.md".into(),
                base: ExpectedBase::Text {
                    text: "one\n".into(),
                    hash: String::new(),
                },
                new_text: "one\ntwo\n".into(),
            }],
        }
    }

    #[test]
    fn the_list_shows_each_proposal_with_its_state_and_counts() {
        let modal = ProposalsModal::new(vec![
            proposal("Link the two notes", ProposalState::Open),
            proposal("Fix a date", ProposalState::Stale),
        ]);
        let out = render_to_plain_text(&modal.view(100, 10), 100);
        assert!(out.contains("Proposals  2 proposals"), "{out}");
        assert!(
            out.contains("› open"),
            "the cursor is on the first row: {out}"
        );
        assert!(out.contains("Link the two notes"), "{out}");
        assert!(out.contains("stale"), "{out}");
        assert!(out.contains("reflection · 1 file · +1 −0"), "{out}");
    }

    #[test]
    fn enter_opens_the_proposal_diff_and_escape_goes_back_to_the_list() {
        let mut modal = ProposalsModal::new(vec![
            proposal("First", ProposalState::Open),
            proposal("Second", ProposalState::Open),
        ]);
        modal.handle_key(key(KeyCode::Char('j')));
        assert_eq!(modal.cursor(), 1);
        modal.handle_key(key(KeyCode::Enter));
        assert!(modal.open_diff().is_some());

        // The diff draws the texts that the proposal holds, with no request.
        let out = render_to_plain_text(&modal.view(80, 20), 80);
        assert!(out.contains("Diff: Second (open)"), "{out}");
        assert!(out.contains("two"), "the new line: {out}");
        assert!(!out.contains("loading"), "{out}");

        assert_eq!(
            modal.handle_key(key(KeyCode::Esc)),
            ProposalsModalOutcome::Handled
        );
        assert!(modal.open_diff().is_none(), "back to the list");
        assert_eq!(
            modal.handle_key(key(KeyCode::Esc)),
            ProposalsModalOutcome::Close
        );
    }

    #[test]
    fn a_refetch_keeps_the_cursor_and_closes_the_diff_of_a_proposal_that_left() {
        let first = proposal("First", ProposalState::Open);
        let second = proposal("Second", ProposalState::Open);
        let mut modal = ProposalsModal::new(vec![first.clone(), second.clone()]);
        modal.handle_key(key(KeyCode::Char('j')));
        modal.handle_key(key(KeyCode::Enter));

        // A new proposal at the top moves the second row down by one.
        let newer = proposal("Newer", ProposalState::Open);
        modal.update(vec![newer.clone(), first.clone(), second.clone()]);
        assert_eq!(modal.cursor(), 2, "the cursor stays on Second");
        assert_eq!(modal.open_diff(), Some(&second.id));

        // Second was accepted, so it left the Inbox.
        modal.update(vec![newer, first]);
        assert!(modal.open_diff().is_none());
        assert_eq!(modal.cursor(), 1);
    }
}
