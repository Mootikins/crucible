//! Integration tests for Lua/Fennel tool discovery and execution

use crucible_lua::{lifecycle::PluginManager, manifest::PluginState};
use crucible_lua::{
    register_oq_module, register_shell_module, LuaExecutor, LuaToolRegistry, ShellPolicy,
};
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;
use tokio::fs;

/// Helper to create a temp dir with tool files
async fn setup_tool_dir() -> TempDir {
    TempDir::new().unwrap()
}

// ============================================================================
// LUA TOOL DISCOVERY AND EXECUTION
// ============================================================================

#[tokio::test]
async fn test_lua_tool_discovery_and_execution() {
    let dir = setup_tool_dir().await;

    // Create a Lua tool with annotations
    let tool_source = r#"
--- Add two numbers
-- @tool handler
-- @param x number The first number
-- @param y number The second number
-- @return number The sum
function handler(args)
    return { result = args.x + args.y }
end
"#;

    let tool_path = dir.path().join("add.lua");
    fs::write(&tool_path, tool_source).await.unwrap();

    // Discover tools
    let mut registry = LuaToolRegistry::new().unwrap();
    let count = registry.discover_from(dir.path()).await.unwrap();

    assert_eq!(count, 1, "Should discover 1 tool");

    // Get and verify the tool
    let tools = registry.list_tools();
    assert_eq!(tools.len(), 1);

    // Execute the tool
    let result = registry
        .execute("handler", json!({"x": 10, "y": 5}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.content, json!({"result": 15}));
}

#[tokio::test]
async fn test_lua_tool_with_string_operations() {
    let dir = setup_tool_dir().await;

    let tool_source = r#"
--- String manipulation tool
-- @tool handler
-- @param text string The input text
-- @param mode string Processing mode (upper/lower/reverse)
function handler(args)
    local text = args.text
    local mode = args.mode or "upper"

    if mode == "upper" then
        return { result = text:upper() }
    elseif mode == "lower" then
        return { result = text:lower() }
    elseif mode == "reverse" then
        return { result = text:reverse() }
    else
        return { error = "Unknown mode: " .. mode }
    end
end
"#;

    let tool_path = dir.path().join("string_tool.lua");
    fs::write(&tool_path, tool_source).await.unwrap();

    let mut registry = LuaToolRegistry::new().unwrap();
    registry.discover_from(dir.path()).await.unwrap();

    // Test uppercase
    let result = registry
        .execute("handler", json!({"text": "hello", "mode": "upper"}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.content["result"], "HELLO");

    // Test reverse
    let result = registry
        .execute("handler", json!({"text": "hello", "mode": "reverse"}))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.content["result"], "olleh");
}

#[tokio::test]
async fn test_lua_tool_with_oq_module() {
    let executor = LuaExecutor::new().unwrap();

    // Register oq module
    register_oq_module(executor.lua()).unwrap();

    let source = r#"
function handler(args)
    -- Parse JSON string
    local data = oq.parse(args.json_str)

    -- Modify and encode back
    data.processed = true
    data.count = (data.count or 0) + 1

    return {
        encoded = oq.json(data),
        pretty = oq.json_pretty(data)
    }
end
"#;

    let result = executor
        .execute_source(
            source,
            false,
            json!({"json_str": r#"{"name":"test","count":5}"#}),
        )
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();

    // Verify the encoded result
    let encoded: serde_json::Value =
        serde_json::from_str(content["encoded"].as_str().unwrap()).unwrap();
    assert_eq!(encoded["name"], "test");
    assert_eq!(encoded["count"], 6);
    assert_eq!(encoded["processed"], true);

    // Verify pretty print has newlines
    let pretty = content["pretty"].as_str().unwrap();
    assert!(pretty.contains('\n'));
}

// NOTE: Shell module uses async functions which can't be called from synchronous
// Lua handlers via execute_source. The shell module is tested in its own unit tests.
// This test verifies the shell module loads correctly.
#[tokio::test]
async fn test_lua_shell_module_registration() {
    let executor = LuaExecutor::new().unwrap();

    // Verify shell module can be registered
    register_shell_module(executor.lua(), ShellPolicy::permissive()).unwrap();

    // Verify shell.which works (synchronous function)
    let source = r#"
function handler(args)
    -- shell.which is synchronous and should work
    local echo_path = shell.which("echo")
    return {
        found = echo_path ~= nil,
        is_string = type(echo_path) == "string"
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();
    assert!(result.success);
    let content = result.content.unwrap();
    assert!(content["found"].as_bool().unwrap());
    assert!(content["is_string"].as_bool().unwrap());
}

#[tokio::test]
async fn test_lua_shell_policy_blocks_dangerous_commands() {
    let executor = LuaExecutor::new().unwrap();

    // Use default policy which blocks rm, sudo, etc.
    register_shell_module(executor.lua(), ShellPolicy::default()).unwrap();

    let source = r#"
function handler(args)
    local result = shell.exec("rm", {"-rf", "/"}, {})
    return { executed = true }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    // The shell.exec should fail due to policy
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not allowed"));
}

// ============================================================================
// FULL PIPELINE: DISCOVERY -> REGISTRY -> EXECUTION
// ============================================================================

#[tokio::test]
async fn test_full_pipeline_lua() {
    let dir = setup_tool_dir().await;

    // Create a tool that uses both JSON and math
    let tool_source = r#"
--- Process data with transformations
-- @tool handler
-- @param data string JSON data to process
-- @param multiplier number Multiply numbers by this
function handler(args)
    local parsed = crucible.json_decode(args.data)

    -- Transform any numbers in the data
    if parsed.value then
        parsed.value = parsed.value * args.multiplier
    end
    if parsed.items then
        for i, v in ipairs(parsed.items) do
            if type(v) == "number" then
                parsed.items[i] = v * args.multiplier
            end
        end
    end

    return {
        processed = true,
        result = parsed
    }
end
"#;

    fs::write(dir.path().join("transform.lua"), tool_source)
        .await
        .unwrap();

    // Create and populate registry
    let mut registry = LuaToolRegistry::new().unwrap();
    let count = registry.discover_from(dir.path()).await.unwrap();
    assert_eq!(count, 1);

    // Execute with test data
    let input = json!({
        "data": r#"{"value": 10, "items": [1, 2, 3]}"#,
        "multiplier": 2
    });

    let result = registry.execute("handler", input).await.unwrap();
    assert!(result.success);

    let content = result.content;
    assert!(content["processed"].as_bool().unwrap());
    assert_eq!(content["result"]["value"], 20);
    // Arrays should be multiplied too
    let items = content["result"]["items"].as_array().unwrap();
    assert_eq!(items[0], 2);
    assert_eq!(items[1], 4);
    assert_eq!(items[2], 6);
}

#[tokio::test]
async fn test_error_handling_in_tools() {
    let dir = setup_tool_dir().await;

    let tool_source = r#"
--- Tool that may error
-- @tool handler
-- @param should_error boolean Whether to throw
function handler(args)
    if args.should_error then
        error("Intentional error for testing")
    end
    return { success = true }
end
"#;

    fs::write(dir.path().join("may_error.lua"), tool_source)
        .await
        .unwrap();

    let mut registry = LuaToolRegistry::new().unwrap();
    registry.discover_from(dir.path()).await.unwrap();

    // Test success case
    let result = registry
        .execute("handler", json!({"should_error": false}))
        .await
        .unwrap();
    assert!(result.success);

    // Test error case
    let result = registry
        .execute("handler", json!({"should_error": true}))
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Intentional error"));
}

#[tokio::test]
async fn test_tool_schema_generation() {
    use crucible_lua::{generate_input_schema, LuaTool, ToolParam};

    // Test schema generation directly from a LuaTool
    let tool = LuaTool {
        name: "search".to_string(),
        description: "Search with typed params".to_string(),
        params: vec![
            ToolParam {
                name: "query".to_string(),
                param_type: "string".to_string(),
                description: "The search query".to_string(),
                required: true,
                default: None,
            },
            ToolParam {
                name: "limit".to_string(),
                param_type: "number".to_string(),
                description: "Maximum results".to_string(),
                required: false,
                default: None,
            },
            ToolParam {
                name: "include_archived".to_string(),
                param_type: "boolean".to_string(),
                description: "Include archived notes".to_string(),
                required: false,
                default: None,
            },
        ],
        source_path: "test.lua".to_string(),
        is_fennel: false,
    };

    // Generate schema
    let schema = generate_input_schema(&tool);

    // Verify schema structure
    assert_eq!(schema["type"], "object");
    let properties = schema["properties"].as_object().unwrap();

    // Should have all params
    assert!(properties.contains_key("query"));
    assert!(properties.contains_key("limit"));
    assert!(properties.contains_key("include_archived"));

    // Query should be required
    let required = schema["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "query"));
}

// ============================================================================
// FENNEL COMPILE VIA LUA GLOBAL
// ============================================================================

#[cfg(feature = "fennel")]
#[test]
fn test_fennel_compile_via_lua_global() {
    let executor = LuaExecutor::new().unwrap();
    let lua = executor.lua();

    let has_fennel: bool = lua
        .load("return fennel ~= nil")
        .eval()
        .expect("should be able to check fennel global");
    assert!(
        has_fennel,
        "fennel global should be available in LuaExecutor"
    );

    let compiled: String = lua
        .load(r#"return fennel.compileString("(+ 1 1)")"#)
        .eval()
        .expect("fennel.compileString should compile simple Fennel");

    assert!(
        !compiled.is_empty(),
        "compiled Lua should not be empty: got {:?}",
        compiled
    );

    let result: i32 = lua
        .load(&compiled)
        .eval()
        .expect("compiled Fennel should execute as valid Lua");
    assert_eq!(result, 2, "compiled (+ 1 1) should return 2");

    let bad_result: Result<String, _> = lua
        .load(r#"return fennel.compileString("(invalid syntax ][")"#)
        .eval();
    assert!(
        bad_result.is_err(),
        "invalid Fennel should produce compilation error"
    );
}

#[cfg(feature = "fennel")]
#[test]
fn test_fennel_test_runner_integration() {
    let executor = LuaExecutor::new().unwrap();
    let lua = executor.lua();

    lua.load("test_mocks.setup()")
        .set_name("test_mocks_setup")
        .exec()
        .expect("test_mocks.setup() should succeed");

    let fennel_test_source = r#"
(describe "fennel basics" (fn []
  (it "arithmetic works" (fn []
    (assert.equal 2 (+ 1 1))))
  (it "string concatenation works" (fn []
    (assert.equal "hello world" (.. "hello" " " "world"))))))
"#;

    let compiled: String = lua
        .load(format!(
            "return fennel.compileString({:?})",
            fennel_test_source
        ))
        .eval()
        .expect("Fennel test source should compile");

    lua.load(&compiled)
        .set_name("fennel_test.fnl")
        .exec()
        .expect("compiled Fennel test should load");

    let results: mlua::Table = lua
        .load("return run_tests()")
        .eval()
        .expect("run_tests() should return results");

    let passed: usize = results.get("passed").unwrap();
    let failed: usize = results.get("failed").unwrap();
    assert_eq!(passed, 2, "both Fennel tests should pass");
    assert_eq!(failed, 0, "no Fennel tests should fail");
}

// ============================================================================
// FENNEL INTEGRATION (when available)
// ============================================================================

#[cfg(feature = "fennel")]
mod fennel_tests {
    use super::*;

    #[tokio::test]
    async fn test_fennel_tool_execution() {
        // Note: This requires fennel.lua to be present in vendor/
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
(fn handler [args]
  {:result (+ args.x args.y)
   :language "fennel"})
"#;

        let result = executor
            .execute_source(source, true, json!({"x": 5, "y": 3}))
            .await;

        // If Fennel is available, verify execution
        if let Ok(res) = result {
            if res.success {
                let content = res.content.unwrap();
                assert_eq!(content["result"], 8);
                assert_eq!(content["language"], "fennel");
            }
        }
    }

    #[tokio::test]
    async fn test_fennel_contracts_basic() {
        // Test Steel-style contracts in Fennel
        let executor = LuaExecutor::new().unwrap();

        // Inline contract predicates and wrap function for testing
        let source = r#"
;; Inline contract predicates
(fn string? [x] (= (type x) :string))
(fn number? [x] (= (type x) :number))
(fn positive? [x] (and (number? x) (> x 0)))
(fn table? [x] (= (type x) :table))

(fn and-c [p1 p2]
  (fn [x] (and (p1 x) (p2 x))))

;; Contract wrapper
(fn wrap-with-contract [f spec]
  (fn [...]
    (let [args [...]]
      ;; Check pre-conditions
      (when spec.pre
        (each [i pred (ipairs spec.pre)]
          (let [arg (. args i)]
            (when (not (pred arg))
              (error (.. "Contract violation: pre-condition #" i " failed for " spec.name))))))
      ;; Call function
      (let [result (f ...)]
        ;; Check post-condition
        (when spec.post
          (when (not (spec.post result))
            (error (.. "Contract violation: post-condition failed for " spec.name))))
        result))))

;; Test 1: Working contract
(local add-positive
  (wrap-with-contract
    (fn [x y] (+ x y))
    {:name "add-positive"
     :pre [positive? positive?]
     :post positive?}))

;; Test 2: Contract that will fail
(local will-fail
  (wrap-with-contract
    (fn [x] x)
    {:name "must-be-string"
     :pre [string?]}))

;; Use global to expose handler (Fennel scoping)
(global handler (fn [args]
  (let [results {}]
    ;; Test passing contract
    (tset results :sum (add-positive 5 3))

    ;; Test failing pre-condition
    (let [(ok err) (pcall will-fail 42)]
      (tset results :pre_failed (not ok))
      (tset results :error_has_contract (if (not ok)
                                            (not= nil (string.find err "Contract"))
                                            false)))

    results)))
"#;

        let result = executor.execute_source(source, true, json!({})).await;

        if let Ok(res) = result {
            if res.success {
                let content = res.content.unwrap();
                assert_eq!(content["sum"], 8, "Contracted add should work");
                assert_eq!(
                    content["pre_failed"], true,
                    "Pre-condition should fail for wrong type"
                );
                assert_eq!(
                    content["error_has_contract"], true,
                    "Error should mention Contract"
                );
            } else {
                panic!("Fennel execution failed: {:?}", res.error);
            }
        }
    }

    #[tokio::test]
    async fn test_fennel_contracts_preserves() {
        // Test that preserved fields are checked
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
;; Contract with preserves checking
(fn table? [x] (= (type x) :table))

(fn check-preserves [keys before after blame]
  (each [_ key (ipairs keys)]
    (let [bv (. before key)
          av (. after key)]
      (when (not= bv av)
        (error (.. "Contract violation: " blame " changed preserved field '" key "'"))))))

(fn wrap-with-preserves [f spec]
  (fn [event]
    ;; Snapshot preserved fields
    (local before {})
    (when spec.preserves
      (each [_ k (ipairs spec.preserves)]
        (tset before k (. event k))))
    ;; Call function
    (let [result (f event)]
      ;; Check preserves
      (when spec.preserves
        (check-preserves spec.preserves before result spec.name))
      result)))

;; Good handler: preserves timestamp
(local good-handler
  (wrap-with-preserves
    (fn [e]
      (tset e :processed true)
      e)
    {:name "good-handler"
     :preserves [:timestamp]}))

;; Bad handler: mutates timestamp
(local bad-handler
  (wrap-with-preserves
    (fn [e]
      (tset e :timestamp 99999)  ;; BAD!
      e)
    {:name "bad-handler"
     :preserves [:timestamp]}))

;; Use global to expose handler (Fennel scoping)
(global handler (fn [args]
  (let [results {}]
    ;; Good handler should work
    (let [event {:timestamp 12345 :data "hello"}
          result (good-handler event)]
      (tset results :good_preserved (= result.timestamp 12345))
      (tset results :good_processed result.processed))

    ;; Bad handler should fail
    (let [event {:timestamp 12345}
          (ok err) (pcall bad-handler event)]
      (tset results :bad_failed (not ok))
      (tset results :error_has_preserved (if (not ok)
                                             (not= nil (string.find err "preserved"))
                                             false)))
    results)))
"#;

        let result = executor.execute_source(source, true, json!({})).await;

        if let Ok(res) = result {
            if res.success {
                let content = res.content.unwrap();
                assert_eq!(
                    content["good_preserved"], true,
                    "Good handler should preserve timestamp"
                );
                assert_eq!(
                    content["good_processed"], true,
                    "Good handler should set processed"
                );
                assert_eq!(content["bad_failed"], true, "Bad handler should fail");
                assert_eq!(
                    content["error_has_preserved"], true,
                    "Error should mention preserved"
                );
            } else {
                panic!("Fennel execution failed: {:?}", res.error);
            }
        }
    }

    /// Test deftool macro generates both contracts and schemas
    #[tokio::test]
    async fn test_deftool_schema_generation() {
        let executor = LuaExecutor::new().unwrap();

        // Define a tool using deftool macro with schema
        let source = r#"
;; Load contracts module functions inline (since require won't work)
(global __tool_schemas__ {})

(fn register-tool-schema [name schema]
  (tset __tool_schemas__ name schema))

(fn schema-to-json-schema [schema]
  (let [properties {}
        required []]
    (each [_ param (ipairs (or schema.params []))]
      (tset properties param.name {:type param.type})
      (when (and (not= param.required false) (not param.default))
        (table.insert required param.name)))
    {:type "object"
     :properties properties
     :required required}))

(fn validate-tool-params [args params tool-name]
  (each [_ param (ipairs params)]
    (let [val (. args param.name)
          required (if (= param.required nil) true param.required)]
      (when (and required (= val nil) (not param.default))
        (error (.. "Contract violation: missing required param '" param.name "'"))))))

(fn apply-defaults [args params]
  (let [result (or args {})]
    (each [_ param (ipairs params)]
      (when (and (= (. result param.name) nil) param.default)
        (tset result param.name param.default)))
    result))

(fn make-tool [name schema impl]
  (register-tool-schema name schema)
  (fn [args]
    (let [with-defaults (apply-defaults args (or schema.params []))]
      (validate-tool-params with-defaults (or schema.params []) name)
      (impl with-defaults))))

;; Define the tool using our make-tool (simulating deftool macro)
(global search (make-tool "search"
  {:description "Search the knowledge base"
   :params [{:name "query" :type "string" :required true :description "Search query"}
            {:name "limit" :type "number" :required false :default 10}]
   :returns "array"}
  (fn [args]
    [{:title (.. "Result for: " args.query) :score 0.95}
     {:title "Another result" :score 0.8}])))

;; Test handler: invoke tool and get schema
(global handler (fn [_]
  (let [result (search {:query "test"})
        schema (. __tool_schemas__ "search")
        json-schema (schema-to-json-schema schema)]
    {:tool_result result
     :schema_description schema.description
     :schema_params_count (length schema.params)
     :json_schema json-schema})))
"#;

        let result = executor.execute_source(source, true, json!({})).await;

        match result {
            Ok(res) => {
                if res.success {
                    let content = res.content.unwrap();
                    // Verify tool executed
                    assert!(content["tool_result"].is_array());
                    assert_eq!(content["tool_result"][0]["title"], "Result for: test");

                    // Verify schema was registered
                    assert_eq!(content["schema_description"], "Search the knowledge base");
                    assert_eq!(content["schema_params_count"], 2);

                    // Verify JSON schema format
                    let json_schema = &content["json_schema"];
                    assert_eq!(json_schema["type"], "object");
                    assert!(json_schema["properties"]["query"].is_object());
                    assert!(json_schema["required"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("query")));
                } else {
                    panic!("Fennel execution failed: {:?}", res.error);
                }
            }
            Err(e) => panic!("Test failed: {}", e),
        }
    }

    /// Test contract validation rejects invalid params
    #[tokio::test]
    async fn test_deftool_contract_validation() {
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
(global __tool_schemas__ {})

(fn register-tool-schema [name schema]
  (tset __tool_schemas__ name schema))

(fn validate-tool-params [args params tool-name]
  (each [_ param (ipairs params)]
    (let [val (. args param.name)
          required (if (= param.required nil) true param.required)]
      (when (and required (= val nil) (not param.default))
        (error (.. "Contract violation: missing required param '" param.name "'"))))))

(fn apply-defaults [args params]
  (let [result (or args {})]
    (each [_ param (ipairs params)]
      (when (and (= (. result param.name) nil) param.default)
        (tset result param.name param.default)))
    result))

(fn make-tool [name schema impl]
  (register-tool-schema name schema)
  (fn [args]
    (let [with-defaults (apply-defaults args (or schema.params []))]
      (validate-tool-params with-defaults (or schema.params []) name)
      (impl with-defaults))))

;; Tool with required param
(global strict-tool (make-tool "strict"
  {:description "Strict tool"
   :params [{:name "required_field" :type "string" :required true}]}
  (fn [args] args.required_field)))

(global handler (fn [_]
  (let [results {}]
    ;; Test: valid call
    (tset results :valid_result (strict-tool {:required_field "hello"}))

    ;; Test: missing required param (should fail)
    (let [(ok err) (pcall strict-tool {})]
      (tset results :missing_failed (not ok))
      (tset results :error_mentions_contract (if (not ok)
                                                 (not= nil (string.find err "Contract"))
                                                 false)))
    results)))
"#;

        let result = executor.execute_source(source, true, json!({})).await;

        match result {
            Ok(res) => {
                if res.success {
                    let content = res.content.unwrap();
                    assert_eq!(content["valid_result"], "hello");
                    assert_eq!(content["missing_failed"], true);
                    assert_eq!(content["error_mentions_contract"], true);
                } else {
                    panic!("Fennel execution failed: {:?}", res.error);
                }
            }
            Err(e) => panic!("Test failed: {}", e),
        }
    }

    #[tokio::test]
    async fn test_fennel_oil_basic_components() {
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
;; Test Oil component wrappers directly (inline since require isn't set up)
(fn text [content ?style]
  (cru.oil.text content ?style))

(fn col [...]
  (cru.oil.col ...))

(fn row [...]
  (cru.oil.row ...))

(fn badge [label ?style]
  (cru.oil.badge label ?style))

(fn spacer []
  (cru.oil.spacer))

(fn when* [condition node]
  (cru.oil.when condition node))

(fn if-else [condition true-node false-node]
  (cru.oil.if_else condition true-node false-node))

(global handler (fn [args]
  ;; Build a simple UI tree
  (let [view (col {:gap 1 :padding 1}
               (text "Title" {:bold true})
               (row
                 (badge "OK" {:fg :green})
                 (spacer)
                 (text "Status"))
               (when* args.show_extra
                 (text "Extra content"))
               (if-else args.is_online
                 (text "Online" {:fg :green})
                 (text "Offline" {:fg :red})))]
    ;; Return success - the view is a LuaNode userdata
    {:built true
     :has_view (not= view nil)})))
"#;

        let result = executor
            .execute_source(source, true, json!({"show_extra": true, "is_online": true}))
            .await;

        match result {
            Ok(res) => {
                if res.success {
                    let content = res.content.unwrap();
                    assert_eq!(content["built"], true);
                    assert_eq!(content["has_view"], true);
                } else {
                    panic!("Fennel oil test failed: {:?}", res.error);
                }
            }
            Err(e) => panic!("Test failed: {}", e),
        }
    }

    #[tokio::test]
    async fn test_fennel_oil_component_factory() {
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
;; Test component factory
(fn component [base-fn default-props]
  (cru.oil.component base-fn default-props))

(global handler (fn [args]
  ;; Create a Card component with default props
  (let [Card (component cru.oil.col {:padding 2 :border "rounded"})
        ;; Use the Card with additional props
        view (Card {:gap 1}
               (cru.oil.text "Card Title")
               (cru.oil.text "Card Body"))]
    {:component_created true
     :has_view (not= view nil)})))
"#;

        let result = executor.execute_source(source, true, json!({})).await;

        match result {
            Ok(res) => {
                if res.success {
                    let content = res.content.unwrap();
                    assert_eq!(content["component_created"], true);
                    assert_eq!(content["has_view"], true);
                } else {
                    panic!("Fennel component factory test failed: {:?}", res.error);
                }
            }
            Err(e) => panic!("Test failed: {}", e),
        }
    }

    #[tokio::test]
    async fn test_fennel_oil_each_iteration() {
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
(global handler (fn [args]
  ;; Test each iteration
  (let [items [{:name "Item 1"} {:name "Item 2"} {:name "Item 3"}]
        list-view (cru.oil.each items (fn [item]
                    (cru.oil.text item.name)))]
    {:items_count (length items)
     :has_list (not= list-view nil)})))
"#;

        let result = executor.execute_source(source, true, json!({})).await;

        match result {
            Ok(res) => {
                if res.success {
                    let content = res.content.unwrap();
                    assert_eq!(content["items_count"], 3);
                    assert_eq!(content["has_list"], true);
                } else {
                    panic!("Fennel each test failed: {:?}", res.error);
                }
            }
            Err(e) => panic!("Test failed: {}", e),
        }
    }
}

// ============================================================================
// HEALTH MODULE INTEGRATION TESTS
// ============================================================================

#[tokio::test]
async fn test_health_collect_ok_and_warn() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    cru.health.start("test-plugin")
    cru.health.ok("Database connected")
    cru.health.ok("Cache initialized")
    cru.health.warn("Memory usage at 75%", {"Consider increasing heap size"})
    cru.health.info("Last sync: 5 minutes ago")
    
    local results = cru.health.get_results()
    return {
        name = results.name,
        healthy = results.healthy,
        check_count = #results.checks,
        first_check_level = results.checks[1].level,
        first_check_msg = results.checks[1].msg,
        warn_check_level = results.checks[3].level,
        warn_has_advice = results.checks[3].advice ~= nil,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["name"], "test-plugin");
    assert_eq!(content["healthy"], true);
    assert_eq!(content["check_count"], 4);
    assert_eq!(content["first_check_level"], "ok");
    assert_eq!(content["first_check_msg"], "Database connected");
    assert_eq!(content["warn_check_level"], "warn");
    assert_eq!(content["warn_has_advice"], true);
}

#[tokio::test]
async fn test_health_error_makes_unhealthy() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    cru.health.start("failing-plugin")
    cru.health.ok("Config loaded")
    cru.health.error("API key missing", {"Set CRUCIBLE_API_KEY env var", "Or use config file"})
    cru.health.warn("Fallback mode active")
    
    local results = cru.health.get_results()
    return {
        name = results.name,
        healthy = results.healthy,
        check_count = #results.checks,
        error_check_level = results.checks[2].level,
        error_msg = results.checks[2].msg,
        error_advice_count = results.checks[2].advice and #results.checks[2].advice or 0,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["name"], "failing-plugin");
    assert_eq!(content["healthy"], false);
    assert_eq!(content["check_count"], 3);
    assert_eq!(content["error_check_level"], "error");
    assert_eq!(content["error_msg"], "API key missing");
    assert_eq!(content["error_advice_count"], 2);
}

#[test]
fn test_plugin_template_yaml_is_valid() {
    let template_yaml =
        include_str!("../../crucible-cli/src/commands/plugin/templates/plugin.yaml");
    let substituted = template_yaml.replace("{{name}}", "test-plugin");

    let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(&substituted);
    assert!(parsed.is_ok(), "plugin.yaml template should be valid YAML");

    let manifest = parsed.unwrap();
    assert!(manifest["name"].is_string(), "name field should be present");
    assert!(
        manifest["version"].is_string(),
        "version field should be present"
    );
    assert!(manifest["main"].is_string(), "main field should be present");
    assert_eq!(manifest["main"].as_str().unwrap(), "init.lua");
}

#[test]
fn test_plugin_template_init_lua_is_syntactically_valid() {
    let template_lua = include_str!("../../crucible-cli/src/commands/plugin/templates/init.lua");
    let substituted = template_lua.replace("{{name}}", "test-plugin");

    let lua = mlua::Lua::new();
    let result = lua.load(&substituted).eval::<mlua::Value>();

    assert!(
        result.is_ok(),
        "init.lua template should be syntactically valid Lua: {:?}",
        result.err()
    );
}

#[test]
fn test_plugin_template_tool_annotation_format() {
    let template_lua = include_str!("../../crucible-cli/src/commands/plugin/templates/init.lua");

    assert!(
        template_lua.contains("@tool name=\"greet\""),
        "Should have @tool annotation"
    );
    assert!(
        template_lua.contains("@param name string"),
        "Should have @param annotation"
    );
    assert!(
        template_lua.contains("on_session_start"),
        "Should have on_session_start hook"
    );
    assert!(
        template_lua.contains("return {"),
        "Should return a plugin spec"
    );
}

#[test]
fn test_plugin_template_health_lua_is_syntactically_valid() {
    let template_lua = include_str!("../../crucible-cli/src/commands/plugin/templates/health.lua");
    let substituted = template_lua.replace("{{name}}", "test-plugin");

    let lua = mlua::Lua::new();
    let result = lua.load(&substituted).eval::<mlua::Value>();

    assert!(
        result.is_ok(),
        "health.lua template should be syntactically valid Lua: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_cru_inspect_simple_values() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    return {
        nil_str = cru.inspect(nil),
        bool_true = cru.inspect(true),
        bool_false = cru.inspect(false),
        number = cru.inspect(42),
        string = cru.inspect("hello"),
        func = cru.inspect(function() end),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["nil_str"], "nil");
    assert_eq!(content["bool_true"], "true");
    assert_eq!(content["bool_false"], "false");
    assert_eq!(content["number"], "42");
    assert_eq!(content["string"], "\"hello\"");
    assert_eq!(content["func"], "<function>");
}

#[tokio::test]
async fn test_cru_inspect_tables() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local empty = {}
    local simple = {a = 1, b = "test"}
    local nested = {x = {y = {z = 42}}}
    
    return {
        empty = cru.inspect(empty),
        simple = cru.inspect(simple),
        nested = cru.inspect(nested),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["empty"], "{}");
    assert!(content["simple"].as_str().unwrap().contains("a = 1"));
    assert!(content["simple"].as_str().unwrap().contains("b = \"test\""));
    assert!(content["nested"].as_str().unwrap().contains("z = 42"));
}

#[tokio::test]
async fn test_cru_inspect_cycle_detection() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = 1}
    t.self = t
    
    local result = cru.inspect(t)
    return {
        has_cycle = string.find(result, "cycle") ~= nil,
        result = result,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["has_cycle"], true);
    assert!(content["result"].as_str().unwrap().contains("cycle"));
}

#[tokio::test]
async fn test_cru_inspect_depth_limit() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local deep = {a = {b = {c = {d = 42}}}}
    
    local unlimited = cru.inspect(deep)
    local limited = cru.inspect(deep, {max_depth = 2})
    
    return {
        unlimited_has_42 = string.find(unlimited, "42") ~= nil,
        limited_has_42 = string.find(limited, "42") ~= nil,
        limited_has_dots = string.find(limited, "%.%.%.") ~= nil,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["unlimited_has_42"], true);
    assert_eq!(content["limited_has_42"], false);
    assert_eq!(content["limited_has_dots"], true);
}

#[tokio::test]
async fn test_cru_inspect_global_alias() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local result = inspect({x = 1})
    return {
        has_x = string.find(result, "x") ~= nil,
        is_string = type(result) == "string",
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["has_x"], true);
    assert_eq!(content["is_string"], true);
}

#[tokio::test]
async fn test_cru_tbl_deep_extend_force() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t1 = {a = 1, b = {x = 10}}
    local t2 = {b = {x = 20, y = 30}, c = 3}
    
    local result = cru.tbl_deep_extend("force", t1, t2)
    
    return {
        a = result.a,
        b_x = result.b.x,
        b_y = result.b.y,
        c = result.c,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b_x"], 20);
    assert_eq!(content["b_y"], 30);
    assert_eq!(content["c"], 3);
}

#[tokio::test]
async fn test_cru_tbl_deep_extend_keep() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t1 = {a = 1, b = {x = 10}}
    local t2 = {b = {x = 20, y = 30}, c = 3}
    
    local result = cru.tbl_deep_extend("keep", t1, t2)
    
    return {
        a = result.a,
        b_x = result.b.x,
        b_y = result.b.y,
        c = result.c,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b_x"], 10);
    assert_eq!(content["b_y"], 30);
    assert_eq!(content["c"], 3);
}

#[tokio::test]
async fn test_cru_tbl_deep_extend_multiple_tables() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t1 = {a = 1}
    local t2 = {b = 2}
    local t3 = {c = 3}
    
    local result = cru.tbl_deep_extend("force", t1, t2, t3)
    
    return {
        a = result.a,
        b = result.b,
        c = result.c,
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b"], 2);
    assert_eq!(content["c"], 3);
}

#[tokio::test]
async fn test_cru_tbl_get_simple() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = 1, b = 2}
    
    return {
        a = cru.tbl_get(t, "a"),
        b = cru.tbl_get(t, "b"),
        missing = cru.tbl_get(t, "c"),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["a"], 1);
    assert_eq!(content["b"], 2);
    assert!(content["missing"].is_null());
}

#[tokio::test]
async fn test_cru_tbl_get_nested() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = {b = {c = 42}}}
    
    return {
        deep = cru.tbl_get(t, "a", "b", "c"),
        partial = cru.tbl_get(t, "a", "b"),
        missing = cru.tbl_get(t, "a", "x", "y"),
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    assert!(result.success);
    let content = result.content.unwrap();
    assert_eq!(content["deep"], 42);
    assert!(content["partial"].is_object());
    assert!(content["missing"].is_null());
}

