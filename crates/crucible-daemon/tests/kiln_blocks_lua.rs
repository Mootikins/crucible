//! `cru.kiln.blocks` reads the SQLite block store the pipeline wrote.
//!
//! The crate that registers the binding cannot build a block store, so the
//! test that proves Lua reaches stored vectors lives here, over the
//! production loader, a registered kiln and the fixture embedder.

use std::sync::Arc;

use crucible_core::config::{EmbeddingProviderConfig, MockConfig};
use crucible_daemon::kiln_manager::KilnManager;
use crucible_daemon::test_support::kiln_registry;
use tokio::sync::broadcast;

const NOTE: &str = "# Title\n\nAlpha has enough words here for embedding.\n\n## Section\n\nBeta has enough words here for embedding too.\n";

#[tokio::test]
async fn cru_kiln_blocks_reads_the_stored_vectors_in_span_order() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let kiln_dir = dir.path().join("notes");
    std::fs::create_dir_all(kiln_dir.join(".crucible")).expect("kiln dir");
    let file = kiln_dir.join("a.md");
    std::fs::write(&file, NOTE).expect("write note");

    let registry = kiln_registry(&dir.path().join("data"), &[("notes", &kiln_dir)]);
    let (event_tx, _rx) = broadcast::channel(8);
    let embedder = EmbeddingProviderConfig::Mock(MockConfig {
        model: "mock".to_string(),
        dimensions: 8,
    });
    let kiln_manager = Arc::new(
        KilnManager::with_event_tx(event_tx, Some(embedder), 4096)
            .with_kiln_registry(Arc::clone(&registry)),
    );
    kiln_manager
        .process_file(&kiln_dir, &file)
        .await
        .expect("process");
    let handle = kiln_manager.get(&kiln_dir).await.expect("open");
    let stored = handle
        .as_block_store()
        .blocks_for_note("a.md")
        .await
        .expect("stored");
    assert_eq!(stored.len(), 4, "the fixture note has four blocks");

    let loader = crucible_daemon::daemon_plugins::DaemonPluginLoader::new(Default::default())
        .expect("plugin loader")
        .with_kiln_blocks_resolver(registry, kiln_manager)
        .expect("bind the resolver");

    let lua = loader.plugin_lua();
    let rows: Vec<mlua::Table> = lua
        .load(r#"return cru.kiln.blocks("notes", "a.md")"#)
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

    // A name the registry does not hold answers with the name, never a path.
    let err = lua
        .load(r#"return cru.kiln.blocks("elsewhere", "a.md")"#)
        .eval_async::<mlua::Value>()
        .await
        .expect_err("unregistered kiln");
    let message = err.to_string();
    assert!(message.contains("elsewhere"), "{message}");
    assert!(
        !message.contains(&*dir.path().to_string_lossy()),
        "{message}"
    );
}
