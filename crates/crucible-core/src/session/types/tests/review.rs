//! Review type tests. The identity rules here are load-bearing for the gate:
//! a hunk that changes identity under an unrelated edit loses its review
//! state, and a hunk that shares identity with another inherits a decision
//! the user never made about it.

use crate::session::types::{
    ChildLedgerRef, ComposedHunk, HunkId, Integrity, Interval, Ledger, LineRange, PhysicalRoot,
    ReviewState, RootBase, RootInterval, Skip, SkipKind, TreeSha, Verdict,
};
use std::path::Path;

fn hunk(id: HunkId, tool_call_ids: Vec<String>) -> ComposedHunk {
    ComposedHunk {
        id,
        root: PhysicalRoot::from_top_level("/repo"),
        path: "src/lib.rs".into(),
        base_range: LineRange::new(1, 2),
        current_range: LineRange::new(1, 2),
        before_content: "a\n".into(),
        after_content: "b\n".into(),
        tool_call_ids,
        state: ReviewState::Unreviewed,
        reapplied: false,
    }
}

#[test]
fn hunk_identity_includes_the_root() {
    // A session tracks several roots and hunk paths are root-relative, so the
    // same relative path in a workspace and a kiln is two different files.
    // Sharing an id would make accepting one silently accept the other.
    let workspace = HunkId::derive(
        &PhysicalRoot::from_top_level("/work"),
        "a.txt",
        "old\n",
        "new\n",
        LineRange::new(1, 2),
    );
    let kiln = HunkId::derive(
        &PhysicalRoot::from_top_level("/kiln"),
        "a.txt",
        "old\n",
        "new\n",
        LineRange::new(1, 2),
    );
    assert_ne!(workspace, kiln);
}

#[test]
fn hunk_identity_separates_the_root_from_the_path() {
    // Without length-prefixed fields the root/path seam would move freely:
    // these two name different files and must not hash the same bytes.
    let a = HunkId::derive(
        &PhysicalRoot::from_top_level("/a/b"),
        "c",
        "",
        "",
        LineRange::new(1, 1),
    );
    let b = HunkId::derive(
        &PhysicalRoot::from_top_level("/a"),
        "b/c",
        "",
        "",
        LineRange::new(1, 1),
    );
    assert_ne!(a, b);
}

#[test]
fn hunk_identity_separates_path_from_content() {
    // Same seam, one field along.
    let root = &PhysicalRoot::from_top_level("/repo");
    let a = HunkId::derive(root, "ab", "c", "", LineRange::new(1, 1));
    let b = HunkId::derive(root, "a", "bc", "", LineRange::new(1, 1));
    assert_ne!(a, b);
}

#[test]
fn hunk_identity_differs_per_path_per_side_and_per_base_range() {
    let root = &PhysicalRoot::from_top_level("/repo");
    let range = LineRange::new(1, 2);
    let base = HunkId::derive(root, "src/lib.rs", "old\n", "new\n", range);
    assert_ne!(
        base,
        HunkId::derive(root, "src/main.rs", "old\n", "new\n", range)
    );
    assert_ne!(
        base,
        HunkId::derive(root, "src/lib.rs", "other\n", "new\n", range)
    );
    assert_ne!(
        base,
        HunkId::derive(root, "src/lib.rs", "old\n", "other\n", range)
    );
    assert_ne!(
        base,
        HunkId::derive(root, "src/lib.rs", "old\n", "new\n", LineRange::new(1, 3))
    );
    assert_ne!(
        base,
        HunkId::derive(root, "src/lib.rs", "old\n", "new\n", LineRange::new(2, 2))
    );
}

#[test]
fn unreviewed_is_the_default_state() {
    assert_eq!(ReviewState::default(), ReviewState::Unreviewed);
}

#[test]
fn hunk_with_no_attribution_is_external() {
    let id = HunkId::derive(
        &PhysicalRoot::from_top_level("/repo"),
        "p",
        "a",
        "b",
        LineRange::new(1, 2),
    );
    assert!(hunk(id.clone(), vec![]).is_external());
    assert!(!hunk(id, vec!["call-1".into()]).is_external());
}

#[test]
fn line_range_is_half_open() {
    let r = LineRange::new(4, 7);
    assert_eq!(r.len(), 3);
    assert!(r.contains(4) && r.contains(6));
    assert!(!r.contains(3) && !r.contains(7));

    let insertion_point = LineRange::new(9, 9);
    assert!(insertion_point.is_empty());
    assert!(!insertion_point.contains(9));
}

