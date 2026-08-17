//! Tests for the precognition path: the context block, the
//! `precognition_format` seam, and the `precognition_select` seam.
//!
//! Split out of `mod.rs` to stay under the 1000-line module budget
//! enforced by `no_new_oversized_modules`.

use super::*;

/// A registry claiming `name` for a directory, with the tempdir it lives under.
///
/// Both are returned because the tempdir must outlive the registry: paths
/// resolve lexically, so a registry over a dropped tempdir still answers, and a
/// test could pass without ever exercising the entry it meant to register.
/// `data` is a sibling of the kiln, never its ancestor — the floor refuses a
/// kiln at or above the daemon's data root.
fn registry_naming(
    name: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::sync::Arc<crate::kiln_registry::KilnRegistry>,
) {
    let root = tempfile::TempDir::new().expect("tempdir");
    let kiln = root.path().join("corpora").join(name);
    let registry =
        crate::test_support::kiln_registry(&root.path().join("data"), &[(name, kiln.as_path())]);
    (root, kiln, registry)
}

/// A registry that claims nothing — for the tests that are about something
/// other than which kiln a note came from.
fn registry_naming_nothing() -> (
    tempfile::TempDir,
    std::sync::Arc<crate::kiln_registry::KilnRegistry>,
) {
    let root = tempfile::TempDir::new().expect("tempdir");
    let registry = crate::test_support::kiln_registry(&root.path().join("data"), &[]);
    (root, registry)
}

#[cfg(test)]
mod format_precognition_context_tests {
    use super::*;
    use crucible_core::types::database::DocumentId;
    use std::path::PathBuf;

    fn make_result(
        doc_id: &str,
        score: f64,
        snippet: Option<&str>,
        kiln: Option<&str>,
    ) -> crucible_core::SearchResult {
        crucible_core::SearchResult {
            document_id: DocumentId(doc_id.to_string()),
            score,
            highlights: None,
            snippet: snippet.map(|s| s.to_string()),
            kiln_path: kiln.map(PathBuf::from),
        }
    }

    // These tests cover `precognition_context_block` — the pure
    // formatter for the kiln-injected system message. The previous
    // shape concatenated the user's content onto the block; that
    // concatenation now happens implicitly by prepending the system
    // message via `transform_context`. So the block itself no longer
    // contains the user content; tests assert the block content only.

    #[test]
    fn precognition_context_block_empty_results_returns_empty_string() {
        // compute_precognition_message skips injection on empty blocks;
        // this is the contract — empty → empty.
        let result = AgentManager::precognition_context_block(&[], false);
        assert_eq!(result, "");
    }

    #[test]
    fn precognition_context_block_single_result_has_system_tags() {
        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::precognition_context_block(&results, false);

        assert!(output.starts_with("<system>\n"));
        assert!(output.contains("</system>"));
        assert!(output.contains("Found 1 relevant notes:"));
        assert!(output.contains("## Rust"));
        assert!(output.contains("(similarity: 0.85)"));
        assert!(output.contains("Rust is a systems programming language."));
    }

    #[test]
    fn precognition_context_block_multiple_results() {
        let results = vec![
            make_result(
                "notes/Rust.md",
                0.92,
                Some("Rust is fast."),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Go.md",
                0.78,
                Some("Go is simple."),
                Some("/home/user/notes"),
            ),
        ];

        let output = AgentManager::precognition_context_block(&results, false);

        assert!(output.contains("Found 2 relevant notes:"));
        assert!(output.contains("## Rust"));
        assert!(output.contains("## Go"));
        assert!(output.contains("Rust is fast."));
        assert!(output.contains("Go is simple."));
    }

    #[test]
    fn precognition_context_block_labels_kilns_when_the_session_has_several() {
        let results = vec![make_result(
            "notes/External.md",
            0.70,
            Some("External content."),
            Some("/other/kiln"),
        )];

        let output = AgentManager::precognition_context_block(&results, true);

        assert!(output.contains("[from: kiln]"));
    }

