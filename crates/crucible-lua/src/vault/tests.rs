//! Tests for the `cru.kiln` Lua module — split from `mod.rs` for the
//! file-size gate. Three groups: `stub_tests` (the storage-free registration
//! `DaemonPluginLoader::new` performs), `store_tests` (`list`/`get` over a
//! `NoteStore`), and `graph_tests` (`outlinks`/`backlinks`/`neighbors`).

mod stub_tests {
    use crate::test_support::TestLuaBuilder;
    use crate::vault::*;

    #[test]
    fn test_register_kiln_module() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let kiln: Table = cru.get("kiln").expect("cru.kiln should exist");

        assert!(kiln.contains_key("list").unwrap());
        assert!(kiln.contains_key("get").unwrap());
        assert!(kiln.contains_key("search").unwrap());
        assert!(kiln.contains_key("outlinks").unwrap());
        assert!(kiln.contains_key("backlinks").unwrap());
        assert!(kiln.contains_key("neighbors").unwrap());
    }

    #[test]
    fn test_kiln_also_registered_as_crucible() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let crucible: Table = lua
            .globals()
            .get("crucible")
            .expect("crucible should exist");
        let kiln: Table = crucible.get("kiln").expect("crucible.kiln should exist");

        assert!(kiln.contains_key("list").unwrap());
    }

    #[tokio::test]
    async fn test_kiln_list_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_kiln_get_stub_returns_nil() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Value = lua
            .load(r#"return cru.kiln.get("test.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[tokio::test]
    async fn test_kiln_outlinks_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.outlinks("test.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_kiln_backlinks_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.backlinks("test.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_kiln_neighbors_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.neighbors("test.md", 2)"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }
}

mod store_tests {
    use crate::test_support::TestLuaBuilder;
    use crate::vault::*;
    use async_trait::async_trait;
    use crucible_core::events::{InternalSessionEvent, SessionEvent};
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{
        Filter, GraphLink, NoteRecord, NoteStore, SearchResult, StorageResult,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock NoteStore for testing.
    ///
    /// Shared with `graph_tests`: the three graph functions are registered by
    /// `register_vault_module_with_store_scoped`, so `NoteStore` is the seam
    /// production actually wires and mocking it exercises the real
    /// registration. What this mock cannot stand in for is link *resolution*
    /// — see `backlinks` below — which the `crucible-daemon` wiring tests
    /// cover against a real `SqliteNoteStore`.
    pub(super) struct MockNoteStore {
        notes: Mutex<HashMap<String, NoteRecord>>,
    }

    impl MockNoteStore {
        pub(super) fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        pub(super) fn with_notes(notes: Vec<NoteRecord>) -> Self {
            let store = Self::new();
            {
                let mut map = store.notes.lock().unwrap();
                for note in notes {
                    map.insert(note.path.clone(), note);
                }
            }
            store
        }
    }

    /// The visibility rule `SqliteNoteStore` enforces in SQL: a note is
    /// readable iff its stamped scope is the authority's workspace, or it has
    /// no scope at all (legacy notes belong to the kiln they live in).
    fn readable(note: &NoteRecord, authority: &crucible_core::storage::Scope) -> bool {
        note.scope()
            .is_none_or(|scope| authority.same_workspace(&scope))
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
            let title = note.title.clone();
            let path = note.path.clone();
            let mut map = self.notes.lock().unwrap();
            map.insert(note.path.clone(), note);
            let event = SessionEvent::internal(InternalSessionEvent::NoteCreated {
                path: path.into(),
                title: Some(title),
            });
            Ok(vec![event])
        }

        async fn get(
            &self,
            path: &str,
            authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
            let map = self.notes.lock().unwrap();
            Ok(map.get(path).filter(|n| readable(n, authority)).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
            let mut map = self.notes.lock().unwrap();
            map.remove(path);
            Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
                path: path.into(),
                existed: false,
            }))
        }

        async fn list(
            &self,
            authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Vec<NoteRecord>> {
            let map = self.notes.lock().unwrap();
            Ok(map
                .values()
                .filter(|n| readable(n, authority))
                .cloned()
                .collect())
        }

        /// Sources whose `links_to` names `target_path` exactly.
        ///
        /// The daemon's index also resolves by title and by file stem
        /// (`storage/sqlite/link_index.rs`), so exact-path is a strict subset
        /// of production resolution. Deliberate: a mock that guessed at
        /// resolution would be asserting its own guess.
        async fn backlinks(&self, target_path: &str) -> StorageResult<Vec<String>> {
            let map = self.notes.lock().unwrap();
            let mut sources: Vec<String> = map
                .values()
                .filter(|n| n.links_to.iter().any(|link| link == target_path))
                .map(|n| n.path.clone())
                .collect();
            sources.sort();
            Ok(sources)
        }

        async fn graph_links(&self) -> StorageResult<Vec<GraphLink>> {
            let map = self.notes.lock().unwrap();
            let mut edges = Vec::new();
            for note in map.values() {
                for target in &note.links_to {
                    // Self-links are excluded by the real index too.
                    if *target == note.path {
                        continue;
                    }
                    edges.push(GraphLink {
                        source: note.path.clone(),
                        target: target.clone(),
                        resolved: map.contains_key(target),
                    });
                }
            }
            edges.sort_by(|a, b| (&a.source, &a.target).cmp(&(&b.source, &b.target)));
            edges.dedup();
            Ok(edges)
        }

        async fn get_by_hash(
            &self,
            _hash: &BlockHash,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _k: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<SearchResult>> {
            Ok(vec![])
        }
    }

    fn sample_note(path: &str, title: &str) -> NoteRecord {
        NoteRecord::new(path, BlockHash::zero())
            .with_title(title)
            .with_tags(vec!["rust".to_string(), "test".to_string()])
            .with_links(vec!["other/note.md".to_string()])
    }

    #[tokio::test]
    async fn test_vault_list_returns_notes() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("a.md", "Note A"),
            sample_note("b.md", "Note B"),
            sample_note("c.md", "Note C"),
        ]));
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(r#"return cru.kiln.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_vault_list_with_limit() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("a.md", "Note A"),
            sample_note("b.md", "Note B"),
            sample_note("c.md", "Note C"),
        ]));
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(r#"return cru.kiln.list(2)"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_vault_get_returns_note() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md",
            "Test Note",
        )]));
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(r#"return cru.kiln.get("test.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<String>("path").unwrap(), "test.md");
        assert_eq!(result.get::<String>("title").unwrap(), "Test Note");
        assert!(result.get::<bool>("has_embedding").is_ok());
    }

    #[tokio::test]
    async fn test_vault_get_returns_nil_for_missing() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Value = lua
            .load(r#"return cru.kiln.get("nonexistent.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[tokio::test]
    async fn test_vault_get_includes_tags() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md", "Test",
        )]));
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(
                r#"
                local note = cru.kiln.get("test.md")
                return note.tags
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
        assert_eq!(result.get::<String>(1).unwrap(), "rust");
        assert_eq!(result.get::<String>(2).unwrap(), "test");
    }

    #[tokio::test]
    async fn test_vault_get_includes_links() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md", "Test",
        )]));
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(
                r#"
                local note = cru.kiln.get("test.md")
                return note.links_to
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 1);
        assert_eq!(result.get::<String>(1).unwrap(), "other/note.md");
    }

    #[tokio::test]
    async fn test_vault_note_has_all_fields() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md", "Test",
        )]));
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(
                r#"
                local note = cru.kiln.get("test.md")
                return {
                    has_path = note.path ~= nil,
                    has_title = note.title ~= nil,
                    has_tags = note.tags ~= nil,
                    has_links = note.links_to ~= nil,
                    has_updated = note.updated_at ~= nil,
                    has_embedding_flag = note.has_embedding ~= nil,
                    has_properties = note.properties ~= nil,
                    has_content_hash = note.content_hash ~= nil,
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert!(result.get::<bool>("has_path").unwrap());
        assert!(result.get::<bool>("has_title").unwrap());
        assert!(result.get::<bool>("has_tags").unwrap());
        assert!(result.get::<bool>("has_links").unwrap());
        assert!(result.get::<bool>("has_updated").unwrap());
        assert!(result.get::<bool>("has_embedding_flag").unwrap());
        assert!(result.get::<bool>("has_properties").unwrap());
        assert!(result.get::<bool>("has_content_hash").unwrap());
    }
}
mod graph_tests {
    use super::store_tests::MockNoteStore;
    use crate::test_support::TestLuaBuilder;
    use crate::vault::*;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::NoteRecord;

