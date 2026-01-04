//! In-memory graph view for SQLite backend
//!
//! Provides fast graph traversal by maintaining an in-memory index of links.
//! The graph is built from [`NoteRecord`]s and must be rebuilt when notes change.

use std::collections::{HashMap, HashSet, VecDeque};

use crucible_core::storage::note_store::{GraphView, NoteRecord};

/// In-memory graph built from note links
///
/// Maintains bidirectional indices for fast outlink and backlink lookups.
/// Call `rebuild` after bulk updates to the NoteStore.
#[derive(Debug, Default)]
pub struct SqliteGraphView {
    /// Map from path -> paths this note links to
    outlinks: HashMap<String, Vec<String>>,

    /// Map from path -> paths that link to this note
    backlinks: HashMap<String, Vec<String>>,
}

impl SqliteGraphView {
    /// Create a new empty graph view
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a graph view from an iterator of note records
    pub fn from_notes(notes: impl IntoIterator<Item = NoteRecord>) -> Self {
        let mut view = Self::new();
        let notes: Vec<_> = notes.into_iter().collect();
        view.rebuild(&notes);
        view
    }
}

impl GraphView for SqliteGraphView {
    fn outlinks(&self, path: &str) -> Vec<String> {
        self.outlinks.get(path).cloned().unwrap_or_default()
    }

    fn backlinks(&self, path: &str) -> Vec<String> {
        self.backlinks.get(path).cloned().unwrap_or_default()
    }

    fn neighbors(&self, path: &str, depth: usize) -> Vec<String> {
        if depth == 0 {
            return Vec::new();
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Start with direct neighbors
        visited.insert(path.to_string());
        queue.push_back((path.to_string(), 0));

        while let Some((current, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }

            // Add outlinks
            if let Some(links) = self.outlinks.get(&current) {
                for link in links {
                    if visited.insert(link.clone()) {
                        queue.push_back((link.clone(), current_depth + 1));
                    }
                }
            }

            // Add backlinks
            if let Some(links) = self.backlinks.get(&current) {
                for link in links {
                    if visited.insert(link.clone()) {
                        queue.push_back((link.clone(), current_depth + 1));
                    }
                }
            }
        }

        // Remove the starting node from results
        visited.remove(path);
        visited.into_iter().collect()
    }

    fn rebuild(&mut self, notes: &[NoteRecord]) {
        self.outlinks.clear();
        self.backlinks.clear();

        for note in notes {
            // Store outlinks
            if !note.links_to.is_empty() {
                self.outlinks
                    .insert(note.path.clone(), note.links_to.clone());
            }

            // Build backlinks index
            for target in &note.links_to {
                self.backlinks
                    .entry(target.clone())
                    .or_default()
                    .push(note.path.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crucible_core::parser::BlockHash;

    fn make_note(path: &str, links_to: Vec<&str>) -> NoteRecord {
        NoteRecord {
            path: path.to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: path.to_string(),
            tags: vec![],
            links_to: links_to.into_iter().map(String::from).collect(),
            properties: Default::default(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_empty_graph() {
        let graph = SqliteGraphView::new();
        assert!(graph.outlinks("any").is_empty());
        assert!(graph.backlinks("any").is_empty());
        assert!(graph.neighbors("any", 1).is_empty());
    }

    #[test]
    fn test_outlinks() {
        let notes = vec![
            make_note("a.md", vec!["b.md", "c.md"]),
            make_note("b.md", vec!["c.md"]),
            make_note("c.md", vec![]),
        ];

        let graph = SqliteGraphView::from_notes(notes);

        assert_eq!(graph.outlinks("a.md"), vec!["b.md", "c.md"]);
        assert_eq!(graph.outlinks("b.md"), vec!["c.md"]);
        assert!(graph.outlinks("c.md").is_empty());
    }

    #[test]
    fn test_backlinks() {
        let notes = vec![
            make_note("a.md", vec!["c.md"]),
            make_note("b.md", vec!["c.md"]),
            make_note("c.md", vec![]),
        ];

        let graph = SqliteGraphView::from_notes(notes);

        let backlinks = graph.backlinks("c.md");
        assert_eq!(backlinks.len(), 2);
        assert!(backlinks.contains(&"a.md".to_string()));
        assert!(backlinks.contains(&"b.md".to_string()));
    }

    #[test]
    fn test_neighbors_depth_1() {
        // a -> b -> c
        let notes = vec![
            make_note("a.md", vec!["b.md"]),
            make_note("b.md", vec!["c.md"]),
            make_note("c.md", vec![]),
        ];

        let graph = SqliteGraphView::from_notes(notes);

        let neighbors = graph.neighbors("a.md", 1);
        assert_eq!(neighbors.len(), 1);
        assert!(neighbors.contains(&"b.md".to_string()));
    }

    #[test]
    fn test_neighbors_depth_2() {
        // a -> b -> c
        let notes = vec![
            make_note("a.md", vec!["b.md"]),
            make_note("b.md", vec!["c.md"]),
            make_note("c.md", vec![]),
        ];

        let graph = SqliteGraphView::from_notes(notes);

        let neighbors = graph.neighbors("a.md", 2);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"b.md".to_string()));
        assert!(neighbors.contains(&"c.md".to_string()));
    }

    #[test]
    fn test_neighbors_includes_backlinks() {
        // a -> b, c -> b
        let notes = vec![
            make_note("a.md", vec!["b.md"]),
            make_note("b.md", vec![]),
            make_note("c.md", vec!["b.md"]),
        ];

        let graph = SqliteGraphView::from_notes(notes);

        // From b, we should reach both a and c via backlinks
        let neighbors = graph.neighbors("b.md", 1);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&"a.md".to_string()));
        assert!(neighbors.contains(&"c.md".to_string()));
    }

    #[test]
    fn test_neighbors_depth_0() {
        let notes = vec![make_note("a.md", vec!["b.md"])];
        let graph = SqliteGraphView::from_notes(notes);

        assert!(graph.neighbors("a.md", 0).is_empty());
    }

    #[test]
    fn test_rebuild_clears_old_data() {
        let mut graph = SqliteGraphView::from_notes(vec![make_note("a.md", vec!["b.md"])]);

        assert_eq!(graph.outlinks("a.md"), vec!["b.md"]);

        // Rebuild with different data
        graph.rebuild(&[make_note("x.md", vec!["y.md"])]);

        assert!(graph.outlinks("a.md").is_empty());
        assert_eq!(graph.outlinks("x.md"), vec!["y.md"]);
    }

    #[test]
    fn test_circular_links() {
        // a -> b -> c -> a (cycle)
        let notes = vec![
            make_note("a.md", vec!["b.md"]),
            make_note("b.md", vec!["c.md"]),
            make_note("c.md", vec!["a.md"]),
        ];

        let graph = SqliteGraphView::from_notes(notes);

        // Should not infinite loop
        let neighbors = graph.neighbors("a.md", 10);
        assert_eq!(neighbors.len(), 2); // b and c
    }
}