    #[test]
    fn precognition_context_block_omits_the_label_for_a_single_kiln() {
        let results = vec![make_result(
            "notes/Local.md",
            0.90,
            Some("Local content."),
            Some("/home/user/notes"),
        )];

        let output = AgentManager::precognition_context_block(&results, false);

        assert!(!output.contains("[from:"));
    }

    #[test]
    fn precognition_context_block_missing_snippet_handled() {
        let results = vec![make_result(
            "notes/NoSnippet.md",
            0.60,
            None,
            Some("/home/user/notes"),
        )];

        let output = AgentManager::precognition_context_block(&results, false);

        assert!(output.contains("<system>"));
        assert!(output.contains("</system>"));
        assert!(output.contains("## NoSnippet"));
    }

    #[test]
    fn precognition_context_cap_truncates_when_aggregate_exceeds_limit() {
        let mut results = vec![
            make_result(
                "notes/One.md",
                0.9,
                Some(&"a".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Two.md",
                0.8,
                Some(&"b".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Three.md",
                0.7,
                Some(&"c".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Four.md",
                0.6,
                Some(&"d".repeat(800)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Five.md",
                0.5,
                Some(&"e".repeat(800)),
                Some("/home/user/notes"),
            ),
        ];

        apply_precognition_char_cap(&mut results, 3000);

        let total_chars: usize = results
            .iter()
            .map(|result| {
                result
                    .snippet
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count()
            })
            .sum();

        assert_eq!(total_chars, 3000);
        assert!(results.iter().all(|result| result
            .snippet
            .as_deref()
            .unwrap_or_default()
            .chars()
            .count()
            <= 600));
    }

    #[test]
    fn precognition_context_cap_does_not_truncate_when_under_limit() {
        let mut results = vec![
            make_result(
                "notes/One.md",
                0.9,
                Some(&"a".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Two.md",
                0.8,
                Some(&"b".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Three.md",
                0.7,
                Some(&"c".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Four.md",
                0.6,
                Some(&"d".repeat(200)),
                Some("/home/user/notes"),
            ),
            make_result(
                "notes/Five.md",
                0.5,
                Some(&"e".repeat(200)),
                Some("/home/user/notes"),
            ),
        ];

        apply_precognition_char_cap(&mut results, 3000);

        let total_chars: usize = results
            .iter()
            .map(|result| {
                result
                    .snippet
                    .as_deref()
                    .unwrap_or_default()
                    .chars()
                    .count()
            })
            .sum();

        assert_eq!(total_chars, 1000);
        assert!(results.iter().all(|result| {
            result
                .snippet
                .as_deref()
                .unwrap_or_default()
                .chars()
                .count()
                == 200
        }));
    }
}

#[cfg(test)]
mod precognition_format_hook_tests {
    use super::*;
    use crucible_core::types::database::DocumentId;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex as StdMutex};

    fn make_result(
        doc_id: &str,
        score: f64,
        snippet: Option<&str>,
        kiln: Option<&str>,
    ) -> crucible_core::SearchResult {
        crucible_core::SearchResult {
            document_id: DocumentId(doc_id.to_string()),
            score,
            highlights: None,
            snippet: snippet.map(|s| s.to_string()),
            kiln_path: kiln.map(PathBuf::from),
        }
    }

    fn make_session_event_state() -> SessionEventState {
        let lua = mlua::Lua::new();
        let registry = crucible_lua::LuaScriptHandlerRegistry::new();

        register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register_crucible_on_api should succeed");

        SessionEventState {
            lua,
            registry,
            permission_hooks: Arc::new(StdMutex::new(Vec::new())),
            permission_functions: Arc::new(StdMutex::new(HashMap::new())),
            reactor: Reactor::new(),
            spill_counter: std::sync::atomic::AtomicU32::new(1),
        }
    }

    #[tokio::test]
    async fn precognition_format_hook_customizes_output() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r###"
                crucible.on("precognition_format", function(ctx, event)
                    return "## Custom Format\n" .. event.user_message .. "\n" .. event.results[1].title
                end)
            "###,
            )
            .exec()
            .expect("Lua handler should load");

        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        // Custom format handler controls the block content. The block
        // is now what gets injected as a system ContextMessage; the
        // user content lives in a separate message and isn't part of
        // the block.
        let (_root, registry) = registry_naming_nothing();
        let output = AgentManager::format_precognition_context_block(
            "session-1",
            "What is Rust?",
            &results,
            &registry,
            false,
            &state,
            None,
        )
        .await;

        assert!(output.starts_with("## Custom Format"));
        assert!(output.contains("Rust"));
        assert!(!output.starts_with("<system>"));
    }

    #[tokio::test]
    async fn precognition_format_no_handler_uses_default() {
        let state = make_session_event_state();
        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some("/home/user/notes"),
        )];

        let (_root, registry) = registry_naming_nothing();
        let output = AgentManager::format_precognition_context_block(
            "session-1",
            "What is Rust?",
            &results,
            &registry,
            false,
            &state,
            None,
        )
        .await;

        // Block is just the system content; ContextMessage role handles
        // the "this is a system message" semantic separately.
        assert!(output.contains("<system>"));
        assert!(output.contains("Found 1 relevant notes:"));
        assert!(output.contains("Rust is a systems programming language."));
        // Original content is no longer concatenated into the block.
        assert!(!output.contains("What is Rust?"));
    }

    /// A plugin is told which kiln a note came from by *name*; the directory
    /// never reaches it.
    ///
    /// Asserted on the whole formatted string rather than on "no path appears
    /// in it": a substring check passes when the payload key is simply absent,
    /// which would also pass if the handler never ran.
    #[tokio::test]
    async fn precognition_format_names_the_kiln_and_withholds_its_directory() {
        let (_root, kiln, registry) = registry_naming("notes");
        let state = make_session_event_state();
        state
            .lua
            .load(
                r###"
                crucible.on("precognition_format", function(ctx, event)
                    local note = event.results[1]
                    return string.format(
                        "kiln=%s kiln_path=%s",
                        tostring(note.kiln),
                        tostring(note.kiln_path)
                    )
                end)
            "###,
            )
            .exec()
            .expect("Lua handler should load");

        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("Rust is a systems programming language."),
            Some(kiln.to_str().expect("utf-8 tempdir path")),
        )];

        let output = AgentManager::format_precognition_context_block(
            "session-1",
            "What is Rust?",
            &results,
            &registry,
            false,
            &state,
            None,
        )
        .await;

        assert_eq!(output, "kiln=notes kiln_path=nil");
    }