    // Ported from the `MockGraphView` era. The mock is gone: these run
    // through `register_vault_module_with_store_scoped` over a `NoteStore`,
    // which is the registration `DaemonPluginLoader::upgrade_with_storage`
    // performs. They used to pass against a `GraphView` production never
    // constructed, which is exactly how `cru.kiln.outlinks/backlinks/
    // neighbors` shipped returning an empty table forever.

    fn note(path: &str, links: &[&str]) -> NoteRecord {
        NoteRecord::new(path, BlockHash::zero())
            .with_title(path.trim_end_matches(".md"))
            .with_links(links.iter().map(|s| (*s).to_string()).collect())
    }

    fn store(notes: Vec<NoteRecord>) -> Arc<dyn NoteStore> {
        Arc::new(MockNoteStore::with_notes(notes))
    }

    async fn eval_paths(lua: &Lua, script: &str) -> Vec<String> {
        let table: Table = lua.load(script).eval_async().await.expect("eval");
        table
            .sequence_values::<String>()
            .collect::<Result<Vec<_>, _>>()
            .expect("array of strings")
    }

    #[tokio::test]
    async fn outlinks_returns_the_notes_a_note_links_to() {
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![
                note("test.md", &["linked/a.md", "linked/b.md"]),
                note("linked/a.md", &[]),
                note("linked/b.md", &[]),
            ]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.outlinks("test.md")"#).await;

        assert_eq!(paths, ["linked/a.md", "linked/b.md"]);
    }

