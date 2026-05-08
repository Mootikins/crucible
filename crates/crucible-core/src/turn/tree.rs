//! Append-only conversation tree.
//!
//! Conversations naturally branch: a user may fork from a prior turn,
//! a workflow may fan out parallel sub-tasks and collect their results,
//! a plan may be re-tried from an earlier decision point. Modelling
//! this as a tree (rather than a linear `Vec<Message>`) makes those
//! operations primitive instead of bolted-on.
//!
//! Ownership: the scheduler (e.g. daemon's `SessionManager`) owns the
//! tree. Agent handles are stateless between turns — they receive the
//! linearised path from the scheduler and emit `TurnEvent`s the
//! scheduler folds back into the tree.
//!
//! Invariants:
//!
//! * Append-only: nodes and their parent link are immutable after
//!   creation. Text deltas land on the *current leaf* via
//!   `append_delta`; completing the text moves the current node onto
//!   a fresh child.
//! * `path_to_here` walks `parent` links only. Fanout/collect live in
//!   [`NodeMeta`] as side-channel metadata; they do not affect the
//!   linear path delivered to an agent.
//! * Collect merges never target a leaf as their parent: the merge
//!   node attaches to the common ancestor of the branches it joins, so
//!   the linear path downstream of the merge does not traverse any one
//!   branch arbitrarily.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

/// Stable handle to a node in a [`ConversationTree`].
///
/// Copy-cheap (wraps a `NonZeroU32`), serialisable, stable across the
/// life of a tree. Not portable across trees — a `NodeId` from one
/// tree is meaningless in another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(NonZeroU32);

impl NodeId {
    fn new(raw: u32) -> Self {
        NodeId(NonZeroU32::new(raw).expect("NodeId is 1-based"))
    }

    /// Numeric index for debugging/telemetry. Not stable across
    /// serialisation formats that rewrite ids.
    pub fn index(self) -> u32 {
        self.0.get()
    }
}

/// Content carried by a conversation node.
///
/// One variant per logical role. `Agent` holds the LLM's textual reply,
/// built up across many `append_delta` calls. `ToolCall`/`ToolResult`
/// attach to the agent turn they belong to by parent link; they live
/// as siblings or children, never inside the agent node's text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeContent {
    /// Root of the tree. Contains no content; exists so every real
    /// node has a parent.
    Root,
    /// User-authored message.
    User { text: String },
    /// Agent response (model output). Text accumulates via
    /// `append_delta`; finalised on the next non-delta event.
    Agent { text: String },
    /// System / developer instruction injected into context.
    System { text: String },
    /// Agent-authored reasoning trace ("thinking" / reflection).
    Thinking { text: String },
    /// Tool invocation request from the agent.
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    /// Tool execution result.
    ToolResult {
        id: String,
        name: String,
        result: serde_json::Value,
        error: Option<String>,
    },
    /// Marker node introduced by `fanout` or `collect`. Holds no text.
    /// The tree-walker treats it transparently; UIs may render it as
    /// a branch / merge annotation.
    Marker { label: String },
}

/// Side-channel metadata on a node.
///
/// `merged_from` is how `collect` records the branches it unified
/// without violating the "parent link is the one true link" invariant.
/// The merge node's `parent` is the common ancestor; `merged_from`
/// lists the leaves (or sub-roots) whose contents contributed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeMeta {
    /// Non-empty iff this node was produced by `collect`.
    pub merged_from: Vec<NodeId>,
    /// Non-empty iff this node was produced by `fanout`. The children
    /// are the parallel branches about to run.
    pub fanout_children: Vec<NodeId>,
    /// Arbitrary tags (e.g. "plan", "tool-batch") used by handlers /
    /// workflow scripts to recognise structural intent.
    pub tags: Vec<String>,
}

/// A single node in the conversation tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnNode {
    /// `None` only for the root node.
    pub parent: Option<NodeId>,
    pub content: NodeContent,
    pub meta: NodeMeta,
}

