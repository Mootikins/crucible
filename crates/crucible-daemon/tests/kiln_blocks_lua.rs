//! `cru.kiln.blocks` reads the SQLite block store the pipeline wrote.
//!
//! The crate that registers the binding cannot build a block store, so the
//! test that proves Lua reaches stored vectors lives here, over the real
//! pipeline with the fixture embedder.

use std::sync::Arc;

use crucible_core::storage::{BlockStore, NoteStore};
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::enrichment::Enricher;
use crucible_daemon::llm::embeddings::FixtureEmbeddingProvider;
use crucible_daemon::pipeline::{NotePipeline, NotePipelineConfig};
use crucible_daemon::storage::sqlite::{
    SqliteBlockStore, SqliteKnowledgeRepository, SqliteNoteStore, SqlitePool,
};

const NOTE: &str = "# Title\n\nAlpha has enough words here for embedding.\n\n## Section\n\nBeta has enough words here for embedding too.\n";

#[tokio::test]
async fn cru_kiln_blocks_reads_the_stored_vectors_in_span_order() {
    let pool = SqlitePool::memory().expect("in-memory pool");
    let notes = Arc::new(SqliteNoteStore::new(pool.clone()));
    let blocks: Arc<dyn BlockStore> = Arc::new(SqliteBlockStore::new(pool));
    let enricher = Arc::new(
        Enricher::new(Arc::new(FixtureEmbeddingProvider::with_dimensions(8)))
            .with_block_cache(Arc::clone(&blocks)),
    );
    let note_store: Arc<dyn NoteStore> = notes.clone();
    let pipeline = NotePipeline::with_config(
        enricher,
        note_store,
        NotePipelineConfig {
            skip_enrichment: false,
            force_reprocess: true,
        },
    )
    .with_block_store(Arc::clone(&blocks));

    let dir = tempfile::TempDir::new().expect("tempdir");
    let file = dir.path().join("a.md");
    std::fs::write(&file, NOTE).expect("write note");
    pipeline.process(&file).await.expect("process");
    let note_path = file.to_string_lossy().to_string();

    let stored = blocks.blocks_for_note(&note_path).await.expect("stored");
    assert_eq!(stored.len(), 4, "the fixture note has four blocks");

    let repo: Arc<dyn KnowledgeRepository> =
        Arc::new(SqliteKnowledgeRepository::new(notes).with_block_store(blocks));
    let loader = crucible_daemon::daemon_plugins::DaemonPluginLoader::new(Default::default())
        .expect("plugin loader");
    let resolver: crucible_lua::KilnRepositoryResolver = Arc::new(move |name: &str| {
        if name == "notes" {
            Ok(Arc::clone(&repo))
        } else {
            Err(format!("kiln '{name}' is not attached"))
        }
    });
    crucible_lua::register_kiln_blocks_resolver(loader.executor().lua(), resolver)
        .expect("register the resolver");

    let lua = loader.plugin_lua();
    let rows: Vec<mlua::Table> = lua
        .load(format!(
            r#"return cru.kiln.blocks("notes", {})"#,
            serde_json::to_string(&note_path).expect("json string")
        ))
        .eval_async()
        .await
        .expect("cru.kiln.blocks");

    let kinds: Vec<String> = rows.iter().map(|r| r.get("kind").unwrap()).collect();
    assert_eq!(kinds, vec!["heading", "paragraph", "heading", "paragraph"]);
    let starts: Vec<usize> = rows.iter().map(|r| r.get("span_start").unwrap()).collect();
    assert!(starts.windows(2).all(|w| w[0] < w[1]), "{starts:?}");

    // The prose blocks carry the vector the pipeline stored; the headings,
    // under the word floor, carry none.
    let heading: mlua::Value = rows[0].get("vector").unwrap();
    assert!(heading.is_nil());
    let vector: Vec<f32> = rows[1].get("vector").unwrap();
    assert_eq!(Some(&vector), stored[1].embedding.as_ref());
    assert_eq!(vector.len(), 8);
}
