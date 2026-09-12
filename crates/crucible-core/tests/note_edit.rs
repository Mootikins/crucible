//! The rules an anchored edit must keep. Each one is a way a note gets
//! corrupted or a user loses work, written down as a test.

use crucible_core::note_edit::{apply_anchored_edits, AnchoredEdit, EditOutcome, EditRefusal};

fn edit(expect: &str, replace: &str) -> AnchoredEdit {
    AnchoredEdit { expect: expect.into(), replace: replace.into(), occurrence: None }
}

fn at(expect: &str, replace: &str, occurrence: usize) -> AnchoredEdit {
    AnchoredEdit { expect: expect.into(), replace: replace.into(), occurrence: Some(occurrence) }
}

fn applied(original: &str, edits: &[AnchoredEdit]) -> String {
    match apply_anchored_edits(original, edits) {
        EditOutcome::Applied(text) => text,
        EditOutcome::Refused(why) => panic!("refused: {why:?}"),
    }
}

fn refused(original: &str, edits: &[AnchoredEdit]) -> Vec<EditRefusal> {
    match apply_anchored_edits(original, edits) {
        EditOutcome::Applied(text) => panic!("applied unexpectedly: {text:?}"),
        EditOutcome::Refused(why) => why,
    }
}

const TICKET: &str = "---\nstatus: todo\nupdated: 09-01\n---\n\n# Ship it\n\nA body a human wrote.\n";

#[test]
fn changes_only_the_anchored_line() {
    let out = applied(TICKET, &[edit("status: todo", "status: doing")]);
    assert_eq!(
        out,
        "---\nstatus: doing\nupdated: 09-01\n---\n\n# Ship it\n\nA body a human wrote.\n"
    );
}

#[test]
fn applies_a_batch_together() {
    let out = applied(
        TICKET,
        &[edit("status: todo", "status: doing"), edit("updated: 09-01", "updated: 09-11")],
    );
    assert!(out.contains("status: doing"));
    assert!(out.contains("updated: 09-11"));
}

/// A half-applied batch is the failure a user cannot see.
#[test]
fn one_bad_edit_refuses_the_whole_batch() {
    let why = refused(
        TICKET,
        &[edit("status: todo", "status: doing"), edit("nothing like this", "x")],
    );
    assert_eq!(why, vec![EditRefusal::NotFound { index: 1 }]);
}

#[test]
fn matches_whole_lines_only() {
    // `status: todo` is a prefix of `status: todoish`, and must not take it.
    let note = "---\nstatus: todoish\n---\n";
    assert_eq!(refused(note, &[edit("status: todo", "status: doing")]), vec![EditRefusal::NotFound { index: 0 }]);
}

/// Prose about a value is not the value. Rewriting inside a fence edits the
/// documentation and leaves the real line alone.
#[test]
fn never_matches_inside_a_code_fence() {
    let note = "---\nstatus: doing\n---\n\n```yaml\nstatus: todo\n```\n";
    assert_eq!(refused(note, &[edit("status: todo", "status: done")]), vec![EditRefusal::NotFound { index: 0 }]);
}

#[test]
fn refuses_an_ambiguous_anchor() {
    let note = "- [ ] Buy milk\n- [ ] Buy milk\n";
    assert_eq!(
        refused(note, &[edit("- [ ] Buy milk", "- [x] Buy milk")]),
        vec![EditRefusal::Ambiguous { index: 0, matches: 2 }]
    );
}

/// The named use case: two identical checkbox lines, and the user tapped one.
#[test]
fn an_occurrence_picks_between_identical_lines() {
    let note = "- [ ] Buy milk\n- [ ] Buy milk\n";
    assert_eq!(applied(note, &[at("- [ ] Buy milk", "- [x] Buy milk", 1)]), "- [ ] Buy milk\n- [x] Buy milk\n");
}

