//! The turn boundary the review's `Turn` scope filters on.
//!
//! No marker is written for it: `Interval::node_id` is the tree node current
//! at bracket close, ids are append-only, so the last `User` node on the
//! current path separates this turn's intervals from every earlier one.

use super::*;
use crucible_core::turn::NodeContent;

#[tokio::test]
async fn turn_start_node_is_the_last_user_node_on_the_current_path() {
    let session_manager = temp_session_manager();
    let am = create_test_agent_manager(session_manager);
    let session_id = "scoped";

    assert_eq!(
        am.turn_start_node(session_id).await,
        None,
        "a session with no tree and no record has no turn"
    );

    let tree = am
        .get_or_rebuild_session_tree(session_id, std::path::Path::new("/nonexistent.jsonl"))
        .await;
    let (u1, u2) = {
        let mut t = tree.lock().await;
        let root = t.root();
        let u1 = t.add_child_and_advance(
            root,
            NodeContent::User {
                text: "first".into(),
            },
        );
        t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let cursor = t.current();
        let u2 = t.add_child_and_advance(
            cursor,
            NodeContent::User {
                text: "second".into(),
            },
        );
        t.add_child_and_advance(u2, NodeContent::Agent { text: "a2".into() });
        (u1, u2)
    };

    assert_eq!(am.turn_start_node(session_id).await, Some(u2.index()));
    assert!(u1.index() < u2.index(), "ids are append-only");

    // The cursor moves back on an undo, and the boundary follows the path.
    tree.lock().await.undo_turns(1);
    assert_eq!(am.turn_start_node(session_id).await, Some(u1.index()));
}
