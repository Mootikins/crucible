//! In-Memory Graph Implementation
//!
//! This module provides an in-memory graph built from the denormalized `links_to`
//! field in [`NoteRecord`]. It implements the [`GraphView`] trait for efficient
//! graph traversal over a kiln's link structure.
//!
//! # Design
//!
//! Instead of storing relations in the database, we denormalize links into each
//! `NoteRecord.links_to` and rebuild the graph in-memory on startup. This approach:
//!
//! - Provides O(1) outlink and backlink lookups
//! - Supports efficient BFS traversal for neighbor queries
//! - Rebuilds in ~100ms for 20k notes
//!
//! # Example
//!
//! ```
//! use crucible_core::storage::{InMemoryGraph, GraphView, NoteRecord};
//! use crucible_core::parser::BlockHash;
//!
//! // Create notes with links
//! let notes = vec![
//!     NoteRecord::new("a.md", BlockHash::zero()).with_links(vec!["b.md".to_string()]),
//!     NoteRecord::new("b.md", BlockHash::zero()).with_links(vec!["c.md".to_string()]),
//!     NoteRecord::new("c.md", BlockHash::zero()),
//! ];
//!
//! // Build graph from notes
//! let graph = InMemoryGraph::from_notes(&notes);
//!
//! // Query the graph
//! assert_eq!(graph.outlinks("a.md"), vec!["b.md"]);
//! assert_eq!(graph.backlinks("b.md"), vec!["a.md"]);
//! ```

use std::collections::{HashMap, HashSet, VecDeque};

use super::note_store::{GraphView, NoteRecord};

// ============================================================================
// InMemoryGraph
// ============================================================================

/// In-memory graph built from denormalized note links
///
/// This struct maintains two lookup tables for efficient bidirectional traversal:
/// - `outlinks`: path -> [targets] (notes this note links to)
/// - `backlinks`: path -> [sources] (notes linking to this note)
///
/// The graph is built from `NoteRecord.links_to` and should be rebuilt whenever
/// notes are added, removed, or have their links modified.
#[derive(Debug, Clone)]
pub struct InMemoryGraph {
    /// Forward links: path -> [targets]
    outlinks: HashMap<String, Vec<String>>,
    /// Backward links: path -> [sources]
    backlinks: HashMap<String, Vec<String>>,
}

impl InMemoryGraph {
    /// Create an empty graph
    #[must_use]
    pub fn new() -> Self {
        Self {
            outlinks: HashMap::new(),
            backlinks: HashMap::new(),
        }
    }

    /// Build a graph from a slice of note records
    ///
    /// This iterates through all notes and builds both the outlinks and backlinks
    /// maps in a single pass.
    ///
    /// # Example
    ///
    /// ```
    /// use crucible_core::storage::{InMemoryGraph, NoteRecord};
    /// use crucible_core::parser::BlockHash;
    ///
    /// let notes = vec![
    ///     NoteRecord::new("a.md", BlockHash::zero())
    ///         .with_links(vec!["b.md".to_string(), "c.md".to_string()]),
    /// ];
    ///
    /// let graph = InMemoryGraph::from_notes(&notes);
    /// ```
    #[must_use]
    pub fn from_notes(notes: &[NoteRecord]) -> Self {
        let mut graph = Self::new();
        graph.rebuild(notes);
        graph
    }

    /// Get the number of nodes that have outgoing links
    #[must_use]
    pub fn outlink_count(&self) -> usize {
        self.outlinks.len()
    }

    /// Get the number of nodes that have incoming links
    #[must_use]
    pub fn backlink_count(&self) -> usize {
        self.backlinks.len()
    }

    /// Check if the graph is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outlinks.is_empty() && self.backlinks.is_empty()
    }

    /// Clear the graph
    pub fn clear(&mut self) {
        self.outlinks.clear();
        self.backlinks.clear();
    }
}

impl Default for InMemoryGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphView for InMemoryGraph {
    /// Get all notes this note links to
    ///
    /// Returns an empty vector if the path has no outgoing links.
    fn outlinks(&self, path: &str) -> Vec<String> {
        self.outlinks.get(path).cloned().unwrap_or_default()
    }

    /// Get all notes that link to this note
    ///
    /// Returns an empty vector if no notes link to this path.
    fn backlinks(&self, path: &str) -> Vec<String> {
        self.backlinks.get(path).cloned().unwrap_or_default()
    }