    #[tokio::test]
    async fn outlinks_is_empty_for_a_note_with_no_links() {
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![note("orphan.md", &[])]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.outlinks("orphan.md")"#).await;

        assert!(paths.is_empty(), "{paths:?}");
    }

    #[tokio::test]
    async fn backlinks_returns_the_notes_that_link_here() {
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![
                note("backlink/from-a.md", &["test.md"]),
                note("test.md", &[]),
            ]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.backlinks("test.md")"#).await;

        assert_eq!(paths, ["backlink/from-a.md"]);
    }

    #[tokio::test]
    async fn backlinks_is_empty_when_nothing_links_here() {
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![note("orphan.md", &[])]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.backlinks("orphan.md")"#).await;

        assert!(paths.is_empty(), "{paths:?}");
    }

    #[tokio::test]
    async fn neighbors_walks_outlinks_and_backlinks_alike() {
        // test.md links out to two notes and is linked to by a third; the
        // default depth is 1, and the walk is undirected, so all three.
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![
                note("test.md", &["linked/a.md", "linked/b.md"]),
                note("linked/a.md", &[]),
                note("linked/b.md", &[]),
                note("backlink/from-a.md", &["test.md"]),
            ]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.neighbors("test.md")"#).await;

        assert_eq!(paths, ["backlink/from-a.md", "linked/a.md", "linked/b.md"]);
    }

    #[tokio::test]
    async fn neighbors_reaches_further_hops_with_a_greater_depth() {
        // a.md -> b.md -> c.md -> d.md
        let notes = vec![
            note("a.md", &["b.md"]),
            note("b.md", &["c.md"]),
            note("c.md", &["d.md"]),
            note("d.md", &[]),
        ];

        let lua = TestLuaBuilder::new()
            .with_vault_store(store(notes.clone()))
            .build();
        let one_hop = eval_paths(&lua, r#"return cru.kiln.neighbors("a.md", 1)"#).await;
        assert_eq!(one_hop, ["b.md"]);

        let lua = TestLuaBuilder::new().with_vault_store(store(notes)).build();
        let three_hops = eval_paths(&lua, r#"return cru.kiln.neighbors("a.md", 3)"#).await;
        assert_eq!(three_hops, ["b.md", "c.md", "d.md"]);
    }

    #[tokio::test]
    async fn neighbors_is_empty_for_an_isolated_note() {
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![
                note("isolated.md", &[]),
                note("a.md", &["b.md"]),
                note("b.md", &[]),
            ]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.neighbors("isolated.md", 2)"#).await;

        assert!(paths.is_empty(), "{paths:?}");
    }

    // The three `GraphView::neighbors` invariants the port had to preserve
    // and that no ported test pinned.

    #[tokio::test]
    async fn neighbors_at_depth_zero_returns_nothing() {
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![note("a.md", &["b.md"]), note("b.md", &[])]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.neighbors("a.md", 0)"#).await;

        assert!(paths.is_empty(), "{paths:?}");
    }

    #[tokio::test]
    async fn neighbors_never_includes_the_starting_note() {
        // A cycle back to the start, plus a self-link: neither may surface.
        let lua = TestLuaBuilder::new()
            .with_vault_store(store(vec![
                note("a.md", &["b.md", "a.md"]),
                note("b.md", &["c.md"]),
                note("c.md", &["a.md"]),
            ]))
            .build();

        let paths = eval_paths(&lua, r#"return cru.kiln.neighbors("a.md", 5)"#).await;

        assert_eq!(paths, ["b.md", "c.md"]);
    }

    #[tokio::test]
    async fn graph_functions_hide_notes_outside_the_authority_scope() {
        let authority = Scope::workspace_unchecked("/kiln");
        let lua = TestLuaBuilder::new()
            .with_vault_store_scoped(
                store(vec![
                    note("target.md", &[]),
                    note("neighbor.md", &["target.md"]),
                    note("secret.md", &["target.md"])
                        .with_scope(Scope::workspace_unchecked("/other")),
                ]),
                authority,
            )
            .build();

        let backlinks = eval_paths(&lua, r#"return cru.kiln.backlinks("target.md")"#).await;
        assert_eq!(backlinks, ["neighbor.md"], "secret.md is out of scope");

        let neighbors = eval_paths(&lua, r#"return cru.kiln.neighbors("target.md", 3)"#).await;
        assert_eq!(neighbors, ["neighbor.md"], "scope holds across the BFS");

        let outlinks = eval_paths(&lua, r#"return cru.kiln.outlinks("secret.md")"#).await;
        assert!(
            outlinks.is_empty(),
            "an out-of-scope source reads as a missing one: {outlinks:?}"
        );
    }
}
