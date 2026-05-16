//! Adversarial tests for memory-scoping enforcement at the kiln RPC boundary.
//!
//! Restores the regression tests that shipped with the original
//! `DaemonVaultBridge` write-validation (commit c1d2e1424) and were
//! deleted in the "cascade-orphaned APIs" cleanup. The validation now
//! lives directly in `handle_note_upsert` (no bridge), so the tests
//! exercise the RPC handler rather than a removed trait.

use crate::kiln_manager::KilnManager;
use crate::server::kiln;
use crucible_core::parser::BlockHash;
use crucible_core::protocol::{Request, INVALID_PARAMS};
use crucible_core::storage::Scope;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Set up a `KilnManager` with one open kiln. Returns the manager, a tempdir
/// guard (drop = cleanup), and the canonical path to the kiln.
async fn open_kiln() -> (Arc<KilnManager>, TempDir, PathBuf) {
    let km = Arc::new(KilnManager::new());
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();
    km.open(&kiln_path).await.unwrap();
    let canon = kiln_path.canonicalize().unwrap();
    (km, tmp, canon)
}

fn note_record_json(path: &str, scope: Option<Value>) -> Value {
    let mut props = serde_json::Map::new();
    if let Some(s) = scope {
        props.insert("scope".to_string(), s);
    }
    json!({
        "path": path,
        "content_hash": BlockHash::zero(),
        "title": "Test note",
        "tags": [],
        "links_to": [],
        "properties": Value::Object(props),
        "updated_at": chrono::Utc::now().to_rfc3339(),
    })
}

fn upsert_request(kiln_path: &std::path::Path, note: Value) -> Request {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "note.upsert",
        "params": {
            "kiln": kiln_path.to_string_lossy(),
            "note": note,
        }
    }))
    .unwrap()
}

fn list_request(kiln_path: &std::path::Path, scope_override: Option<Value>) -> Request {
    let mut params = serde_json::Map::new();
    params.insert(
        "kiln".to_string(),
        Value::String(kiln_path.to_string_lossy().to_string()),
    );
    if let Some(s) = scope_override {
        params.insert("scope".to_string(), s);
    }
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "note.list",
        "params": Value::Object(params),
    }))
    .unwrap()
}

// ─────────────────────────────────────────────────────────────────────────
// C1 — write-side scope validation in handle_note_upsert
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_with_scope_exceeding_session_authority_fails_sibling_workspace() {
    let (km, _tmp, kiln_path) = open_kiln().await;
    // Sibling workspace path that exists on disk so canonicalization succeeds.
    let other_tmp = TempDir::new().unwrap();
    let sibling = other_tmp.path().canonicalize().unwrap();

    let forged = json!({"kind":"workspace","path": sibling.to_string_lossy()});
    let req = upsert_request(
        &kiln_path,
        note_record_json("notes/forged.md", Some(forged)),
    );

    let resp = kiln::handle_note_upsert(req, &km).await;

    let err = resp.error.expect("must reject sibling-workspace scope");
    assert_eq!(err.code, INVALID_PARAMS);
    assert!(
        err.message.contains("exceeds session write authority"),
        "error message should explain the rejection: {}",
        err.message
    );
}

#[tokio::test]
async fn upsert_with_scope_equal_to_session_succeeds() {
    let (km, _tmp, kiln_path) = open_kiln().await;
    let same = json!({"kind":"workspace","path": kiln_path.to_string_lossy()});
    let req = upsert_request(&kiln_path, note_record_json("notes/ok.md", Some(same)));

    let resp = kiln::handle_note_upsert(req, &km).await;

    assert!(
        resp.error.is_none(),
        "matching workspace scope should be accepted: {:?}",
        resp.error
    );
}

#[tokio::test]
async fn upsert_without_scope_gets_workspace_default() {
    // No `scope` property on the note. The handler should stamp the kiln's
    // workspace scope onto the record so legacy-style unscoped writes don't
    // become cross-workspace visible after upsert.
    let (km, _tmp, kiln_path) = open_kiln().await;
    let req = upsert_request(&kiln_path, note_record_json("notes/legacy.md", None));

    let resp = kiln::handle_note_upsert(req, &km).await;

    assert!(resp.error.is_none(), "unscoped upsert should succeed");

    // Read it back: list with the kiln authority should see the note.
    let list = kiln::handle_note_list(list_request(&kiln_path, None), &km).await;
    let notes = list.result.expect("list should return result");
    let arr = notes.as_array().expect("list result is an array");
    assert!(
        arr.iter().any(|n| n["path"] == "notes/legacy.md"),
        "stamped note should be visible under the kiln's authority: {:?}",
        notes
    );
}

#[tokio::test]
async fn upsert_with_unbound_frontmatter_scope_binds_to_kiln() {
    // Frontmatter form `scope: workspace` (no path) → bound to the kiln.
    let (km, _tmp, kiln_path) = open_kiln().await;
    let req = upsert_request(
        &kiln_path,
        note_record_json("notes/unbound.md", Some(Value::String("workspace".into()))),
    );

    let resp = kiln::handle_note_upsert(req, &km).await;

    assert!(
        resp.error.is_none(),
        "unbound frontmatter scope must bind to kiln, not fail: {:?}",
        resp.error
    );
}