    /// Get all neighbors within a given depth using BFS traversal
    ///
    /// Traverses both outlinks and backlinks to find all notes reachable
    /// within the specified depth. The starting note is not included in
    /// the results.
    ///
    /// # Arguments
    ///
    /// * `path` - Starting note path
    /// * `depth` - Maximum link distance (1 = direct links only)
    ///
    /// # Performance
    ///
    /// This is O(n) where n is the number of nodes visited. For depth=1,
    /// this is O(k) where k is the number of direct neighbors.
    fn neighbors(&self, path: &str, depth: usize) -> Vec<String> {
        if depth == 0 {
            return Vec::new();
        }

        let mut visited = HashSet::new();
        let mut queue: VecDeque<(&str, usize)> = VecDeque::new();

        // Mark the starting node as visited but don't include it in results
        visited.insert(path.to_string());
        queue.push_back((path, 0));

        while let Some((current, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }

            // Get both outlinks and backlinks
            let outlinks = self.outlinks.get(current);
            let backlinks = self.backlinks.get(current);

            // Process outlinks
            if let Some(links) = outlinks {
                for link in links {
                    if visited.insert(link.clone()) {
                        queue.push_back((link, current_depth + 1));
                    }
                }
            }

            // Process backlinks
            if let Some(links) = backlinks {
                for link in links {
                    if visited.insert(link.clone()) {
                        queue.push_back((link, current_depth + 1));
                    }
                }
            }
        }

        // Remove the starting node and collect results
        visited.remove(path);
        visited.into_iter().collect()
    }

    /// Rebuild the graph from note records
    ///
    /// This clears both maps and rebuilds them from scratch. The algorithm:
    /// 1. Clear existing data
    /// 2. Iterate through all notes, building the outlinks map
    /// 3. Simultaneously build the backlinks map (inverse of outlinks)
    fn rebuild(&mut self, notes: &[NoteRecord]) {
        self.outlinks.clear();
        self.backlinks.clear();

        for note in notes {
            let source = &note.path;

            // Skip notes with no outgoing links
            if note.links_to.is_empty() {
                continue;
            }

            // Store outlinks
            self.outlinks.insert(source.clone(), note.links_to.clone());

            // Build backlinks (inverse relationship)
            for target in &note.links_to {
                self.backlinks
                    .entry(target.clone())
                    .or_default()
                    .push(source.clone());
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::BlockHash;

    /// Create a test note with the given path and links
    fn note(path: &str, links: Vec<&str>) -> NoteRecord {
        NoteRecord::new(path, BlockHash::zero())
            .with_links(links.into_iter().map(String::from).collect())
    }

    // ------------------------------------------------------------------------
    // Construction tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_new_creates_empty_graph() {
        let graph = InMemoryGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.outlink_count(), 0);
        assert_eq!(graph.backlink_count(), 0);
    }

    #[test]
    fn test_default_creates_empty_graph() {
        let graph = InMemoryGraph::default();
        assert!(graph.is_empty());
    }

    #[test]
    fn test_from_notes_builds_graph() {
        let notes = vec![
            note("a.md", vec!["b.md"]),
            note("b.md", vec!["c.md"]),
            note("c.md", vec![]),
        ];

        let graph = InMemoryGraph::from_notes(&notes);
        assert!(!graph.is_empty());
        assert_eq!(graph.outlink_count(), 2); // a.md, b.md have outlinks
        assert_eq!(graph.backlink_count(), 2); // b.md, c.md have backlinks
    }

    // ------------------------------------------------------------------------
    // Empty graph handling
    // ------------------------------------------------------------------------

    #[test]
    fn test_outlinks_on_empty_graph() {
        let graph = InMemoryGraph::new();
        assert!(graph.outlinks("any.md").is_empty());
    }

    #[test]
    fn test_backlinks_on_empty_graph() {
        let graph = InMemoryGraph::new();
        assert!(graph.backlinks("any.md").is_empty());
    }

    #[test]
    fn test_neighbors_on_empty_graph() {
        let graph = InMemoryGraph::new();
        assert!(graph.neighbors("any.md", 3).is_empty());
    }

    #[test]
    fn test_outlinks_for_nonexistent_path() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let graph = InMemoryGraph::from_notes(&notes);
        assert!(graph.outlinks("nonexistent.md").is_empty());
    }