/// Append-only conversation tree.
///
/// Storage is a `Vec<TurnNode>` indexed by `NodeId - 1`. The root is
/// always `NodeId(1)`. Deletion is not supported — every append gets a
/// fresh id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTree {
    nodes: Vec<TurnNode>,
    /// The node the next `append_delta` will land on. Scheduler moves
    /// this by calling `finalize_current` after a turn's text is
    /// complete (or after a user message is added).
    current: NodeId,
}

impl ConversationTree {
    /// Build an empty tree with just the root.
    pub fn new() -> Self {
        let root = TurnNode {
            parent: None,
            content: NodeContent::Root,
            meta: NodeMeta::default(),
        };
        Self {
            nodes: vec![root],
            current: NodeId::new(1),
        }
    }

    /// Root node id. Always `NodeId(1)`.
    pub fn root(&self) -> NodeId {
        NodeId::new(1)
    }

    /// The node most recent ops will append into.
    pub fn current(&self) -> NodeId {
        self.current
    }

    /// Read-only access to a node.
    pub fn get(&self, id: NodeId) -> &TurnNode {
        &self.nodes[(id.index() - 1) as usize]
    }

    /// Number of nodes in the tree (incl. root).
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    /// Iterate over every node in the tree in insertion order, paired
    /// with its `NodeId`. Includes the root.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &TurnNode)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| (NodeId::new((idx + 1) as u32), node))
    }

    /// Append a new child under `parent` and return its id. Does not
    /// move `current`. Callers use [`Self::add_child_and_advance`] when
    /// they want the new node to become `current`.
    pub fn add_child(&mut self, parent: NodeId, content: NodeContent) -> NodeId {
        self.add_child_with_meta(parent, content, NodeMeta::default())
    }

    /// As [`Self::add_child`] but with explicit metadata.
    pub fn add_child_with_meta(
        &mut self,
        parent: NodeId,
        content: NodeContent,
        meta: NodeMeta,
    ) -> NodeId {
        let id = NodeId::new((self.nodes.len() + 1) as u32);
        self.nodes.push(TurnNode {
            parent: Some(parent),
            content,
            meta,
        });
        id
    }

    /// Convenience: append + advance `current` to the new node.
    pub fn add_child_and_advance(&mut self, parent: NodeId, content: NodeContent) -> NodeId {
        let id = self.add_child(parent, content);
        self.current = id;
        id
    }

    /// Append text to the current node, provided its content is a
    /// text-carrying variant (`Agent` / `Thinking`). No-op otherwise.
    ///
    /// Returns `true` if the delta landed, `false` if the current
    /// node's content kind does not accept deltas.
    pub fn append_delta(&mut self, delta: &str) -> bool {
        let cur = self.current;
        let node = &mut self.nodes[(cur.index() - 1) as usize];
        match &mut node.content {
            NodeContent::Agent { text } | NodeContent::Thinking { text } => {
                text.push_str(delta);
                true
            }
            _ => false,
        }
    }

    /// Move `current` explicitly (e.g. the scheduler just created a
    /// fresh `Agent` node to accumulate a new turn).
    pub fn set_current(&mut self, id: NodeId) {
        debug_assert!((id.index() as usize) <= self.nodes.len());
        self.current = id;
    }

    /// Walk `parent` links from `id` up to the root, returning the path
    /// in root-to-`id` order. Does **not** traverse `merged_from`; that
    /// is structural metadata a renderer may consult separately.
    pub fn path_to_here(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = Some(id);
        while let Some(n) = cur {
            out.push(n);
            cur = self.nodes[(n.index() - 1) as usize].parent;
        }
        out.reverse();
        out
    }

    /// Branch `parent` into `n` parallel child markers. Returns their
    /// ids. The `parent` node gains `fanout_children` metadata listing
    /// them. Useful for workflow scripts that want to run several
    /// agents concurrently and later merge.
    pub fn fanout(&mut self, parent: NodeId, labels: Vec<String>) -> Vec<NodeId> {
        let ids: Vec<NodeId> = labels
            .into_iter()
            .map(|label| self.add_child(parent, NodeContent::Marker { label }))
            .collect();
        let parent_meta = &mut self.nodes[(parent.index() - 1) as usize]
            .meta
            .fanout_children;
        parent_meta.extend(ids.iter().copied());
        ids
    }

    /// Merge a set of branch nodes into a single marker whose parent
    /// is their common ancestor. `branches` must be non-empty; every
    /// branch id must exist in this tree; they must share an ancestor
    /// strictly above every branch (this is always true for branches
    /// produced by `fanout`).
    ///
    /// Returns the new merge node's id. Its parent is the common
    /// ancestor (never any of the branches themselves), and its
    /// `merged_from` metadata records the branches it unified.
    pub fn collect(&mut self, branches: &[NodeId], label: impl Into<String>) -> NodeId {
        assert!(!branches.is_empty(), "collect requires ≥1 branch");
        let ancestor = self
            .common_ancestor(branches)
            .expect("collect: branches share no ancestor");
        for b in branches {
            debug_assert_ne!(*b, ancestor, "collect: ancestor must not equal a branch");
        }
        let meta = NodeMeta {
            merged_from: branches.to_vec(),
            ..Default::default()
        };
        self.add_child_with_meta(
            ancestor,
            NodeContent::Marker {
                label: label.into(),
            },
            meta,
        )
    }

    /// Count the `User` nodes on the current path — the number of
    /// turns that could be undone.
    pub fn undo_depth(&self) -> usize {
        self.path_to_here(self.current)
            .iter()
            .filter(|id| matches!(self.get(**id).content, NodeContent::User { .. }))
            .count()
    }

    /// True iff there is at least one turn on the current path.
    pub fn can_undo(&self) -> bool {
        self.undo_depth() > 0
    }

    /// Per-turn summaries for every undoable turn on the current path,
    /// in oldest-to-newest order. Each entry mirrors what `undo_turns`
    /// *would* produce if invoked with the full undo depth — without
    /// mutating the tree. Useful for the `cru.sessions.undo_history`
    /// Lua API and any UI that wants to show "what would be undone".
    pub fn turn_summaries(&self) -> Vec<crate::types::UndoSummary> {
        let path = self.path_to_here(self.current);
        let user_indices: Vec<usize> = path
            .iter()
            .enumerate()
            .filter(|(_, id)| matches!(self.get(**id).content, NodeContent::User { .. }))
            .map(|(idx, _)| idx)
            .collect();
        if user_indices.is_empty() {
            return Vec::new();
        }
        let n = user_indices.len();
        let mut summaries = Vec::with_capacity(n);
        for i in 0..n {
            let turn_span = if i + 1 < n {
                user_indices[i + 1] - user_indices[i]
            } else {
                path.len() - user_indices[i]
            };
            summaries.push(crate::types::UndoSummary {
                messages_removed: turn_span,
            });
        }
        summaries
    }

    /// Undo `n` turns: move `current` back to the parent of the
    /// `n`-th most-recent `User` node on the current path. Returns one
    /// entry per turn undone, recording how many non-root nodes were
    /// abandoned (for UI / telemetry). Caps at available turns.
    pub fn undo_turns(&mut self, n: usize) -> Vec<crate::types::UndoSummary> {
        if n == 0 {
            return Vec::new();
        }
        let path = self.path_to_here(self.current);
        let user_indices: Vec<usize> = path
            .iter()
            .enumerate()
            .filter(|(_, id)| matches!(self.get(**id).content, NodeContent::User { .. }))
            .map(|(idx, _)| idx)
            .collect();
        if user_indices.is_empty() {
            return Vec::new();
        }
        let n = n.min(user_indices.len());
        let target_user = user_indices[user_indices.len() - n];
        let new_current = if target_user == 0 {
            self.root()
        } else {
            path[target_user - 1]
        };
        let mut summaries = Vec::with_capacity(n);
        let mut remaining = path.len() - target_user;
        for i in 0..n {
            let turn_span = if i + 1 < n {
                let next = user_indices[user_indices.len() - n + i + 1];
                next - user_indices[user_indices.len() - n + i]
            } else {
                remaining
            };
            summaries.push(crate::types::UndoSummary {
                messages_removed: turn_span,
            });
            remaining = remaining.saturating_sub(turn_span);
        }
        self.current = new_current;
        summaries
    }

    /// Remove a contiguous slice of messages from the current path
    /// (root excluded), reporting how many path nodes became
    /// unreachable.
    ///
    /// The tree is append-only, so "removal" means rewinding the
    /// `current` cursor to the last surviving node — anything below
    /// becomes unreachable from `current` onward. Concretely:
    ///
    /// * `All` → drop everything, current becomes root, returns the
    ///   prior path length (excluding root).
    /// * `Last(n)` → rewind `n` non-root nodes from current; returns
    ///   `min(n, path_len)`.
    /// * `First(n)` → an append-only tree cannot drop a prefix while
    ///   keeping a suffix; this rewinds to root and returns
    ///   `min(n, path_len)`.
    /// * `Indices(start..end)` → rewinds to keep the first `start`
    ///   non-root path nodes (the cursor lands on that node, or the
    ///   root if `start == 0`), then reports how many path nodes the
    ///   range named (`min(end, path_len) - start`, saturating at 0).
    pub fn remove_range(&mut self, range: crate::traits::context_ops::Range) -> usize {
        use crate::traits::context_ops::Range;
        // path_to_here includes root; non-root indices map to 1..path.len()
        let path = self.path_to_here(self.current);
        if path.len() <= 1 {
            return 0;
        }
        let path_len = path.len() - 1;
        let (keep_idx_in_path, removed) = match range {
            Range::All => (0, path_len),
            Range::Last(n) => {
                let n = n.min(path_len);
                (path_len - n, n)
            }
            Range::First(n) => (0, n.min(path_len)),
            Range::Indices(r) => {
                let start = r.start.min(path_len);
                let end = r.end.min(path_len);
                if end <= start {
                    return 0;
                }
                (start, end - start)
            }
        };
        // path[0] = root; path[1..] are non-root nodes. Cursor lands on
        // path[keep_idx_in_path] which is root when keep_idx_in_path == 0.
        self.current = path[keep_idx_in_path];
        removed
    }

    /// Flatten the current path (root → current leaf) into the unified
    /// [`ContextMessage`] representation agents consume via
    /// `TurnContext.messages`. Root / marker / thinking nodes skip; the
    /// others project to their natural role.
    pub fn flatten_current_path_to_context(
        &self,
    ) -> Vec<crate::traits::context_ops::ContextMessage> {
        use crate::traits::context_ops::ContextMessage;
        let path = self.path_to_here(self.current);
        let mut out = Vec::with_capacity(path.len());
        for id in path {
            let node = self.get(id);
            match &node.content {
                NodeContent::Root | NodeContent::Marker { .. } => continue,
                NodeContent::User { text } => out.push(ContextMessage::user(text)),
                NodeContent::Agent { text } if !text.is_empty() => {
                    out.push(ContextMessage::assistant(text));
                }
                NodeContent::Agent { .. } => continue,
                NodeContent::System { text } => out.push(ContextMessage::system(text)),
                NodeContent::Thinking { .. } => continue,
                NodeContent::ToolCall { .. } | NodeContent::ToolResult { .. } => {
                    // Tool exchanges are folded into the turn by the
                    // agent's in-loop bookkeeping; they do not
                    // participate in the scheduler-flattened context.
                    continue;
                }
            }
        }
        out
    }

    /// Lowest common ancestor of the supplied nodes. `None` only if the
    /// slice is empty.
    pub fn common_ancestor(&self, nodes: &[NodeId]) -> Option<NodeId> {
        let mut iter = nodes.iter().copied();
        let first = iter.next()?;
        let mut ancestors: Vec<NodeId> = self.path_to_here(first);
        for n in iter {
            let path = self.path_to_here(n);
            ancestors.retain(|a| path.contains(a));
            if ancestors.is_empty() {
                return None;
            }
        }
        ancestors.last().copied()
    }
}