#[test]
fn refuses_an_occurrence_the_file_does_not_have() {
    let note = "- [ ] Buy milk\n";
    assert_eq!(
        refused(note, &[at("- [ ] Buy milk", "- [x] Buy milk", 3)]),
        vec![EditRefusal::NoSuchOccurrence { index: 0, matches: 1 }]
    );
}

#[test]
fn one_line_may_become_several() {
    let note = "- [ ] Water the plants\n";
    let out = applied(note, &[edit("- [ ] Water the plants", "- [x] Water the plants\n- [ ] Water the plants")]);
    assert_eq!(out, "- [x] Water the plants\n- [ ] Water the plants\n");
}

#[test]
fn keeps_the_file_s_own_line_endings() {
    let note = "---\r\nstatus: todo\r\n---\r\n";
    let out = applied(note, &[edit("status: todo", "status: doing\nnote: added")]);
    assert_eq!(out, "---\r\nstatus: doing\r\nnote: added\r\n---\r\n");
    assert!(!out.contains("doing\nnote"), "a lone LF would split the file's endings");
}

#[test]
fn matches_a_multi_line_anchor_across_crlf() {
    let note = "alpha\r\nbeta\r\ngamma\r\n";
    assert_eq!(applied(note, &[edit("alpha\nbeta", "one\ntwo")]), "one\r\ntwo\r\ngamma\r\n");
}

/// Trailing whitespace is content: `status: doing ` is not `status: doing`.
#[test]
fn trailing_whitespace_is_significant() {
    let note = "status: doing \n";
    assert_eq!(refused(note, &[edit("status: doing", "status: done")]), vec![EditRefusal::NotFound { index: 0 }]);
}

/// The normal offline success path, not a conflict: another device already
/// wrote this change, so the edit has nothing left to do.
#[test]
fn an_edit_already_applied_succeeds_unchanged() {
    let note = "---\nstatus: doing\n---\n";
    assert_eq!(applied(note, &[edit("status: todo", "status: doing")]), note);
}

#[test]
fn refuses_two_edits_that_cover_the_same_lines() {
    let note = "alpha\nbeta\n";
    let why = refused(note, &[edit("alpha\nbeta", "one"), edit("beta", "two")]);
    assert_eq!(why, vec![EditRefusal::Overlaps { index: 0, other: 1 }], "got {why:?}");
}

/// The index a refusal reports is the CALLER's edit index.
///
/// An edit that is already applied is a success and contributes no span, so
/// indexing the span list reported the wrong edits to a caller who has no way
/// to see the span list. Here edit 0 is already applied, and the overlap is
/// between edits 1 and 2 — not 0 and 1.
#[test]
fn an_overlap_names_the_caller_s_edit_indices_past_a_skipped_edit() {
    let note = "done\nalpha\nbeta\n";
    let why = refused(
        note,
        &[
            edit("todo", "done"), // already applied: no span
            edit("alpha\nbeta", "one"),
            edit("beta", "two"),
        ],
    );
    assert_eq!(why, vec![EditRefusal::Overlaps { index: 1, other: 2 }], "got {why:?}");
}

/// Resolving against the ORIGINAL is what stops an edit matching text the
/// batch itself wrote — a match against a document that never existed.
#[test]
fn an_edit_never_matches_what_the_batch_wrote() {
    let note = "one\ntwo\n";
    let why = refused(note, &[edit("one", "three"), edit("three", "four")]);
    assert_eq!(why, vec![EditRefusal::NotFound { index: 1 }]);
}

#[test]
fn refuses_an_empty_anchor() {
    assert_eq!(refused("anything\n", &[edit("", "x")]), vec![EditRefusal::EmptyExpect { index: 0 }]);
}

#[test]
fn reports_every_refusal_in_the_batch() {
    let why = refused(TICKET, &[edit("missing one", "a"), edit("missing two", "b")]);
    assert_eq!(why, vec![EditRefusal::NotFound { index: 0 }, EditRefusal::NotFound { index: 1 }]);
}