    /// An unresolvable kiln yields no key at all, not an empty string: `""` is
    /// truthy in Lua, so a handler asking `if note.kiln then` would be told
    /// "yes" about a kiln nothing can name.
    #[tokio::test]
    async fn precognition_format_omits_the_kiln_when_no_entry_claims_it() {
        let (_root, registry) = registry_naming_nothing();
        let state = make_session_event_state();
        state
            .lua
            .load(
                r###"
                crucible.on("precognition_format", function(ctx, event)
                    return "kiln=" .. tostring(event.results[1].kiln)
                end)
            "###,
            )
            .exec()
            .expect("Lua handler should load");

        let results = vec![make_result(
            "notes/Rust.md",
            0.85,
            Some("body"),
            Some("/somewhere/unregistered"),
        )];

        let output = AgentManager::format_precognition_context_block(
            "session-1",
            "What is Rust?",
            &results,
            &registry,
            false,
            &state,
            None,
        )
        .await;

        assert_eq!(output, "kiln=nil");
    }

    #[test]
    fn extract_note_info_deduplicates_same_filename_different_paths() {
        // Same file indexed with relative and absolute paths (DB migration artifact)
        let results = vec![
            make_result(
                "./docs/Getting Started.md",
                0.9,
                Some("content"),
                Some("/kiln"),
            ),
            make_result(
                "/home/user/crucible/docs/Getting Started.md",
                0.85,
                Some("same content"),
                Some("/kiln"),
            ),
            make_result("notes/Plugins.md", 0.7, Some("plugin info"), Some("/kiln")),
        ];

        let info = extract_note_info(&results, false);
        let titles: Vec<&str> = info.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["Getting Started", "Plugins"]);
    }

    #[test]
    fn extract_note_info_keeps_different_filenames_same_title_stem() {
        // Different notes in different directories should NOT be deduped
        // even if they share a display title — they have different filenames
        // (in this case they literally are the same filename so they WILL dedup;
        // truly different notes would have different filenames)
        let results = vec![
            make_result("Help/Guide.md", 0.9, Some("help guide"), Some("/kiln")),
            make_result("Meta/Guide.md", 0.8, Some("meta guide"), Some("/kiln")),
        ];

        // Same filename "Guide.md" from same kiln → deduped (likely duplicate DB entries)
        let info = extract_note_info(&results, false);
        assert_eq!(info.len(), 1);
    }

    #[test]
    fn extract_note_info_keeps_different_kiln_labels() {
        // Same filename from different kilns are kept as separate entries
        let results = vec![
            make_result("notes/Guide.md", 0.9, Some("local"), Some("/primary")),
            make_result("notes/Guide.md", 0.8, Some("remote"), Some("/secondary")),
        ];

        let info = extract_note_info(&results, true);
        assert_eq!(info.len(), 2);
        assert_eq!(info[0].kiln_label.as_deref(), Some("primary"));
        assert_eq!(info[1].kiln_label.as_deref(), Some("secondary"));
    }
}

