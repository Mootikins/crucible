//! The named kiln reads and `cru.embed` reach the SQLite store the pipeline
//! wrote, and the embedder the kiln indexed with.
//!
//! The crate that registers the bindings cannot build a block store, so the
//! tests that prove Lua reaches stored vectors live here, over the
//! production loader, a registered kiln and the fixture embedder.

use std::sync::Arc;

use crucible_core::config::{EmbeddingProviderConfig, MockConfig};
use crucible_daemon::daemon_plugins::DaemonPluginLoader;
use crucible_daemon::kiln_manager::KilnManager;
use crucible_daemon::kiln_registry::KilnRegistry;
use crucible_daemon::test_support::kiln_registry;
use tokio::sync::broadcast;

const NOTE: &str = "# Title\n\nAlpha has enough words here for embedding.\n\n## Section\n\nBeta has enough words here for embedding too.\n";

/// A note that links to `a.md`, so the graph has one resolved edge.
const LINKING_NOTE: &str = "# Other\n\nSee [[a]] for the alpha and beta words.\n";

/// The registered kiln `notes`, processed with the 8-dimension mock embedder.
struct ProcessedKiln {
    _dir: tempfile::TempDir,
    registry: Arc<KilnRegistry>,
    kiln_manager: Arc<KilnManager>,
    kiln_dir: std::path::PathBuf,
}

async fn processed_kiln() -> ProcessedKiln {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let kiln_dir = dir.path().join("notes");
    std::fs::create_dir_all(kiln_dir.join(".crucible")).expect("kiln dir");
    std::fs::write(kiln_dir.join("a.md"), NOTE).expect("write note");
    std::fs::write(kiln_dir.join("other.md"), LINKING_NOTE).expect("write note");

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
    for name in ["a.md", "other.md"] {
        kiln_manager
            .process_file(&kiln_dir, &kiln_dir.join(name))
            .await
            .expect("process");
    }
    ProcessedKiln {
        _dir: dir,
        registry,
        kiln_manager,
        kiln_dir,
    }
}

fn loader(kiln: &ProcessedKiln) -> DaemonPluginLoader {
    DaemonPluginLoader::new(Default::default())
        .expect("plugin loader")
        .with_kiln_repository_resolver(Arc::clone(&kiln.registry), Arc::clone(&kiln.kiln_manager))
        .expect("bind the repository resolver")
        .with_embed_resolver(Arc::clone(&kiln.registry), Arc::clone(&kiln.kiln_manager))
        .expect("bind the embed resolver")
}

#[tokio::test]
async fn cru_kiln_blocks_reads_the_stored_vectors_in_span_order() {
    let kiln = processed_kiln().await;
    let handle = kiln.kiln_manager.get(&kiln.kiln_dir).await.expect("open");
    let stored = handle
        .as_block_store()
        .blocks_for_note("a.md")
        .await
        .expect("stored");
    assert_eq!(stored.len(), 4, "the fixture note has four blocks");

    let loader = loader(&kiln);
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
        !message.contains(&*kiln.kiln_dir.to_string_lossy()),
        "{message}"
    );
}