#[test]
fn ledger_base_is_per_root_and_append_only() {
    let mut ledger = Ledger::new(
        "sess-1",
        vec![
            RootBase {
                root: PhysicalRoot::from_top_level("/repo"),
                base_tree: TreeSha::new("aaa"),
            },
            RootBase {
                root: PhysicalRoot::from_top_level("/kiln"),
                base_tree: TreeSha::new("bbb"),
            },
        ],
    );

    assert_eq!(
        ledger.base_tree(&PhysicalRoot::from_top_level("/repo")),
        Some(&TreeSha::new("aaa"))
    );
    assert_eq!(ledger.base_tree(Path::new("/elsewhere")), None);
    assert_eq!(ledger.roots().count(), 2);

    ledger.push_interval_in_memory(Interval {
        tool_call_id: "call-1".into(),
        node_id: 7,
        roots_touched: vec![RootInterval {
            root: PhysicalRoot::from_top_level("/repo"),
            before_tree: TreeSha::new("aaa"),
            after_tree: TreeSha::new("ccc"),
        }],
        contested: false,
        child_session_id: None,
    });
    ledger.link_child(ChildLedgerRef {
        tool_call_id: "call-2".into(),
        child_session_id: "sess-2".into(),
        node_id: Some(3),
    });

    assert_eq!(ledger.intervals().len(), 1);
    assert_eq!(ledger.children()[0].child_session_id, "sess-2");
    // The base is unchanged by anything that happens after the ledger opens.
    assert_eq!(
        ledger.base_tree(&PhysicalRoot::from_top_level("/repo")),
        Some(&TreeSha::new("aaa"))
    );
}

#[test]
fn ledger_roundtrips_through_json_with_its_base() {
    let mut ledger = Ledger::new(
        "sess-1",
        vec![RootBase {
            root: PhysicalRoot::from_top_level("/repo"),
            base_tree: TreeSha::new("aaa"),
        }],
    );
    ledger.push_interval_in_memory(Interval {
        tool_call_id: "call-1".into(),
        node_id: 1,
        roots_touched: vec![RootInterval {
            root: PhysicalRoot::from_top_level("/repo"),
            before_tree: TreeSha::new("aaa"),
            after_tree: TreeSha::new("ccc"),
        }],
        contested: true,
        child_session_id: None,
    });

    let json = serde_json::to_string(&ledger).unwrap();
    let back: Ledger = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ledger);
    assert!(back.intervals()[0].contested);
}

#[test]
fn review_state_serializes_snake_case() {
    assert_eq!(
        serde_json::to_string(&ReviewState::Unreviewed).unwrap(),
        "\"unreviewed\""
    );
    assert_eq!(
        serde_json::to_string(&ReviewState::Rejected).unwrap(),
        "\"rejected\""
    );
}

#[test]
fn interval_contested_defaults_false_for_older_records() {
    let json = r#"{"tool_call_id":"c","node_id":3,"roots_touched":[]}"#;
    let interval: Interval = serde_json::from_str(json).unwrap();
    assert!(!interval.contested);
}

/// A journal row from before the harvest existed is the parent's own work, so
/// absence has to read as "mine", never as a delegation to some child session
/// the row cannot name.
#[test]
fn interval_child_session_id_is_absent_for_older_records() {
    let json = r#"{"tool_call_id":"c","node_id":3,"roots_touched":[]}"#;
    let interval: Interval = serde_json::from_str(json).unwrap();
    assert_eq!(interval.child_session_id, None);
}

// ── Persistence and degradation ─────────────────────────────────────────────

fn interval_over(root: &str, before: &str, after: &str) -> Interval {
    Interval {
        tool_call_id: "c".to_string(),
        node_id: 1,
        roots_touched: vec![RootInterval {
            root: PhysicalRoot::from_top_level(root),
            before_tree: TreeSha::new(before),
            after_tree: TreeSha::new(after),
        }],
        contested: false,
        child_session_id: None,
    }
}