    #[test]
    fn test_backlinks_for_nonexistent_path() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let graph = InMemoryGraph::from_notes(&notes);
        assert!(graph.backlinks("nonexistent.md").is_empty());
    }

    // ------------------------------------------------------------------------
    // Outlinks tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_outlinks_single_link() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let graph = InMemoryGraph::from_notes(&notes);
        assert_eq!(graph.outlinks("a.md"), vec!["b.md"]);
    }

    #[test]
    fn test_outlinks_multiple_links() {
        let notes = vec![note("a.md", vec!["b.md", "c.md", "d.md"])];
        let graph = InMemoryGraph::from_notes(&notes);

        let outlinks = graph.outlinks("a.md");
        assert_eq!(outlinks.len(), 3);
        assert!(outlinks.contains(&"b.md".to_string()));
        assert!(outlinks.contains(&"c.md".to_string()));
        assert!(outlinks.contains(&"d.md".to_string()));
    }

    #[test]
    fn test_outlinks_no_links() {
        let notes = vec![note("a.md", vec![])];
        let graph = InMemoryGraph::from_notes(&notes);
        assert!(graph.outlinks("a.md").is_empty());
    }

    // ------------------------------------------------------------------------
    // Backlinks tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_backlinks_single_source() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let graph = InMemoryGraph::from_notes(&notes);
        assert_eq!(graph.backlinks("b.md"), vec!["a.md"]);
    }

    #[test]
    fn test_backlinks_multiple_sources() {
        let notes = vec![
            note("a.md", vec!["target.md"]),
            note("b.md", vec!["target.md"]),
            note("c.md", vec!["target.md"]),
        ];
        let graph = InMemoryGraph::from_notes(&notes);

        let backlinks = graph.backlinks("target.md");
        assert_eq!(backlinks.len(), 3);
        assert!(backlinks.contains(&"a.md".to_string()));
        assert!(backlinks.contains(&"b.md".to_string()));
        assert!(backlinks.contains(&"c.md".to_string()));
    }

    #[test]
    fn test_backlinks_no_incoming() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let graph = InMemoryGraph::from_notes(&notes);
        // a.md has no incoming links
        assert!(graph.backlinks("a.md").is_empty());
    }

    // ------------------------------------------------------------------------
    // Neighbors BFS tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_neighbors_depth_zero() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let graph = InMemoryGraph::from_notes(&notes);
        assert!(graph.neighbors("a.md", 0).is_empty());
    }

    #[test]
    fn test_neighbors_depth_one_outlinks_only() {
        // a -> b, a -> c
        let notes = vec![
            note("a.md", vec!["b.md", "c.md"]),
            note("b.md", vec![]),
            note("c.md", vec![]),
        ];
        let graph = InMemoryGraph::from_notes(&notes);

        let neighbors = graph.neighbors("a.md", 1);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"b.md".to_string()));
        assert!(neighbors.contains(&"c.md".to_string()));
    }

    #[test]
    fn test_neighbors_depth_one_backlinks_only() {
        // a -> b, c -> b (so b has backlinks from a and c)
        let notes = vec![
            note("a.md", vec!["b.md"]),
            note("c.md", vec!["b.md"]),
            note("b.md", vec![]),
        ];
        let graph = InMemoryGraph::from_notes(&notes);

        let neighbors = graph.neighbors("b.md", 1);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"a.md".to_string()));
        assert!(neighbors.contains(&"c.md".to_string()));
    }

    #[test]
    fn test_neighbors_depth_one_both_directions() {
        // a -> b -> c (so b has outlink c and backlink a)
        let notes = vec![
            note("a.md", vec!["b.md"]),
            note("b.md", vec!["c.md"]),
            note("c.md", vec![]),
        ];
        let graph = InMemoryGraph::from_notes(&notes);

        let neighbors = graph.neighbors("b.md", 1);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"a.md".to_string())); // backlink
        assert!(neighbors.contains(&"c.md".to_string())); // outlink
    }

    #[test]
    fn test_neighbors_depth_two() {
        // a -> b -> c -> d
        let notes = vec![
            note("a.md", vec!["b.md"]),
            note("b.md", vec!["c.md"]),
            note("c.md", vec!["d.md"]),
            note("d.md", vec![]),
        ];
        let graph = InMemoryGraph::from_notes(&notes);

        // From a.md at depth 2:
        // depth 1: b.md (outlink)
        // depth 2: c.md (via b.md outlink), a.md is excluded (starting node)
        let neighbors = graph.neighbors("a.md", 2);
        assert!(neighbors.contains(&"b.md".to_string()));
        assert!(neighbors.contains(&"c.md".to_string()));
        assert!(!neighbors.contains(&"d.md".to_string())); // depth 3
        assert!(!neighbors.contains(&"a.md".to_string())); // starting node
    }

    #[test]
    fn test_neighbors_does_not_include_self() {
        let notes = vec![note("a.md", vec!["a.md"])]; // self-link
        let graph = InMemoryGraph::from_notes(&notes);

        let neighbors = graph.neighbors("a.md", 1);
        assert!(neighbors.is_empty()); // self not included
    }

    #[test]
    fn test_neighbors_handles_cycles() {
        // a -> b -> c -> a (cycle)
        let notes = vec![
            note("a.md", vec!["b.md"]),
            note("b.md", vec!["c.md"]),
            note("c.md", vec!["a.md"]),
        ];
        let graph = InMemoryGraph::from_notes(&notes);

        // Depth 3 should not revisit nodes
        let neighbors = graph.neighbors("a.md", 3);
        assert_eq!(neighbors.len(), 2); // b.md and c.md
        assert!(neighbors.contains(&"b.md".to_string()));
        assert!(neighbors.contains(&"c.md".to_string()));
    }

    #[test]
    fn test_neighbors_large_depth() {
        // a -> b -> c
        let notes = vec![
            note("a.md", vec!["b.md"]),
            note("b.md", vec!["c.md"]),
            note("c.md", vec![]),
        ];
        let graph = InMemoryGraph::from_notes(&notes);

        // Large depth should still work correctly
        let neighbors = graph.neighbors("a.md", 100);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"b.md".to_string()));
        assert!(neighbors.contains(&"c.md".to_string()));
    }

    // ------------------------------------------------------------------------
    // Rebuild tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_rebuild_clears_previous_data() {
        let notes1 = vec![note("a.md", vec!["b.md"])];
        let mut graph = InMemoryGraph::from_notes(&notes1);

        assert_eq!(graph.outlinks("a.md"), vec!["b.md"]);

        // Rebuild with different data
        let notes2 = vec![note("x.md", vec!["y.md"])];
        graph.rebuild(&notes2);

        // Old data should be gone
        assert!(graph.outlinks("a.md").is_empty());
        assert!(graph.backlinks("b.md").is_empty());

        // New data should be present
        assert_eq!(graph.outlinks("x.md"), vec!["y.md"]);
        assert_eq!(graph.backlinks("y.md"), vec!["x.md"]);
    }

    #[test]
    fn test_rebuild_with_empty_notes() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let mut graph = InMemoryGraph::from_notes(&notes);

        graph.rebuild(&[]);

        assert!(graph.is_empty());
        assert!(graph.outlinks("a.md").is_empty());
        assert!(graph.backlinks("b.md").is_empty());
    }

    #[test]
    fn test_rebuild_updates_both_maps() {
        let mut graph = InMemoryGraph::new();

        let notes = vec![
            note("a.md", vec!["b.md", "c.md"]),
            note("b.md", vec!["c.md"]),
        ];
        graph.rebuild(&notes);

        // Check outlinks
        assert_eq!(graph.outlinks("a.md").len(), 2);
        assert_eq!(graph.outlinks("b.md"), vec!["c.md"]);

        // Check backlinks
        assert_eq!(graph.backlinks("b.md"), vec!["a.md"]);

        let c_backlinks = graph.backlinks("c.md");
        assert_eq!(c_backlinks.len(), 2);
        assert!(c_backlinks.contains(&"a.md".to_string()));
        assert!(c_backlinks.contains(&"b.md".to_string()));
    }

    // ------------------------------------------------------------------------
    // Clear test
    // ------------------------------------------------------------------------

    #[test]
    fn test_clear() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let mut graph = InMemoryGraph::from_notes(&notes);

        assert!(!graph.is_empty());

        graph.clear();

        assert!(graph.is_empty());
        assert!(graph.outlinks("a.md").is_empty());
        assert!(graph.backlinks("b.md").is_empty());
    }

    // ------------------------------------------------------------------------
    // Clone test
    // ------------------------------------------------------------------------

    #[test]
    fn test_clone() {
        let notes = vec![note("a.md", vec!["b.md"])];
        let graph = InMemoryGraph::from_notes(&notes);

        let cloned = graph.clone();

        assert_eq!(cloned.outlinks("a.md"), graph.outlinks("a.md"));
        assert_eq!(cloned.backlinks("b.md"), graph.backlinks("b.md"));
    }
}