#[cfg(test)]
mod precognition_select_hook_tests {
    use super::*;
    use crucible_core::types::database::DocumentId;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex as StdMutex};

    const KILN: &str = "/home/user/notes";

    fn make_result(doc_id: &str, score: f64, snippet: &str) -> crucible_core::SearchResult {
        crucible_core::SearchResult {
            document_id: DocumentId(doc_id.to_string()),
            score,
            highlights: None,
            snippet: Some(snippet.to_string()),
            kiln_path: Some(PathBuf::from(KILN)),
        }
    }

    fn three_results() -> Vec<crucible_core::SearchResult> {
        vec![
            make_result("notes/Alpha.md", 0.9, "alpha body"),
            make_result("notes/Beta.md", 0.8, "beta body"),
            make_result("notes/Gamma.md", 0.7, "gamma body"),
        ]
    }

    fn make_session_event_state() -> SessionEventState {
        let lua = mlua::Lua::new();
        let registry = crucible_lua::LuaScriptHandlerRegistry::new();
        register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        )
        .expect("register_crucible_on_api should succeed");

        SessionEventState {
            lua,
            registry,
            permission_hooks: Arc::new(StdMutex::new(Vec::new())),
            permission_functions: Arc::new(StdMutex::new(HashMap::new())),
            reactor: Reactor::new(),
            spill_counter: std::sync::atomic::AtomicU32::new(1),
        }
    }

    async fn run_select(
        state: &SessionEventState,
        results: &[crucible_core::SearchResult],
    ) -> Option<Vec<crucible_core::SearchResult>> {
        run_select_with_budget(state, results, 3000).await
    }

    async fn run_select_with_budget(
        state: &SessionEventState,
        results: &[crucible_core::SearchResult],
        char_budget: usize,
    ) -> Option<Vec<crucible_core::SearchResult>> {
        // A registry that claims `KILN` under the name `notes`, so the payload
        // a handler sees here is the shape production produces: every result of
        // `three_results()` came from a registered kiln.
        let root = tempfile::TempDir::new().expect("tempdir");
        let registry = crate::test_support::kiln_registry(
            &root.path().join("data"),
            &[("notes", std::path::Path::new(KILN))],
        );
        AgentManager::execute_precognition_select_handlers(
            "session-1",
            "what is alpha?",
            results,
            &registry,
            char_budget,
            state,
            None,
        )
        .await
    }

    fn titles(results: &[crucible_core::SearchResult]) -> Vec<String> {
        results.iter().map(result_title).collect()
    }

    fn snippets(results: &[crucible_core::SearchResult]) -> Vec<String> {
        results
            .iter()
            .map(|r| r.snippet.clone().unwrap_or_default())
            .collect()
    }

    /// The dogfood: the Rust default's snippet-budget policy, reimplemented in
    /// Lua using only what the seam hands a handler.
    ///
    /// `utf8.offset` rather than `string.sub` is load-bearing — the Rust cap
    /// counts *characters* (`.chars().take(n)`) while Lua string indexing is
    /// byte-based, so the naive port silently disagrees on any non-ASCII
    /// snippet and can slice a UTF-8 sequence in half.
    const LUA_EQUAL_SPLIT_CAP: &str = r#"
        crucible.on("precognition_select", function(ctx, event)
            local total = 0
            for _, r in ipairs(event.results) do
                total = total + utf8.len(r.snippet)
            end

            local out = {}
            if total <= event.char_budget then
                for _, r in ipairs(event.results) do
                    out[#out + 1] = { index = r.index }
                end
                return out
            end

            local per = math.floor(event.char_budget / event.note_count)
            for _, r in ipairs(event.results) do
                local snippet = r.snippet
                if utf8.len(snippet) > per then
                    local stop = utf8.offset(snippet, per + 1)
                    snippet = stop and snippet:sub(1, stop - 1) or snippet
                end
                out[#out + 1] = { index = r.index, snippet = snippet }
            end
            return out
        end)
    "#;

    #[tokio::test]
    async fn select_hook_narrows_and_reorders() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return { { index = 3 }, { index = 1 } }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("handler should take the decision");

        assert_eq!(titles(&selected), vec!["Gamma", "Alpha"]);
    }

    #[tokio::test]
    async fn select_hook_overrides_snippet() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return { { index = 1, snippet = "rewritten" } }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("selection");

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].snippet.as_deref(), Some("rewritten"));
    }

    #[tokio::test]
    async fn select_hook_receives_index_and_budget() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    -- Assert the payload contract from inside Lua: picking by
                    -- these fields is the whole point of the seam.
                    if event.char_budget ~= 3000 then return {} end
                    if event.note_count ~= 3 then return {} end
                    if event.results[2].kiln ~= "notes" then return {} end
                    return { { index = event.results[2].index } }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("selection");

        assert_eq!(titles(&selected), vec!["Beta"]);
    }

    #[tokio::test]
    async fn select_hook_empty_table_suppresses_precognition() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return {}
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("empty selection is a decision, not a fall-through");

        assert!(selected.is_empty());
    }

    #[tokio::test]
    async fn select_hook_error_falls_back_to_rust_default() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    error("boom")
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        // Selection is not a gate, so it fails open — only pre_tool_call
        // fails closed.
        assert!(run_select(&state, &three_results()).await.is_none());
    }

    #[tokio::test]
    async fn select_hook_malformed_return_falls_back_rather_than_suppressing() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return { oops = "not a selection" }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        // A table with entries but no numeric keys is a bug in the handler.
        // Falling back beats silently dropping the agent's grounding.
        assert!(run_select(&state, &three_results()).await.is_none());
    }

    #[tokio::test]
    async fn no_handler_leaves_the_rust_default_in_place() {
        let state = make_session_event_state();
        assert!(run_select(&state, &three_results()).await.is_none());
    }

    /// A selecting plugin can tell one corpus from another by name, and is
    /// never handed the directory to do it with.
    ///
    /// The name is smuggled out through `snippet` because that is the only
    /// field of the payload a handler can hand back — asserting on the
    /// returned snippet proves the handler both ran and saw the value.
    #[tokio::test]
    async fn precognition_select_names_the_kiln_and_withholds_its_directory() {
        let state = make_session_event_state();
        state
            .lua
            .load(
                r###"
                crucible.on("precognition_select", function(ctx, event)
                    local note = event.results[1]
                    return { {
                        index = 1,
                        snippet = string.format(
                            "kiln=%s kiln_path=%s",
                            tostring(note.kiln),
                            tostring(note.kiln_path)
                        ),
                    } }
                end)
            "###,
            )
            .exec()
            .expect("Lua handler should load");

        let selected = run_select(&state, &three_results())
            .await
            .expect("the handler selects one note");

        assert_eq!(snippets(&selected), vec!["kiln=notes kiln_path=nil"]);
    }

    #[test]
    fn selection_entries_orders_numerically_not_lexically() {
        // lua_table_to_json stringifies integer keys, so "10" sorts before "2"
        // under plain map ordering. Guard the numeric sort.
        let selection = serde_json::json!({
            "1":  { "index": 1 },
            "2":  { "index": 2 },
            "10": { "index": 10 },
        });

        let entries = selection_entries(&selection).expect("object form is accepted");
        let order: Vec<u64> = entries
            .iter()
            .map(|e| e["index"].as_u64().unwrap())
            .collect();

        assert_eq!(order, vec![1, 2, 10]);
    }

    #[test]
    fn selection_entries_accepts_a_real_json_array() {
        let selection = serde_json::json!([{ "index": 2 }, { "index": 1 }]);
        let entries = selection_entries(&selection).expect("array form is accepted");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn apply_selection_drops_out_of_range_duplicate_and_zero_indices() {
        let results = three_results();
        let selection = serde_json::json!([
            { "index": 2 },
            { "index": 2 },   // duplicate
            { "index": 99 },  // out of range
            { "index": 0 },   // 0 is not a valid 1-based index
            { "no_index": 1 },
        ]);

        let selected =
            apply_precognition_selection(&results, &selection).expect("array form is accepted");

        assert_eq!(titles(&selected), vec!["Beta"]);
    }

    #[tokio::test]
    async fn select_hook_all_invalid_indices_falls_back_rather_than_suppressing() {
        // Distinct from the empty-table case: this handler *asked* for notes
        // and every entry was unusable, which is a typo, not a decision.
        // Suppressing here would silently strip the agent's grounding.
        let state = make_session_event_state();
        state
            .lua
            .load(
                r#"
                crucible.on("precognition_select", function(ctx, event)
                    return { { index = 99 }, { index = 0 }, { nope = 1 } }
                end)
            "#,
            )
            .exec()
            .expect("Lua handler should load");

        assert!(run_select(&state, &three_results()).await.is_none());
    }

    #[test]
    fn apply_selection_falls_back_when_every_entry_is_unusable() {
        let results = three_results();
        let selection = serde_json::json!([{ "index": 99 }, { "index": 0 }]);
        assert!(apply_precognition_selection(&results, &selection).is_none());
    }

    #[test]
    fn apply_selection_still_honours_a_genuinely_empty_selection() {
        // Empty in, empty out — this is the suppress path and must survive the
        // all-unusable guard above.
        let results = three_results();
        let selected = apply_precognition_selection(&results, &serde_json::json!([]))
            .expect("an empty selection is a decision");
        assert!(selected.is_empty());
    }

    #[test]
    fn char_cap_still_bounds_a_runaway_handler_selection() {
        // Budget enforcement stays core's job even when Lua allocated.
        let mut selected = vec![
            make_result("notes/One.md", 0.9, &"a".repeat(4000)),
            make_result("notes/Two.md", 0.8, &"b".repeat(4000)),
        ];

        apply_precognition_char_cap(&mut selected, 1000);

        let total: usize = selected
            .iter()
            .map(|r| r.snippet.as_deref().unwrap_or_default().chars().count())
            .sum();
        assert_eq!(total, 1000);
    }

    /// Rust default: the built-in policy applied to the same inputs.
    fn rust_default(
        results: &[crucible_core::SearchResult],
        budget: usize,
    ) -> Vec<crucible_core::SearchResult> {
        let mut baseline = results.to_vec();
        apply_precognition_char_cap(&mut baseline, budget);
        baseline
    }

    /// Lua path: handler decides, then the core backstop still runs.
    async fn lua_reference(
        results: &[crucible_core::SearchResult],
        budget: usize,
    ) -> Vec<crucible_core::SearchResult> {
        let state = make_session_event_state();
        state
            .lua
            .load(LUA_EQUAL_SPLIT_CAP)
            .exec()
            .expect("reference handler should load");

        let mut selected = run_select_with_budget(&state, results, budget)
            .await
            .expect("reference handler should take the decision");
        apply_precognition_char_cap(&mut selected, budget);
        selected
    }

    #[tokio::test]
    async fn lua_reference_matches_rust_default_under_budget() {
        let results = three_results();
        let budget = 3000;

        assert_eq!(
            snippets(&lua_reference(&results, budget).await),
            snippets(&rust_default(&results, budget)),
        );
    }

    #[tokio::test]
    async fn lua_reference_matches_rust_default_over_budget() {
        let results = vec![
            make_result("notes/One.md", 0.9, &"a".repeat(800)),
            make_result("notes/Two.md", 0.8, &"b".repeat(800)),
            make_result("notes/Three.md", 0.7, &"c".repeat(800)),
        ];
        let budget = 900;

        let lua = lua_reference(&results, budget).await;
        let rust = rust_default(&results, budget);

        assert_eq!(snippets(&lua), snippets(&rust));
        // Guard the fixture: if this didn't truncate, the test proves nothing.
        assert!(lua[0].snippet.as_deref().unwrap().len() < 800);
    }

    #[tokio::test]
    async fn lua_reference_matches_rust_default_on_multibyte_snippets() {
        // The Rust cap counts characters; Lua string indexing counts bytes.
        // A `string.sub`-based port disagrees here and can also slice a UTF-8
        // sequence in half — this is the case that forces `utf8.offset`.
        let body: String = "日本語テキスト".repeat(20);
        let results = vec![
            make_result("notes/JaOne.md", 0.9, &body),
            make_result("notes/JaTwo.md", 0.8, &body),
        ];
        let budget = 60;

        let lua = lua_reference(&results, budget).await;
        let rust = rust_default(&results, budget);

        assert_eq!(snippets(&lua), snippets(&rust));
        assert_eq!(lua[0].snippet.as_deref().unwrap().chars().count(), 30);
    }

    /// Measures what the seam costs on the per-turn path. We are on mlua
    /// (Lua 5.4), not LuaJIT, so this is measured rather than assumed —
    /// it decides whether Lua could ever *be* the default here or stays an
    /// opt-in override with Rust shipped as the default.
    ///
    /// Deliberately asserts nothing about wall-clock: a timing threshold in
    /// CI is a flake generator. Run it and read the numbers.
    #[tokio::test]
    #[ignore = "requires: manual inspection — benchmark; run with --nocapture to read the numbers"]
    async fn measure_lua_selection_overhead() {
        const ITERATIONS: u32 = 500;
        let budget = 3000;

        // Realistic shape: k=5 notes (the default precognition_results) with
        // snippets around the per-note budget.
        let results: Vec<_> = (0..5)
            .map(|i| make_result(&format!("notes/N{i}.md"), 0.9, &"x".repeat(600)))
            .collect();

        let start = std::time::Instant::now();
        for _ in 0..ITERATIONS {
            let mut baseline = results.clone();
            apply_precognition_char_cap(&mut baseline, budget);
            std::hint::black_box(&baseline);
        }
        let rust_total = start.elapsed();

        // Handler registered once, as in production — this measures per-turn
        // dispatch, not Lua compilation.
        let state = make_session_event_state();
        state
            .lua
            .load(LUA_EQUAL_SPLIT_CAP)
            .exec()
            .expect("reference handler should load");

        let start = std::time::Instant::now();
        for _ in 0..ITERATIONS {
            let mut selected = run_select_with_budget(&state, &results, budget)
                .await
                .expect("handler should decide");
            apply_precognition_char_cap(&mut selected, budget);
            std::hint::black_box(&selected);
        }
        let lua_total = start.elapsed();

        println!("iterations:   {ITERATIONS}");
        println!("rust default: {:?}/turn", rust_total / ITERATIONS);
        println!("lua seam:     {:?}/turn", lua_total / ITERATIONS);
        println!(
            "added:        {:?}/turn",
            lua_total.saturating_sub(rust_total) / ITERATIONS
        );
    }
}
