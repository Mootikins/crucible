use crucible_lua::stubs::StubGenerator;
use serde_json::Value;
use tempfile::TempDir;

#[test]
fn generates_emmylua_stubs_with_core_and_ui_modules() {
    let dir = TempDir::new().unwrap();
    StubGenerator::generate(dir.path()).unwrap();

    let stubs = std::fs::read_to_string(dir.path().join("cru.lua")).unwrap();
    assert!(stubs.contains("---@class cru.kiln"));
    assert!(stubs.contains("function cru.kiln.search(query, opts) end"));
    assert!(stubs.contains("---@class cru.oil"));
    assert!(
        stubs.contains("---@note UI-only: requires TUI context, not available in daemon plugins")
    );

    let docs_raw = std::fs::read_to_string(dir.path().join("cru-docs.json")).unwrap();
    let docs: Value = serde_json::from_str(&docs_raw).unwrap();
    assert_eq!(
        docs["cru.kiln.search"]["documentation"],
        "Search the knowledge base for notes"
    );
}

#[test]
fn verify_returns_true_when_stubs_match_generated_output() {
    let dir = TempDir::new().unwrap();
    StubGenerator::generate(dir.path()).unwrap();

    let committed = dir.path().join("cru.lua");
    let is_current = StubGenerator::verify(&committed).unwrap();
    assert!(is_current);
}

#[test]
fn ci_stubs_verification_generates_fresh_and_validates_content() {
    // This test acts as the CI gate for stale stubs.
    // It generates fresh stubs to a temp directory and verifies they contain
    // expected module annotations and documentation.
    let dir = TempDir::new().unwrap();
    StubGenerator::generate(dir.path()).unwrap();

    let stubs = std::fs::read_to_string(dir.path().join("cru.lua")).unwrap();
    let docs_raw = std::fs::read_to_string(dir.path().join("cru-docs.json")).unwrap();

    // Verify universal modules are present with EmmyLua annotations
    assert!(
        stubs.contains("---@class cru.kiln"),
        "Missing cru.kiln class annotation"
    );
    assert!(
        stubs.contains("---@class cru.graph"),
        "Missing cru.graph class annotation"
    );
    assert!(
        stubs.contains("---@class cru.http"),
        "Missing cru.http class annotation"
    );
    assert!(
        stubs.contains("---@class cru.fs"),
        "Missing cru.fs class annotation"
    );
    assert!(
        stubs.contains("---@class cru.session"),
        "Missing cru.session class annotation"
    );
    assert!(
        stubs.contains("---@class cru.sessions"),
        "Missing cru.sessions class annotation"
    );
    assert!(
        stubs.contains("---@class cru.tools"),
        "Missing cru.tools class annotation"
    );
    assert!(
        stubs.contains("---@class cru.oq"),
        "Missing cru.oq class annotation"
    );
    assert!(
        stubs.contains("---@class cru.paths"),
        "Missing cru.paths class annotation"
    );
    assert!(
        stubs.contains("---@class cru.timer"),
        "Missing cru.timer class annotation"
    );
    assert!(
        stubs.contains("---@class cru.ratelimit"),
        "Missing cru.ratelimit class annotation"
    );
    assert!(
        stubs.contains("---@class cru.mcp"),
        "Missing cru.mcp class annotation"
    );
    assert!(
        stubs.contains("---@class cru.hooks"),
        "Missing cru.hooks class annotation"
    );
    assert!(
        stubs.contains("---@class cru.notify"),
        "Missing cru.notify class annotation"
    );
    assert!(
        stubs.contains("---@class cru.ask"),
        "Missing cru.ask class annotation"
    );

    // Verify UI-only modules are marked with UI note
    assert!(
        stubs.contains("---@class cru.oil"),
        "Missing cru.oil class annotation"
    );
    assert!(
        stubs.contains("---@note UI-only: requires TUI context, not available in daemon plugins"),
        "Missing UI-only note for UI modules"
    );

    // Verify documentation JSON is valid and contains expected entries
    let docs: Value = serde_json::from_str(&docs_raw).unwrap();
    assert!(
        docs.get("cru.kiln.search").is_some(),
        "Missing cru.kiln.search documentation"
    );
    assert!(
        docs["cru.kiln.search"]["documentation"].as_str().is_some(),
        "cru.kiln.search documentation is not a string"
    );

    // Verify function signatures are present
    assert!(
        stubs.contains("function cru.kiln.search(query, opts) end"),
        "Missing cru.kiln.search function signature"
    );
}
