//! Integration tests for Lua/Fennel tool discovery and execution

use crucible_lua::{register_oq_module, register_shell_module, LuaExecutor, LuaToolRegistry, ShellPolicy};
use serde_json::json;
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