/// `cru.embed` answers with the kiln's embedder, and `cru.kiln.search` ranks
/// the stored blocks by that vector.
#[tokio::test]
async fn cru_embed_and_search_use_the_kiln_embedder() {
    let kiln = processed_kiln().await;
    let loader = loader(&kiln);
    let lua = loader.plugin_lua();

    let vector: Vec<f32> = lua
        .load(r#"return cru.embed("notes", "alpha words")"#)
        .eval_async()
        .await
        .expect("cru.embed");
    assert_eq!(vector.len(), 8, "the mock embedder's dimension");

    let rows: Vec<mlua::Table> = lua
        .load(r#"return cru.kiln.search("notes", cru.embed("notes", "alpha words"), 2)"#)
        .eval_async()
        .await
        .expect("cru.kiln.search");
    assert_eq!(rows.len(), 2);
    for row in &rows {
        let path: String = row.get("path").unwrap();
        let kind: String = row.get("kind").unwrap();
        let score: f64 = row.get("score").unwrap();
        assert!(path.ends_with(".md"), "{path}");
        assert_eq!(kind, "paragraph", "only prose carries a vector");
        assert!(score.is_finite());
    }

    let err = lua
        .load(r#"return cru.embed("elsewhere", "alpha words")"#)
        .eval_async::<mlua::Value>()
        .await
        .expect_err("unregistered kiln");
    assert!(err.to_string().contains("elsewhere"), "{err}");
}

/// The named graph reads answer the index rows and the resolved edge.
#[tokio::test]
async fn cru_kiln_note_notes_and_links_read_the_named_kiln() {
    let kiln = processed_kiln().await;
    let loader = loader(&kiln);
    let lua = loader.plugin_lua();

    let title: String = lua
        .load(r#"return cru.kiln.note("notes", "a.md").title"#)
        .eval_async()
        .await
        .expect("cru.kiln.note");
    assert_eq!(title, "a", "the pipeline titles a note from its stem");

    let paths: Vec<String> = lua
        .load(
            r#"local t = {}
               for _, n in ipairs(cru.kiln.notes("notes")) do t[#t + 1] = n.path end
               table.sort(t)
               return t"#,
        )
        .eval_async()
        .await
        .expect("cru.kiln.notes");
    assert_eq!(paths, vec!["a.md", "other.md"]);

    let links: mlua::Table = lua
        .load(r#"return cru.kiln.links("notes", "a.md")"#)
        .eval_async()
        .await
        .expect("cru.kiln.links");
    let outlinks: Vec<String> = links.get("outlinks").unwrap();
    let backlinks: Vec<String> = links.get("backlinks").unwrap();
    assert!(outlinks.is_empty(), "{outlinks:?}");
    assert_eq!(backlinks, vec!["other.md"]);

    let links: mlua::Table = lua
        .load(r#"return cru.kiln.links("notes", "other.md")"#)
        .eval_async()
        .await
        .expect("cru.kiln.links");
    let outlinks: Vec<String> = links.get("outlinks").unwrap();
    assert_eq!(outlinks, vec!["a.md"]);
}

/// `process_batch` holds the kiln's connection while `index:blocks` fires.
/// A handler that calls `cru.embed` there must get the provider without that
/// connection, or it waits on itself until the handler budget stops it.
#[tokio::test]
async fn cru_embed_answers_inside_an_index_blocks_handler() {
    let kiln = processed_kiln().await;
    let loader = loader(&kiln);
    let lua = loader.plugin_lua();
    lua.load(
        r#"cru.on("index:blocks", {}, function(ctx, event)
               local replace = {}
               for _, block in ipairs(event.blocks) do
                   if block.vector ~= nil then
                       replace[#replace + 1] = {
                           span_start = block.span_start,
                           vector = cru.embed(event.kiln, "again: " .. block.text),
                       }
                   end
               end
               EMBEDDED_IN_HANDLER = #replace
               INDEX_TITLE = event.title
               return { replace = replace }
           end)"#,
    )
    .exec_async()
    .await
    .expect("register the handler");
    kiln.kiln_manager
        .set_plugin_handlers(loader.plugin_handlers(), loader.plugin_lua());

    kiln.kiln_manager
        .process_batch(&kiln.kiln_dir, &[kiln.kiln_dir.join("a.md")], true)
        .await
        .expect("process the batch");
    let embedded: Option<i64> = lua
        .load("return EMBEDDED_IN_HANDLER")
        .eval_async()
        .await
        .expect("read the count");
    assert_eq!(
        embedded,
        Some(2),
        "both prose blocks were re-embedded in the handler"
    );
    let title: String = lua
        .load("return INDEX_TITLE")
        .eval_async()
        .await
        .expect("read the title");
    assert_eq!(title, "a", "the event carries the note's title");
}