impl Default for ConversationTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> NodeContent {
        NodeContent::User { text: t.into() }
    }

    #[test]
    fn root_is_node_one() {
        let t = ConversationTree::new();
        assert_eq!(t.root().index(), 1);
        assert_eq!(t.current(), t.root());
        assert!(matches!(t.get(t.root()).content, NodeContent::Root));
    }

    #[test]
    fn add_child_extends_tree_and_preserves_parent_link() {
        let mut t = ConversationTree::new();
        let a = t.add_child(t.root(), text("hi"));
        assert_eq!(t.get(a).parent, Some(t.root()));
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn add_child_and_advance_moves_current() {
        let mut t = ConversationTree::new();
        let a = t.add_child_and_advance(
            t.root(),
            NodeContent::Agent {
                text: String::new(),
            },
        );
        assert_eq!(t.current(), a);
    }

    #[test]
    fn append_delta_accumulates_on_agent_node() {
        let mut t = ConversationTree::new();
        let a = t.add_child_and_advance(
            t.root(),
            NodeContent::Agent {
                text: String::new(),
            },
        );
        assert!(t.append_delta("hel"));
        assert!(t.append_delta("lo"));
        match &t.get(a).content {
            NodeContent::Agent { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn append_delta_no_op_on_non_text_node() {
        let mut t = ConversationTree::new();
        let u = t.add_child_and_advance(t.root(), text("who"));
        assert!(!t.append_delta("ignored"));
        assert!(matches!(&t.get(u).content, NodeContent::User { text } if text == "who"));
    }

    #[test]
    fn path_to_here_walks_parent_links_only() {
        let mut t = ConversationTree::new();
        let a = t.add_child(t.root(), text("a"));
        let b = t.add_child(a, text("b"));
        let c = t.add_child(b, text("c"));
        assert_eq!(t.path_to_here(c), vec![t.root(), a, b, c]);
    }

    #[test]
    fn fanout_records_children_metadata() {
        let mut t = ConversationTree::new();
        let kids = t.fanout(t.root(), vec!["l".into(), "r".into()]);
        assert_eq!(kids.len(), 2);
        assert_eq!(t.get(t.root()).meta.fanout_children, kids);
        for k in kids {
            assert_eq!(t.get(k).parent, Some(t.root()));
        }
    }

    #[test]
    fn collect_attaches_to_common_ancestor_not_to_a_branch() {
        let mut t = ConversationTree::new();
        let a = t.add_child(t.root(), text("ctx"));
        let kids = t.fanout(a, vec!["l".into(), "r".into()]);
        let merge = t.collect(&kids, "joined");
        // Merge parent is the fanout parent, not a branch.
        assert_eq!(t.get(merge).parent, Some(a));
        assert_eq!(t.get(merge).meta.merged_from, kids);
        // path_to_here for merge does not traverse any branch.
        let path = t.path_to_here(merge);
        assert_eq!(path, vec![t.root(), a, merge]);
    }

    #[test]
    fn common_ancestor_handles_siblings() {
        let mut t = ConversationTree::new();
        let a = t.add_child(t.root(), text("a"));
        let b = t.add_child(a, text("b"));
        let c = t.add_child(a, text("c"));
        assert_eq!(t.common_ancestor(&[b, c]), Some(a));
    }

    #[test]
    fn undo_depth_counts_user_nodes() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let u2 = t.add_child_and_advance(t.current(), text("u2"));
        t.add_child_and_advance(u2, NodeContent::Agent { text: "a2".into() });
        assert_eq!(t.undo_depth(), 2);
        assert!(t.can_undo());
    }

    #[test]
    fn undo_turns_rewinds_cursor_to_parent_of_user() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        let a1 = t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let u2 = t.add_child_and_advance(t.current(), text("u2"));
        let _a2 = t.add_child_and_advance(u2, NodeContent::Agent { text: "a2".into() });

        let summaries = t.undo_turns(1);
        assert_eq!(summaries.len(), 1);
        assert_eq!(t.current(), a1); // cursor sits at the parent of u2
        assert_eq!(t.undo_depth(), 1);
    }

    #[test]
    fn undo_turns_more_than_available_caps() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let summaries = t.undo_turns(5);
        assert_eq!(summaries.len(), 1);
        assert_eq!(t.current(), t.root());
    }

    #[test]
    fn undo_turns_zero_is_noop() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        assert!(t.undo_turns(0).is_empty());
        assert_eq!(t.current(), u1);
    }

    #[test]
    fn turn_summaries_reports_per_turn_spans_oldest_first() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let u2 = t.add_child_and_advance(t.current(), text("u2"));
        t.add_child_and_advance(u2, NodeContent::Agent { text: "a2".into() });

        let summaries = t.turn_summaries();
        assert_eq!(summaries.len(), 2);
        // Each turn has 2 nodes (user + agent) on the path.
        assert_eq!(summaries[0].messages_removed, 2);
        assert_eq!(summaries[1].messages_removed, 2);
        // Read-only — tree state unchanged.
        assert_eq!(t.undo_depth(), 2);
    }

    #[test]
    fn turn_summaries_empty_when_no_turns() {
        let t = ConversationTree::new();
        assert!(t.turn_summaries().is_empty());
    }

    #[test]
    fn remove_range_all_rewinds_to_root() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        let _a1 = t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let removed = t.remove_range(crate::traits::context_ops::Range::All);
        assert_eq!(removed, 2);
        assert_eq!(t.current(), t.root());
    }

    #[test]
    fn remove_range_last_rewinds_n_nodes() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        let a1 = t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let _u2 = t.add_child_and_advance(a1, text("u2"));
        let removed = t.remove_range(crate::traits::context_ops::Range::Last(2));
        assert_eq!(removed, 2);
        assert_eq!(t.current(), u1);
    }

    #[test]
    fn remove_range_indices_truncates_from_start() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        let a1 = t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let _u2 = t.add_child_and_advance(a1, text("u2"));
        // path (excluding root) has 3 nodes; remove indices [1, 3) = 2 nodes.
        let removed = t.remove_range(crate::traits::context_ops::Range::Indices(1..3));
        assert_eq!(removed, 2);
        assert_eq!(t.current(), u1);
    }

    #[test]
    fn remove_range_first_drops_everything() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        let _a1 = t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let removed = t.remove_range(crate::traits::context_ops::Range::First(1));
        assert_eq!(removed, 1);
        assert_eq!(t.current(), t.root());
    }

    #[test]
    fn remove_range_last_more_than_available_caps() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        let _a1 = t.add_child_and_advance(u1, NodeContent::Agent { text: "a1".into() });
        let removed = t.remove_range(crate::traits::context_ops::Range::Last(10));
        assert_eq!(removed, 2);
        assert_eq!(t.current(), t.root());
    }

    #[test]
    fn remove_range_empty_indices_is_noop() {
        let mut t = ConversationTree::new();
        let u1 = t.add_child_and_advance(t.root(), text("u1"));
        // Build the range explicitly so clippy doesn't flag a literal empty range.
        let two: usize = 2;
        let one: usize = 1;
        let removed = t.remove_range(crate::traits::context_ops::Range::Indices(two..one));
        assert_eq!(removed, 0);
        assert_eq!(t.current(), u1);
    }

    #[test]
    fn remove_range_on_empty_tree_returns_zero() {
        let mut t = ConversationTree::new();
        let removed = t.remove_range(crate::traits::context_ops::Range::All);
        assert_eq!(removed, 0);
        assert_eq!(t.current(), t.root());
    }

    #[test]
    fn node_id_is_copy_and_serialisable() {
        let mut t = ConversationTree::new();
        let a = t.add_child(t.root(), text("a"));
        let s = serde_json::to_string(&a).unwrap();
        let r: NodeId = serde_json::from_str(&s).unwrap();
        assert_eq!(a, r);
    }
}
