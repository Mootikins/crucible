//! cru.on_error and test mocks integration tests.

use crucible_lua::LuaExecutor;
use serde_json::json;

#[tokio::test]
async fn test_cru_on_error_initialization() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    return {
        is_nil = cru.on_error == nil,
        is_function_after_set = (function()
            cru.on_error = function(err, name, tb) return "handled" end
            return type(cru.on_error) == "function"
        end)(),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["is_nil"], true);
    assert_eq!(content["is_function_after_set"], true);
}

// ============================================================================
// TEST MOCKS INTEGRATION TESTS
// ============================================================================

#[tokio::test]
async fn test_mock_globals_exist() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    return {
        has_test_mocks = test_mocks ~= nil,
        type_test_mocks = type(test_mocks),
        has_describe = describe ~= nil,
        has_cru = cru ~= nil,
        has_cru_kiln = cru.kiln ~= nil,
    }
end
"#;
    let result = executor
        .execute_source(source, false, serde_json::json!({}))
        .await
        .unwrap();
    assert!(result.success, "Failed: {:?}", result.error);
    let content = result.content.unwrap();
    eprintln!("DEBUG: {:?}", content);
    assert_eq!(
        content["has_test_mocks"], true,
        "test_mocks should exist, type={}",
        content["type_test_mocks"]
    );
}

#[tokio::test]
async fn test_mock_kiln_returns_configured_fixtures() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    test_mocks.setup({
        kiln = {
            notes = {
                { title = "Alpha", path = "alpha.md", content = "First note about rust" },
                { title = "Beta", path = "beta.md", content = "Second note about lua" },
                { title = "Gamma", path = "gamma.md", content = "Third note about testing" },
            },
        },
    })

    local all_notes = cru.kiln.list()
    local limited = cru.kiln.list(2)
    local found = cru.kiln.get("beta.md")
    local missing = cru.kiln.get("nonexistent.md")
    local search_results = cru.kiln.search("lua", { limit = 10 })

    local list_calls = test_mocks.get_calls("kiln", "list")
    local get_calls = test_mocks.get_calls("kiln", "get")
    local search_calls = test_mocks.get_calls("kiln", "search")

    return {
        all_count = #all_notes,
        limited_count = #limited,
        found_title = found and found.title or nil,
        missing_is_nil = missing == nil,
        search_count = #search_results,
        search_path = search_results[1] and search_results[1].path or nil,
        list_call_count = #list_calls,
        get_call_count = #get_calls,
        search_call_count = #search_calls,
        search_first_arg = search_calls[1] and search_calls[1][1] or nil,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();
    assert!(result.success, "Failed: {:?}", result.error);
    let content = result.content.unwrap();

    assert_eq!(content["all_count"], 3);
    assert_eq!(content["limited_count"], 2);
    assert_eq!(content["found_title"], "Beta");
    assert_eq!(content["missing_is_nil"], true);
    assert_eq!(content["search_count"], 1);
    assert_eq!(content["search_path"], "beta.md");
    assert_eq!(content["list_call_count"], 2);
    assert_eq!(content["get_call_count"], 2);
    assert_eq!(content["search_call_count"], 1);
    assert_eq!(content["search_first_arg"], "lua");
}

#[tokio::test]
async fn test_mock_http_records_requests_and_returns_responses() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    test_mocks.setup({
        http = {
            responses = {
                ["https://api.example.com/data"] = {
                    status = 200,
                    body = '{"result": "success"}',
                    ok = true,
                },
                ["https://api.example.com/error"] = {
                    status = 500,
                    body = "Internal Server Error",
                    ok = false,
                },
            },
        },
    })

    local ok_resp = cru.http.get("https://api.example.com/data")
    local err_resp = cru.http.post("https://api.example.com/error", {
        body = '{"key": "value"}',
    })
    local default_resp = cru.http.get("https://unknown.com")

    local get_calls = test_mocks.get_calls("http", "get")
    local post_calls = test_mocks.get_calls("http", "post")

    return {
        ok_status = ok_resp.status,
        ok_body = ok_resp.body,
        ok_ok = ok_resp.ok,
        err_status = err_resp.status,
        err_ok = err_resp.ok,
        default_status = default_resp.status,
        default_ok = default_resp.ok,
        get_call_count = #get_calls,
        post_call_count = #post_calls,
        first_get_url = get_calls[1] and get_calls[1][1] or nil,
        first_post_url = post_calls[1] and post_calls[1][1] or nil,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();
    assert!(result.success, "Failed: {:?}", result.error);
    let content = result.content.unwrap();

    assert_eq!(content["ok_status"], 200);
    assert_eq!(content["ok_body"], r#"{"result": "success"}"#);
    assert_eq!(content["ok_ok"], true);
    assert_eq!(content["err_status"], 500);
    assert_eq!(content["err_ok"], false);
    assert_eq!(content["default_status"], 200);
    assert_eq!(content["default_ok"], true);
    assert_eq!(content["get_call_count"], 2);
    assert_eq!(content["post_call_count"], 1);
    assert_eq!(content["first_get_url"], "https://api.example.com/data");
    assert_eq!(content["first_post_url"], "https://api.example.com/error");
}

#[tokio::test]
async fn test_mock_reset_clears_call_history_and_fixtures() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    test_mocks.setup({
        kiln = {
            notes = {
                { title = "Note", path = "note.md", content = "content" },
            },
        },
    })

    cru.kiln.list()
    cru.kiln.get("note.md")
    cru.http.get("https://example.com")

    local pre_reset_kiln_list = #test_mocks.get_calls("kiln", "list")
    local pre_reset_kiln_get = #test_mocks.get_calls("kiln", "get")
    local pre_reset_http_get = #test_mocks.get_calls("http", "get")
    local pre_reset_notes = #cru.kiln.list()

    test_mocks.reset()

    local post_reset_kiln_list = #test_mocks.get_calls("kiln", "list")
    local post_reset_http_get = #test_mocks.get_calls("http", "get")

    local post_notes = cru.kiln.list()
    local post_list_calls = #test_mocks.get_calls("kiln", "list")

    return {
        pre_kiln_list = pre_reset_kiln_list,
        pre_kiln_get = pre_reset_kiln_get,
        pre_http_get = pre_reset_http_get,
        pre_note_count = pre_reset_notes,
        post_kiln_list = post_reset_kiln_list,
        post_http_get = post_reset_http_get,
        post_note_count = #post_notes,
        post_list_calls = post_list_calls,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();
    assert!(result.success, "Failed: {:?}", result.error);
    let content = result.content.unwrap();

    assert_eq!(content["pre_kiln_list"], 1);
    assert_eq!(content["pre_kiln_get"], 1);
    assert_eq!(content["pre_http_get"], 1);
    assert_eq!(content["pre_note_count"], 1);
    assert_eq!(
        content["post_kiln_list"], 0,
        "reset should clear kiln list calls"
    );
    assert_eq!(
        content["post_http_get"], 0,
        "reset should clear http get calls"
    );
    assert_eq!(
        content["post_note_count"], 0,
        "reset should return empty default fixtures"
    );
    assert_eq!(
        content["post_list_calls"], 1,
        "new call after reset should be recorded"
    );
}
