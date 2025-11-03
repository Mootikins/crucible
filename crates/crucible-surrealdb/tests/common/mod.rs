//! Common test utilities for SurrealDB integration tests
//!
//! This module provides helper functions to reduce boilerplate in tests,
//! particularly for creating notes, wikilinks, tags, and setting up test databases.

use chrono::Utc;
use crucible_core::parser::{DocumentContent, Frontmatter, FrontmatterFormat, ParsedDocument};
use crucible_core::CrucibleCore;
use crucible_surrealdb::{kiln_integration, SurrealClient};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Setup a test client with initialized schema
///
/// Returns a tuple of (SurrealClient, kiln_root PathBuf)
pub async fn setup_test_client() -> (SurrealClient, PathBuf) {
    let client = SurrealClient::new_memory()
        .await
        .expect("Failed to create in-memory client");

    // Initialize schema
    let _ = kiln_integration::initialize_kiln_schema(&client).await;

    let kiln_root = PathBuf::from("/test/kiln");

    (client, kiln_root)
}

/// Create a test Core instance with in-memory storage
///
/// Returns (Arc<CrucibleCore>, kiln_root_path)
pub async fn setup_test_core() -> (Arc<CrucibleCore>, PathBuf) {
    let client = SurrealClient::new_memory()
        .await
        .expect("Failed to create in-memory client");

    let _ = kiln_integration::initialize_kiln_schema(&client).await;

    let core = Arc::new(
        CrucibleCore::builder()
            .with_storage(client)
            .build()
            .expect("Failed to build core")
    );

    let kiln_root = PathBuf::from("/test/kiln");
    (core, kiln_root)
}

/// Create a test note with given path and content
///
/// # Arguments
/// * `client` - The SurrealDB client
/// * `path` - File path for the note (e.g., "Projects/test.md")
/// * `content` - Plain text content
/// * `kiln_root` - Root path for the kiln
///
/// # Returns
/// The record ID of the created note (e.g., "notes:Projects_test_md")
pub async fn create_test_note(
    client: &SurrealClient,
    path: &str,
    content: &str,
    kiln_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut doc = ParsedDocument::new(PathBuf::from(path));
    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.parsed_at = Utc::now();
    doc.content_hash = format!("hash_{}", path.replace(['/', '.'], "_"));
    doc.file_size = content.len() as u64;

    let record_id = kiln_integration::store_parsed_document(client, &doc, kiln_root).await?;

    Ok(record_id)
}

/// Create a test note with frontmatter metadata
///
/// # Arguments
/// * `client` - The SurrealDB client
/// * `path` - File path for the note
/// * `content` - Plain text content (body after frontmatter)
/// * `frontmatter` - YAML frontmatter string (without --- delimiters)
/// * `kiln_root` - Root path for the kiln
///
/// # Returns
/// The record ID of the created note
pub async fn create_test_note_with_frontmatter(
    client: &SurrealClient,
    path: &str,
    content: &str,
    frontmatter: &str,
    kiln_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut doc = ParsedDocument::new(PathBuf::from(path));
    doc.frontmatter = Some(Frontmatter::new(
        frontmatter.to_string(),
        FrontmatterFormat::Yaml,
    ));
    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.parsed_at = Utc::now();
    doc.content_hash = format!("hash_{}", path.replace(['/', '.'], "_"));
    doc.file_size = content.len() as u64;

    let record_id = kiln_integration::store_parsed_document(client, &doc, kiln_root).await?;

    Ok(record_id)
}

/// Create a wikilink between two notes
///
/// # Arguments
/// * `client` - The SurrealDB client
/// * `from_id` - Source note record ID (e.g., "notes:A_md")
/// * `to_id` - Target note record ID (e.g., "notes:B_md")
/// * `link_text` - The text of the wikilink (e.g., "B")
/// * `position` - Character position in source document
pub async fn create_wikilink(
    client: &SurrealClient,
    from_id: &str,
    to_id: &str,
    link_text: &str,
    position: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Escape single quotes in link_text
    let escaped_text = link_text.replace('\'', "''");

    let sql = format!(
        "RELATE {}->wikilink->{} SET link_text = '{}', position = {}",
        from_id, to_id, escaped_text, position
    );

    client.query(&sql, &[]).await?;

    Ok(())
}

/// Create a tag in the database
///
/// # Arguments
/// * `client` - The SurrealDB client
/// * `tag_name` - Name of the tag (e.g., "rust", "project")
pub async fn create_tag(
    client: &SurrealClient,
    tag_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tag_id = tag_name.replace(['/', '-', ' '], "_");
    // Escape single quotes in tag name
    let escaped_name = tag_name.replace('\'', "''");
    let sql = format!("CREATE tags:{} SET name = '{}'", tag_id, escaped_name);

    client.query(&sql, &[]).await?;

    Ok(())
}

/// Associate a tag with a note
///
/// # Arguments
/// * `client` - The SurrealDB client
/// * `note_id` - Note record ID (e.g., "notes:test_md")
/// * `tag_name` - Tag name (e.g., "rust")
pub async fn associate_tag(
    client: &SurrealClient,
    note_id: &str,
    tag_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tag_id = format!("tags:{}", tag_name.replace(['/', '-', ' '], "_"));
    let sql = format!("RELATE {}->tagged_with->{}", note_id, tag_id);

    client.query(&sql, &[]).await?;

    Ok(())
}

