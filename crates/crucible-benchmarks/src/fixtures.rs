//! Test fixtures for benchmarks
//!
//! Provides graph and note generation with realistic link patterns.

use crucible_core::parser::BlockHash;
use crucible_core::storage::NoteRecord;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::SeedableRng;

/// Default embedding dimension for benchmarks
pub const EMBEDDING_DIM: usize = 384;

/// Graph statistics for validation
#[derive(Debug, Clone)]
pub struct GraphStats {
    pub total_notes: usize,
    pub total_links: usize,
    pub hub_notes: Vec<String>,
    pub orphan_notes: Vec<String>,
    pub avg_links_per_note: f64,
}

/// A generated graph fixture with notes and metadata
#[derive(Debug, Clone)]
pub struct GraphFixture {
    pub notes: Vec<NoteRecord>,
    pub stats: GraphStats,
}

/// Generate a random embedding vector
pub fn random_embedding(rng: &mut impl Rng, dim: usize) -> Vec<f32> {
    (0..dim).map(|_| rng.random::<f32>()).collect()
}

/// Generate a graph with power-law-ish link distribution
///
/// # Arguments
///
/// * `note_count` - Number of notes to generate
/// * `avg_links_per_note` - Average number of outlinks per note
/// * `hub_percentage` - Fraction of notes that are hubs (0.0 - 1.0)
/// * `seed` - RNG seed for reproducibility
///
/// # Returns
///
/// A `GraphFixture` with notes and statistics
pub fn generate_graph(
    note_count: usize,
    avg_links_per_note: usize,
    hub_percentage: f32,
    seed: u64,
) -> GraphFixture {
    let mut rng = StdRng::seed_from_u64(seed);

    // Determine which notes are hubs
    let hub_count = ((note_count as f32) * hub_percentage).ceil() as usize;
    let hub_indices: Vec<usize> = (0..hub_count).collect();

    // Generate notes with links
    let mut notes = Vec::with_capacity(note_count);
    let mut total_links = 0;
    let mut hub_notes = Vec::new();
    let mut orphan_notes = Vec::new();

    // Track inlinks to identify orphans later
    let mut inlink_counts = vec![0usize; note_count];

    for i in 0..note_count {
        let path = format!("notes/note-{:05}.md", i);
        let title = format!("Note {:05}", i);

        // Determine number of outlinks
        let is_hub = hub_indices.contains(&i);
        let link_count = if is_hub {
            // Hubs get 10-20x average links
            let multiplier = rng.random_range(10..=20);
            (avg_links_per_note * multiplier).min(note_count - 1)
        } else {
            // Regular notes get 0 to 2x average
            rng.random_range(0..=(avg_links_per_note * 2))
                .min(note_count - 1)
        };

        if is_hub {
            hub_notes.push(title.clone());
        }

        // Generate random link targets (excluding self)
        let mut link_targets: Vec<usize> = (0..note_count).filter(|&j| j != i).collect();
        link_targets.shuffle(&mut rng);
        link_targets.truncate(link_count);

        // Track inlinks
        for &target in &link_targets {
            inlink_counts[target] += 1;
        }

        let links: Vec<String> = link_targets
            .iter()
            .map(|&j| format!("notes/note-{:05}.md", j))
            .collect();

        total_links += links.len();

        // Generate note with embedding
        let hash_bytes: [u8; 32] = rng.random();
        let note = NoteRecord::new(path, BlockHash::new(hash_bytes))
            .with_title(title)
            .with_tags(vec![
                format!("tag-{}", i % 10),
                format!("category-{}", i % 5),
            ])
            .with_links(links)
            .with_embedding(random_embedding(&mut rng, EMBEDDING_DIM));

        notes.push(note);
    }

    // Identify orphans (notes with zero inlinks)
    for (i, &count) in inlink_counts.iter().enumerate() {
        if count == 0 {
            orphan_notes.push(format!("Note {:05}", i));
        }
    }

    let stats = GraphStats {
        total_notes: note_count,
        total_links,
        hub_notes,
        orphan_notes,
        avg_links_per_note: total_links as f64 / note_count as f64,
    };

    GraphFixture { notes, stats }
}

/// Pre-defined fixture sizes for benchmarks
pub mod sizes {
    /// Power user scale: 2K notes, ~10K links
    pub const POWER_USER: (usize, usize) = (2000, 5);

    /// Small team scale: 10K notes, ~50K links
    pub const SMALL_TEAM: (usize, usize) = (10000, 5);
}

/// Standard seeds for reproducibility
pub mod seeds {
    pub const DEFAULT: u64 = 42;
    pub const ALTERNATE: u64 = 1337;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_small_graph() {
        let fixture = generate_graph(100, 5, 0.05, seeds::DEFAULT);

        assert_eq!(fixture.notes.len(), 100);
        assert!(!fixture.stats.hub_notes.is_empty());
        // Average should be roughly what we asked for (±50%)
        assert!(fixture.stats.avg_links_per_note > 2.0);
        assert!(fixture.stats.avg_links_per_note < 10.0);
    }

    #[test]
    fn test_reproducibility() {
        let fixture1 = generate_graph(50, 3, 0.1, seeds::DEFAULT);
        let fixture2 = generate_graph(50, 3, 0.1, seeds::DEFAULT);

        // Same seed should produce same graph
        assert_eq!(fixture1.notes.len(), fixture2.notes.len());
        assert_eq!(fixture1.stats.total_links, fixture2.stats.total_links);
        assert_eq!(fixture1.notes[0].path, fixture2.notes[0].path);
    }

    #[test]
    fn test_hubs_have_more_links() {
        let fixture = generate_graph(100, 5, 0.1, seeds::DEFAULT);

        // Hubs should exist
        assert!(!fixture.stats.hub_notes.is_empty());

        // Find a hub note
        let hub_title = &fixture.stats.hub_notes[0];
        let hub_note = fixture
            .notes
            .iter()
            .find(|n| &n.title == hub_title)
            .unwrap();

        let non_hub_avg = {
            let non_hub_links: usize = fixture
                .notes
                .iter()
                .filter(|n| !fixture.stats.hub_notes.contains(&n.title))
                .map(|n| n.links_to.len())
                .sum();
            let non_hub_count = fixture.notes.len() - fixture.stats.hub_notes.len();
            non_hub_links as f64 / non_hub_count as f64
        };
        assert!(
            hub_note.links_to.len() > (non_hub_avg * 5.0) as usize,
            "Hub should have at least 5x non-hub average links (hub={}, non_hub_avg={:.1})",
            hub_note.links_to.len(),
            non_hub_avg,
        );
    }

    #[test]
    fn test_no_self_links() {
        let fixture = generate_graph(50, 5, 0.1, seeds::DEFAULT);

        for note in &fixture.notes {
            assert!(
                !note.links_to.contains(&note.path),
                "Note should not link to itself"
            );
        }
    }
}