#[tokio::test]
async fn upsert_with_unsupported_scope_kind_rejected() {
    // The Wave 2 prune dropped `global` / `user:*` — RPC writes that include
    // those kinds in properties must fail loudly.
    let (km, _tmp, kiln_path) = open_kiln().await;
    let req = upsert_request(
        &kiln_path,
        note_record_json("notes/bad.md", Some(json!({"kind":"global"}))),
    );

    let resp = kiln::handle_note_upsert(req, &km).await;

    let err = resp.error.expect("unsupported scope kind must be rejected");
    assert_eq!(err.code, INVALID_PARAMS);
    assert!(
        err.message.contains("unsupported scope"),
        "error should call out unsupported scope: {}",
        err.message
    );
}

#[tokio::test]
async fn crafted_plugin_writing_other_workspace_scope_in_session_rejected() {
    // Adversarial: a Lua plugin running in kiln A sets properties.scope to
    // workspace pointing at kiln B. With the bridge-authority check in
    // place, the daemon refuses the write before it touches disk.
    let (km, _tmp, kiln_path_a) = open_kiln().await;
    let other_tmp = TempDir::new().unwrap();
    let kiln_path_b = other_tmp.path().canonicalize().unwrap();

    let forged = json!({"kind":"workspace","path": kiln_path_b.to_string_lossy()});
    let req = upsert_request(
        &kiln_path_a,
        note_record_json("notes/exfil.md", Some(forged)),
    );

    let resp = kiln::handle_note_upsert(req, &km).await;
    assert!(
        resp.error.is_some(),
        "crafted cross-workspace upsert must be refused"
    );

    // And it must not have landed in either store. Open B and check the list.
    km.open(&kiln_path_b).await.unwrap();
    let list = kiln::handle_note_list(list_request(&kiln_path_b, None), &km).await;
    let arr = list
        .result
        .as_ref()
        .and_then(|v| v.as_array())
        .expect("list returns array");
    assert!(
        arr.iter().all(|n| n["path"] != "notes/exfil.md"),
        "refused write must not appear in target kiln: {:?}",
        list.result
    );
}

// ─────────────────────────────────────────────────────────────────────────
// C2 — read-side authority is derived from kiln_path, not client-supplied
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_ignores_client_supplied_scope() {
    // Pre-fix: a request to kiln A with scope=workspace(/kiln/B) would have
    // the SQL filter enforce B's authority. Post-fix: the `scope` field in
    // request params is ignored, and the authority always derives from
    // `kiln`. Verify by passing a sibling-workspace scope and asserting the
    // response is exactly what listing kiln A returns without the override.
    let (km, _tmp, kiln_path_a) = open_kiln().await;
    let other_tmp = TempDir::new().unwrap();
    let kiln_path_b = other_tmp.path().canonicalize().unwrap();
    km.open(&kiln_path_b).await.unwrap();

    // Insert one note into kiln A so list returns something non-empty.
    let req = upsert_request(&kiln_path_a, note_record_json("notes/a.md", None));
    let resp = kiln::handle_note_upsert(req, &km).await;
    assert!(resp.error.is_none(), "setup write must succeed");

    let baseline = kiln::handle_note_list(list_request(&kiln_path_a, None), &km).await;

    let forged = json!({"kind":"workspace","path": kiln_path_b.to_string_lossy()});
    let with_forged = kiln::handle_note_list(list_request(&kiln_path_a, Some(forged)), &km).await;

    // Same response either way — the forged scope is ignored.
    assert_eq!(
        baseline.result, with_forged.result,
        "client-supplied scope must not change read results"
    );
}

#[tokio::test]
async fn read_authority_always_derives_from_kiln_path() {
    // Tighter assertion of the same invariant: a request to kiln A with a
    // sibling-workspace scope override must NOT return notes from kiln B.
    let (km, _tmp, kiln_path_a) = open_kiln().await;
    let other_tmp = TempDir::new().unwrap();
    let kiln_path_b = other_tmp.path().canonicalize().unwrap();
    km.open(&kiln_path_b).await.unwrap();

    // Write into kiln B directly so it has a distinct note.
    let req = upsert_request(&kiln_path_b, note_record_json("notes/b-private.md", None));
    let resp = kiln::handle_note_upsert(req, &km).await;
    assert!(resp.error.is_none(), "setup write into kiln B must succeed");

    // Read from kiln A while supplying a forged scope pointing at kiln B.
    let forged = json!({"kind":"workspace","path": kiln_path_b.to_string_lossy()});
    let list = kiln::handle_note_list(list_request(&kiln_path_a, Some(forged)), &km).await;

    let arr = list
        .result
        .as_ref()
        .and_then(|v| v.as_array())
        .expect("list returns array");
    assert!(
        arr.iter().all(|n| n["path"] != "notes/b-private.md"),
        "kiln A authority must not reveal kiln B notes even with forged scope: {:?}",
        list.result
    );
}

#[tokio::test]
async fn request_scope_helper_is_kiln_workspace() {
    // Direct unit check on the helper: scope always derives from path.
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().canonicalize().unwrap();
    // request_scope is private; we exercise it indirectly via behavior tests
    // above. This test asserts the documented Scope construction works for
    // a real path.
    let scope = Scope::workspace(&path).unwrap();
    assert_eq!(scope.path(), path);
}