/// Convert a file path to a record ID format
///
/// # Example
/// ```
/// assert_eq!(path_to_record_id("Projects/test.md"), "Projects_test_md");
/// ```
pub fn path_to_record_id(path: &str) -> String {
    path.replace(['/', '.'], "_")
}

/// Execute a query and return the number of results
///
/// Useful for quick assertions about result counts
pub async fn count_query_results(
    client: &SurrealClient,
    query: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let result = client.query(query, &[]).await?;
    Ok(result.records.len())
}

/// Extract paths from query result
pub fn extract_paths(result: &crucible_surrealdb::QueryResult) -> Vec<String> {
    result.records.iter()
        .filter_map(|r| r.data.get("path"))
        .filter_map(|v| v.as_str())
        .map(String::from)
        .collect()
}

/// Count query results
pub fn count_results(result: &crucible_surrealdb::QueryResult) -> usize {
    result.records.len()
}

/// Create a linear chain of notes (A -> B -> C -> D)
///
/// # Arguments
/// * `client` - The SurrealDB client
/// * `names` - Array of note names (e.g., ["A.md", "B.md", "C.md"])
/// * `kiln_root` - Root path for the kiln
///
/// # Returns
/// Vector of record IDs in the same order as names
pub async fn create_linear_chain(
    client: &SurrealClient,
    names: &[&str],
    kiln_root: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut ids = Vec::new();

    for name in names {
        let id = create_test_note(client, name, &format!("Content {}", name), kiln_root).await?;
        ids.push(id);
    }

    for i in 0..(ids.len() - 1) {
        let link_text = names[i + 1].strip_suffix(".md").unwrap_or(names[i + 1]);
        create_wikilink(client, &ids[i], &ids[i + 1], link_text, 0).await?;
    }

    Ok(ids)
}

/// Create a cycle of notes (A -> B -> C -> A)
///
/// # Arguments
/// * `client` - The SurrealDB client
/// * `names` - Array of note names
/// * `kiln_root` - Root path for the kiln
///
/// # Returns
/// Vector of record IDs
pub async fn create_cycle(
    client: &SurrealClient,
    names: &[&str],
    kiln_root: &Path,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let ids = create_linear_chain(client, names, kiln_root).await?;

    // Close the cycle
    if ids.len() > 1 {
        let link_text = names[0].strip_suffix(".md").unwrap_or(names[0]);
        create_wikilink(client, ids.last().unwrap(), &ids[0], link_text, 0).await?;
    }

    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_record_id() {
        assert_eq!(path_to_record_id("test.md"), "test_md");
        assert_eq!(path_to_record_id("Projects/test.md"), "Projects_test_md");
        assert_eq!(
            path_to_record_id("Projects/Sub/test.md"),
            "Projects_Sub_test_md"
        );
    }

    #[tokio::test]
    async fn test_setup_test_client() {
        let (client, kiln_root) = setup_test_client().await;
        assert_eq!(kiln_root, PathBuf::from("/test/kiln"));

        // Verify we can query
        let result = client.query("SELECT * FROM notes", &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_setup_test_core() {
        let (core, kiln_root) = setup_test_core().await;
        assert_eq!(kiln_root, PathBuf::from("/test/kiln"));

        // Verify we can query through Core
        let result = core.query("SELECT * FROM notes").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_create_test_note() {
        let (client, kiln_root) = setup_test_client().await;

        let record_id = create_test_note(&client, "test.md", "Test content", &kiln_root)
            .await
            .unwrap();

        assert!(record_id.contains("test_md"));

        // Verify note was created
        let result = client
            .query(&format!("SELECT * FROM {}", record_id), &[])
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1);
    }

    #[tokio::test]
    async fn test_create_wikilink() {
        let (client, kiln_root) = setup_test_client().await;

        // Create two notes
        let id_a = create_test_note(&client, "A.md", "Note A", &kiln_root)
            .await
            .unwrap();
        let id_b = create_test_note(&client, "B.md", "Note B", &kiln_root)
            .await
            .unwrap();

        // Create wikilink
        create_wikilink(&client, &id_a, &id_b, "B", 0)
            .await
            .unwrap();

        // Verify wikilink exists
        let result = client
            .query(
                &format!("SELECT * FROM wikilink WHERE in = {}", id_a),
                &[],
            )
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1);
    }

    #[tokio::test]
    async fn test_create_tag_and_associate() {
        let (client, kiln_root) = setup_test_client().await;

        // Create note
        let note_id = create_test_note(&client, "tagged.md", "Tagged note", &kiln_root)
            .await
            .unwrap();

        // Create tag
        create_tag(&client, "rust").await.unwrap();

        // Associate tag
        associate_tag(&client, &note_id, "rust").await.unwrap();

        // Verify association
        let result = client
            .query(
                &format!("SELECT * FROM tagged_with WHERE in = {}", note_id),
                &[],
            )
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1);
    }
}