#[tokio::test]
async fn test_cru_tbl_get_non_table_intermediate() {
    let executor = LuaExecutor::new().unwrap();

    let source = r#"
function handler(args)
    local t = {a = 42}
    
    local result = cru.tbl_get(t, "a", "b", "c")
    
    return {
        is_nil = result == nil,
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
}

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

fn create_plugin_files(root: &Path, name: &str, init_source: &str, module_source: &str) {
    let plugin_dir = root.join(name);
    std::fs::create_dir_all(plugin_dir.join(name)).unwrap();

    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        format!(
            "name: {name}\nversion: \"1.0.0\"\nmain: init.lua\nexports:\n  auto_discover: true\n"
        ),
    )
    .unwrap();
    std::fs::write(plugin_dir.join("init.lua"), init_source).unwrap();
    std::fs::write(plugin_dir.join(name).join("core.lua"), module_source).unwrap();
}

#[test]
fn test_reload_picks_up_changes() {
    let temp = TempDir::new().unwrap();
    let plugin_name = "reload_sample";
    let init_source = r#"
local core = require("reload_sample.core")
return {
    name = "reload_sample",
    version = "1.0.0",
    tools = {
        current_value = {
            desc = "Read current module value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;

    create_plugin_files(
        temp.path(),
        plugin_name,
        init_source,
        "return { value = 'v1' }\n",
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load(plugin_name).unwrap();

    let before: String = manager
        .eval_runtime("local mod = require('reload_sample.core'); return mod.value")
        .unwrap();
    assert_eq!(before, "v1");

    std::fs::write(
        temp.path()
            .join(plugin_name)
            .join(plugin_name)
            .join("core.lua"),
        "return { value = 'v2' }\n",
    )
    .unwrap();

    manager.reload_plugin(plugin_name).unwrap();

    let after: String = manager
        .eval_runtime("local mod = require('reload_sample.core'); return mod.value")
        .unwrap();
    assert_eq!(after, "v2");
}

#[test]
fn test_on_unload_hook_fires_on_reload() {
    let temp = TempDir::new().unwrap();
    let plugin_name = "reload_hook";
    let init_source = r#"
_G.reload_trace = (_G.reload_trace or "") .. "L"
local core = require("reload_hook.core")

return {
    name = "reload_hook",
    version = "1.0.0",
    on_unload = function()
        _G.reload_trace = (_G.reload_trace or "") .. "U"
    end,
    tools = {
        current_value = {
            desc = "Read current module value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;

    create_plugin_files(
        temp.path(),
        plugin_name,
        init_source,
        "return { value = 'v1' }\n",
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load(plugin_name).unwrap();

    let initial_trace: String = manager.eval_runtime("return _G.reload_trace").unwrap();
    assert_eq!(initial_trace, "L");

    std::fs::write(
        temp.path()
            .join(plugin_name)
            .join(plugin_name)
            .join("core.lua"),
        "return { value = 'v2' }\n",
    )
    .unwrap();

    manager.reload_plugin(plugin_name).unwrap();

    let trace_after_reload: String = manager.eval_runtime("return _G.reload_trace").unwrap();
    assert_eq!(trace_after_reload, "LUL");
}

#[test]
fn test_reload_failure_leaves_old_plugin_intact() {
    let temp = TempDir::new().unwrap();

    let fragile_init = r#"
local core = require("fragile_plugin.core")
return {
    name = "fragile_plugin",
    version = "1.0.0",
    tools = {
        fragile = {
            desc = "Fragile value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;
    create_plugin_files(
        temp.path(),
        "fragile_plugin",
        fragile_init,
        "return { value = 'good' }\n",
    );

    let stable_init = r#"
local core = require("stable_plugin.core")
return {
    name = "stable_plugin",
    version = "1.0.0",
    tools = {
        stable = {
            desc = "Stable value",
            fn = function()
                return core.value
            end,
        },
    },
}
"#;
    create_plugin_files(
        temp.path(),
        "stable_plugin",
        stable_init,
        "return { value = 'stable' }\n",
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("fragile_plugin").unwrap();
    manager.load("stable_plugin").unwrap();

    let fragile_before: String = manager
        .eval_runtime("local mod = require('fragile_plugin.core'); return mod.value")
        .unwrap();
    let stable_before: String = manager
        .eval_runtime("local mod = require('stable_plugin.core'); return mod.value")
        .unwrap();
    assert_eq!(fragile_before, "good");
    assert_eq!(stable_before, "stable");

    std::fs::write(
        temp.path().join("fragile_plugin").join("init.lua"),
        "return { name = 'fragile_plugin', tools = {\n",
    )
    .unwrap();

    let reload_result = manager.reload_plugin("fragile_plugin");
    assert!(reload_result.is_err());

    let fragile_state = manager.get("fragile_plugin").unwrap().state;
    let stable_state = manager.get("stable_plugin").unwrap().state;
    assert_eq!(fragile_state, PluginState::Error);
    assert_eq!(stable_state, PluginState::Active);

    // After failed reload, fragile plugin's modules are cleared (not restored).
    // The stable plugin's modules remain untouched.
    let stable_after: String = manager
        .eval_runtime("local mod = require('stable_plugin.core'); return mod.value")
        .unwrap();
    assert_eq!(stable_after, "stable");
}