/// The keep ref is built from this list, so anything missing from it is a tree
/// `git gc --prune=now` deletes — and a deleted base makes the composed diff
/// uncomputable rather than merely stale.
#[test]
fn trees_for_a_root_covers_the_base_and_both_sides_of_every_interval() {
    let mut ledger = Ledger::new(
        "s",
        vec![
            RootBase {
                root: PhysicalRoot::from_top_level("/a"),
                base_tree: TreeSha::new("base-a"),
            },
            RootBase {
                root: PhysicalRoot::from_top_level("/b"),
                base_tree: TreeSha::new("base-b"),
            },
        ],
    );
    ledger.push_interval_in_memory(interval_over("/a", "base-a", "t1"));
    ledger.push_interval_in_memory(interval_over("/a", "t1", "t2"));
    ledger.push_interval_in_memory(interval_over("/b", "base-b", "t3"));

    let mut a = ledger.trees_for(&PhysicalRoot::from_top_level("/a"));
    a.sort();
    assert_eq!(
        a,
        vec![
            TreeSha::new("base-a"),
            TreeSha::new("t1"),
            TreeSha::new("t2")
        ],
        "a tree shared by two intervals must appear once, and none may be missing"
    );
    // Per-repository: a tree SHA only means anything in the object store that
    // produced it, so /b's keep ref must never name /a's trees.
    assert_eq!(
        ledger.trees_for(Path::new("/b")),
        vec![TreeSha::new("base-b"), TreeSha::new("t3")]
    );
}

/// Intervals go with the base because a tree pair measured against a base that
/// no longer exists cannot intersect anything: keeping them would leave the
/// ledger claiming attribution it has no way to compute.
#[test]
fn rebasing_replaces_the_base_and_voids_the_intervals_measured_from_it() {
    let mut ledger = Ledger::new(
        "s",
        vec![RootBase {
            root: PhysicalRoot::from_top_level("/a"),
            base_tree: TreeSha::new("old"),
        }],
    );
    ledger.push_interval_in_memory(interval_over("/a", "old", "t1"));
    ledger.link_child(ChildLedgerRef {
        tool_call_id: "d".to_string(),
        child_session_id: "child".to_string(),
        node_id: Some(7),
    });

    ledger.rebase(vec![RootBase {
        root: PhysicalRoot::from_top_level("/a"),
        base_tree: TreeSha::new("new"),
    }]);

    assert_eq!(
        ledger.base_tree(&PhysicalRoot::from_top_level("/a")),
        Some(&TreeSha::new("new"))
    );
    assert!(ledger.intervals().is_empty());
    // Delegation links describe session structure, not measurement, so they
    // survive a rebase the way a comment does.
    assert_eq!(ledger.children().len(), 1);
}

/// Grading exists because losing attribution fails *open*: an unattributed
/// hunk is external, `unreviewed_hunks` skips external hunks, and the gate
/// stops blocking. A lost interval must therefore be louder than a lost
/// comment.
#[test]
fn integrity_blocks_only_the_root_a_skipped_interval_names() {
    let mut integrity = Integrity::default();
    integrity.record(Skip {
        record: SkipKind::Root {
            root: PhysicalRoot::from_top_level("/a"),
        },
        line: 4,
        reason: "truncated".to_string(),
    });

    assert!(integrity.blocks(&PhysicalRoot::from_top_level("/a")));
    assert!(!integrity.blocks(Path::new("/b")));
    assert!(
        !integrity.blocks_everything(),
        "a scoped loss must not hold repositories it says nothing about"
    );
}

/// A loss the loader could not scope may have taken the root list with it, so
/// there is nothing left to compare a path against.
#[test]
fn an_unscoped_skip_blocks_every_root() {
    let mut integrity = Integrity::default();
    integrity.record(Skip {
        record: SkipKind::Session,
        line: 1,
        reason: "header unreadable".to_string(),
    });

    assert!(integrity.blocks_everything());
    assert!(integrity.blocks(Path::new("/anywhere")));
}

#[test]
fn an_informational_skip_blocks_nothing() {
    let mut integrity = Integrity::default();
    integrity.record(Skip {
        record: SkipKind::Informational,
        line: 9,
        reason: "comment unreadable".to_string(),
    });

    assert!(!integrity.is_intact());
    assert!(!integrity.blocks(&PhysicalRoot::from_top_level("/a")));
    assert!(!integrity.blocks_everything());
}

/// Only `Clear` may let a write through. Collapsing `Degraded` into it is the
/// structural-failure case turning the gate off exactly when attribution has
/// stopped working.
#[test]
fn only_a_clear_verdict_lets_a_write_through() {
    assert!(!Verdict::Clear.blocks());
    assert!(Verdict::Unreviewed.blocks());
    assert!(Verdict::Degraded { root: None }.blocks());
}

/// `0` is a real conversation-tree node — the root — so a row written before
/// the field existed must read back as absent, never as turn 0.
#[test]
fn a_child_ledger_ref_without_a_node_id_is_none_not_zero() {
    let json = r#"{"tool_call_id":"d","child_session_id":"child"}"#;
    let child: ChildLedgerRef = serde_json::from_str(json).unwrap();
    assert_eq!(child.node_id, None);
}
