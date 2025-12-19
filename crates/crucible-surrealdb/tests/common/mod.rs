//! Common test fixtures for crucible-surrealdb integration tests.
//!
//! This module provides shared test infrastructure for setting up databases
//! with the test-kiln example vault.
//!
//! Requires the `test-utils` feature to be enabled.

use crucible_core::parser::MarkdownParser;
use crucible_parser::CrucibleParser;
use crucible_surrealdb::test_utils::{
    apply_eav_graph_schema, EAVGraphStore, NoteIngestor, SurrealClient,
};
use std::path::PathBuf;
use walkdir::WalkDir;

/// Get the path to the test-kiln example vault.
pub fn test_kiln_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("examples/test-kiln")
}

/// Find all markdown files in the test-kiln.
pub fn find_test_kiln_markdown_files() -> Vec<PathBuf> {
    let root = test_kiln_root();
    WalkDir::new(&root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Count markdown files in the test-kiln.
pub fn count_kiln_files() -> usize {
    find_test_kiln_markdown_files().len()
}

/// Set up an in-memory test database with the test-kiln data fully ingested.
///
/// This function:
/// 1. Creates an in-memory SurrealDB instance
/// 2. Applies the EAV graph schema
/// 3. Parses all markdown files in test-kiln
/// 4. Ingests them into the database (including wikilinks, tags, blocks, etc.)
///
/// # Returns
/// The configured SurrealClient ready for queries.
pub async fn setup_test_db_with_kiln() -> anyhow::Result<SurrealClient> {
    let client = SurrealClient::new_memory().await?;
    apply_eav_graph_schema(&client).await?;
    let store = EAVGraphStore::new(client.clone());
    let ingestor = NoteIngestor::new(&store);
    let parser = CrucibleParser::with_default_extensions();

    // Parse and ingest all test-kiln files
    let md_files = find_test_kiln_markdown_files();
    for file_path in &md_files {
        let note = parser.parse_file(file_path).await?;
        let relative_path = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        ingestor.ingest(&note, &relative_path).await?;
    }

    Ok(client)
}

/// Set up an in-memory test database without ingesting any data.
///
/// Use this when you want to test ingestion behavior directly.
pub async fn setup_empty_test_db() -> anyhow::Result<(SurrealClient, EAVGraphStore)> {
    let client = SurrealClient::new_memory().await?;
    apply_eav_graph_schema(&client).await?;
    let store = EAVGraphStore::new(client.clone());
    Ok((client, store))
}

/// Configuration for test-kiln fixture.
#[derive(Debug, Clone)]
pub struct TestKilnConfig {
    /// Path to the test-kiln root.
    pub root: PathBuf,
    /// List of markdown files in the kiln.
    pub files: Vec<PathBuf>,
    /// Number of files.
    pub file_count: usize,
}

impl TestKilnConfig {
    /// Create a new TestKilnConfig by scanning the test-kiln directory.
    pub fn new() -> Self {
        let root = test_kiln_root();
        let files = find_test_kiln_markdown_files();
        let file_count = files.len();
        Self {
            root,
            files,
            file_count,
        }
    }

    /// Assert that the test-kiln exists and has expected files.
    pub fn assert_valid(&self) {
        assert!(
            self.root.exists(),
            "Test kiln should exist at: {}",
            self.root.display()
        );
        assert!(
            self.file_count >= 10,
            "Test kiln should have at least 10 files, found: {}",
            self.file_count
        );
    }
}

impl Default for TestKilnConfig {
    fn default() -> Self {
        Self::new()
    }
}
