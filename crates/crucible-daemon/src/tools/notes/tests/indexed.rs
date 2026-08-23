//! The index path of `read_metadata` and `list_notes`.
//!
//! A note the SQLite store knows is answered from the store. A note the store
//! does not know is answered from disk. Both paths serve the same kiln, so a
//! test can put one note in each and ask for both.

use super::super::{ListNotesParams, NoteTools, ReadMetadataParams};
use crate::storage::sqlite::config::SqliteConfig;
use crate::storage::sqlite::connection::SqlitePool;
use crate::storage::sqlite::note_store::SqliteNoteStore;
use crate::storage::sqlite::repository::SqliteKnowledgeRepository;
use chrono::{TimeZone, Utc};
use crucible_core::parser::BlockHash;
use crucible_core::storage::note_store::NoteRecord;
use crucible_core::storage::NoteStore;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use std::sync::Arc;
use tempfile::TempDir;

fn parse(result: CallToolResult) -> serde_json::Value {
    let text = result.content.first().unwrap().as_text().unwrap();
    serde_json::from_str(&text.text).unwrap()
}

/// A kiln with two notes on disk, one of which the store indexed with a
/// property the file does not carry — so the reply tells which path served it.
async fn kiln_with_one_indexed_note() -> (TempDir, NoteTools) {
    let kiln = TempDir::new().unwrap();
    std::fs::create_dir_all(kiln.path().join("notes")).unwrap();
    std::fs::write(
        kiln.path().join("notes/indexed.md"),
        "---\ntitle: On Disk\n---\n# Indexed\n\nbody",
    )
    .unwrap();
    std::fs::write(
        kiln.path().join("notes/fresh.md"),
        "---\ntitle: Fresh\nstatus: draft\n---\n# Fresh\n\none two three",
    )
    .unwrap();

    let store = Arc::new(SqliteNoteStore::new(
        SqlitePool::new(SqliteConfig::memory()).unwrap(),
    ));
    store
        .upsert(NoteRecord {
            path: "notes/indexed.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: Some(vec![1.0]),
            embedding_model: None,
            embedding_dimensions: None,
            title: "From Index".to_string(),
            tags: vec!["indexed".to_string()],
            links_to: vec!["notes/fresh.md".to_string()],
            links: Vec::new(),
            properties: [("status".to_string(), serde_json::json!("final"))]
                .into_iter()
                .collect(),
            updated_at: Utc.with_ymd_and_hms(2024, 1, 2, 3, 4, 5).unwrap(),
        })
        .await
        .unwrap();

    let repo = SqliteKnowledgeRepository::with_kiln_path(store, kiln.path().to_path_buf());
    let tools = NoteTools::new(kiln.path().to_string_lossy().to_string(), Arc::new(repo));
    (kiln, tools)
}

#[tokio::test]
async fn read_metadata_answers_from_the_store_when_the_note_is_indexed() {
    let (_kiln, tools) = kiln_with_one_indexed_note().await;

    let reply = parse(
        tools
            .read_metadata(Parameters(ReadMetadataParams {
                path: "notes/indexed".to_string(),
            }))
            .await
            .unwrap(),
    );

    assert_eq!(reply["source"], "index");
    assert_eq!(reply["frontmatter"]["title"], "From Index");
    assert_eq!(reply["frontmatter"]["status"], "final");
    assert_eq!(reply["frontmatter"]["tags"], serde_json::json!(["indexed"]));
    assert_eq!(reply["stats"]["links_count"], 1);
    assert_eq!(reply["stats"]["tags_count"], 1);
    assert_eq!(reply["stats"]["has_embedding"], true);
    assert_eq!(reply["modified"], 1_704_164_645);
}

#[tokio::test]
async fn read_metadata_answers_from_disk_when_the_note_is_not_indexed() {
    let (_kiln, tools) = kiln_with_one_indexed_note().await;

    let reply = parse(
        tools
            .read_metadata(Parameters(ReadMetadataParams {
                path: "notes/fresh.md".to_string(),
            }))
            .await
            .unwrap(),
    );

    assert_eq!(reply["source"], "disk");
    assert_eq!(reply["frontmatter"]["title"], "Fresh");
    assert_eq!(reply["frontmatter"]["status"], "draft");
    assert!(reply["stats"]["word_count"].as_u64().unwrap() > 0);
    assert_eq!(reply["stats"]["heading_count"], 1);
}

#[tokio::test]
async fn list_notes_serves_each_note_from_where_it_is_known() {
    let (_kiln, tools) = kiln_with_one_indexed_note().await;

    let reply = parse(
        tools
            .list_notes(Parameters(ListNotesParams {
                folder: Some("notes".to_string()),
                include_frontmatter: true,
                recursive: false,
            }))
            .await
            .unwrap(),
    );

    let notes = reply["notes"].as_array().unwrap();
    assert_eq!(reply["count"], 2);
    let by_path = |p: &str| notes.iter().find(|n| n["path"] == p).unwrap().clone();

    let indexed = by_path("notes/indexed.md");
    assert_eq!(indexed["source"], "index");
    assert_eq!(indexed["frontmatter"]["title"], "From Index");
    assert_eq!(indexed["frontmatter"]["status"], "final");
    assert_eq!(indexed["modified"], 1_704_164_645);

    let fresh = by_path("notes/fresh.md");
    assert_eq!(fresh["source"], "disk");
    assert_eq!(fresh["frontmatter"]["status"], "draft");
    assert!(fresh["word_count"].as_u64().unwrap() > 0);
}

/// An index row for a file that is gone is stale, not an answer.
#[tokio::test]
async fn a_deleted_file_is_not_answered_from_its_stale_index_row() {
    let (kiln, tools) = kiln_with_one_indexed_note().await;
    std::fs::remove_file(kiln.path().join("notes/indexed.md")).unwrap();

    let err = tools
        .read_metadata(Parameters(ReadMetadataParams {
            path: "notes/indexed.md".to_string(),
        }))
        .await
        .unwrap_err();
    assert!(err.message.contains("File not found"), "{err:?}");

    let reply = parse(
        tools
            .list_notes(Parameters(ListNotesParams {
                folder: None,
                include_frontmatter: false,
                recursive: true,
            }))
            .await
            .unwrap(),
    );
    assert_eq!(reply["count"], 1);
    assert_eq!(reply["notes"][0]["path"], "notes/fresh.md");
}
