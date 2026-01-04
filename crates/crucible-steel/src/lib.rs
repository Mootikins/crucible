//! Steel (Scheme) scripting runtime for Crucible
//!
//! Steel provides:
//! - Native contracts with blame tracking (Steel's killer feature)
//! - Hygienic macros
//! - Racket-style semantics
//! - Idiomatic Scheme/Lisp syntax for knowledge graph operations
//!
//! Note: Steel's Engine is !Send/!Sync, so execution uses spawn_blocking.

pub mod error;
pub mod executor;
pub mod graph;
pub mod json_query;
pub mod popup;
pub mod registry;
pub mod schema;
pub mod shell;
pub mod types;

pub use error::SteelError;
pub use executor::SteelExecutor;
pub use graph::{GraphModule, GraphViewModule, NoteStoreModule};
pub use json_query::{Format as OqFormat, OqModule};
pub use popup::PopupModule;
pub use registry::SteelToolRegistry;
pub use schema::{ContractSignature, ContractType};
pub use shell::{ShellModule, ShellPolicy};
pub use types::{SteelTool, ToolParam};

/// Steel library source code
pub mod lib_sources {
    /// Crucible prelude with common utilities
    pub const PRELUDE: &str = include_str!("../lib/prelude.scm");

    /// Graph traversal library (pure Scheme)
    pub const GRAPH: &str = include_str!("../lib/graph.scm");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// RED: Basic script execution
    /// This test should fail because SteelExecutor doesn't exist yet
    #[tokio::test]
    async fn test_execute_simple_expression() {
        let executor = SteelExecutor::new().unwrap();

        let result = executor.execute_source("(+ 1 2 3)").await.unwrap();

        assert_eq!(result, json!(6));
    }

    /// RED: Execute function and call it
    #[tokio::test]
    async fn test_execute_function_with_args() {
        let executor = SteelExecutor::new().unwrap();

        let source = r#"
            (define (add-numbers x y)
              (+ x y))
        "#;

        // Load the definition
        executor.execute_source(source).await.unwrap();

        // Call the function with args
        let result = executor
            .call_function("add-numbers", vec![json!(5), json!(3)])
            .await
            .unwrap();

        assert_eq!(result, json!(8));
    }

    /// RED: Contract violation should return error with blame
    #[tokio::test]
    async fn test_contract_violation_with_blame() {
        let executor = SteelExecutor::new().unwrap();

        // Define a function with a contract
        let source = r#"
            (define/contract (positive-add x y)
              (->/c positive? positive? positive?)
              (+ x y))
        "#;

        executor.execute_source(source).await.unwrap();

        // This should fail: -5 is not positive
        let result = executor
            .call_function("positive-add", vec![json!(-5), json!(3)])
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("contract"),
            "Error should mention contract: {}",
            err
        );
    }

    /// Test basic arithmetic contract
    #[tokio::test]
    async fn test_number_contract() {
        let executor = SteelExecutor::new().unwrap();

        // Simple contract: both inputs and output must be numbers
        let source = r#"
            (define/contract (increment x)
              (->/c number? number?)
              (+ x 1))
        "#;

        executor.execute_source(source).await.unwrap();

        let result = executor
            .call_function("increment", vec![json!(5)])
            .await
            .unwrap();

        assert_eq!(result, json!(6));
    }

    // =========================================================================
    // Tool Registry Tests (RED - should fail initially)
    // =========================================================================

    #[tokio::test]
    async fn test_tool_discovery_from_directory() {
        use tempfile::TempDir;
        use tokio::fs;

        let dir = TempDir::new().unwrap();

        // Create a Steel tool file with annotation
        let tool_source = r#"
;;; Search notes by query
;; @tool
;; @param query string The search query
;; @param limit number Maximum results
(define (handler args)
  (list (hash 'title "Result 1" 'score 0.95)))
"#;
        fs::write(dir.path().join("search.scm"), tool_source)
            .await
            .unwrap();

        // Discover tools
        let mut registry = SteelToolRegistry::new().unwrap();
        let count = registry.discover_from(dir.path()).await.unwrap();

        assert_eq!(count, 1, "Should discover 1 tool");

        let tools = registry.list_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "handler");
    }

    #[tokio::test]
    async fn test_tool_execution_via_registry() {
        use tempfile::TempDir;
        use tokio::fs;

        let dir = TempDir::new().unwrap();

        // Simple tool that takes a number and doubles it
        let tool_source = r#"
;;; Double a number
;; @tool
;; @param n number The number to double
(define (handler n)
  (* n 2))
"#;
        fs::write(dir.path().join("double.scm"), tool_source)
            .await
            .unwrap();

        let mut registry = SteelToolRegistry::new().unwrap();
        registry.discover_from(dir.path()).await.unwrap();

        // Execute tool with a simple number
        let result = registry.execute("handler", json!(7)).await.unwrap();

        if !result.success {
            panic!("Tool execution failed: {:?}", result.error);
        }
        assert_eq!(result.content, json!(14));
    }

    #[tokio::test]
    async fn test_tool_with_contract_validation() {
        use tempfile::TempDir;
        use tokio::fs;

        let dir = TempDir::new().unwrap();

        // Tool with contract - takes a positive number
        let tool_source = r#"
;;; Increment with contract
;; @tool
;; @param n number A positive number
(define/contract (handler n)
  (->/c positive? positive?)
  (+ n 1))
"#;
        fs::write(dir.path().join("increment.scm"), tool_source)
            .await
            .unwrap();

        let mut registry = SteelToolRegistry::new().unwrap();
        registry.discover_from(dir.path()).await.unwrap();

        // Valid call
        let result = registry.execute("handler", json!(5)).await.unwrap();

        if !result.success {
            panic!("Tool execution failed: {:?}", result.error);
        }
        assert_eq!(result.content, json!(6));
    }

    // =========================================================================
    // JSON Object → Hashmap Tests
    // =========================================================================

    /// Test that JSON objects are converted to Steel hashmaps that can be queried with hash-ref
    #[tokio::test]
    async fn test_json_object_to_hashmap() {
        let executor = SteelExecutor::new().unwrap();

        // Define a function that extracts a field from a hashmap
        let source = r#"
            (define (get-name obj)
              (hash-ref obj 'name))
        "#;

        executor.execute_source(source).await.unwrap();

        // Pass a JSON object - should be usable as a hashmap
        let result = executor
            .call_function("get-name", vec![json!({"name": "Alice", "age": 30})])
            .await
            .unwrap();

        assert_eq!(result, json!("Alice"));
    }

    /// Test nested JSON objects
    #[tokio::test]
    async fn test_nested_json_object() {
        let executor = SteelExecutor::new().unwrap();

        let source = r#"
            (define (get-city person)
              (hash-ref (hash-ref person 'address) 'city))
        "#;

        executor.execute_source(source).await.unwrap();

        let result = executor
            .call_function(
                "get-city",
                vec![json!({
                    "name": "Bob",
                    "address": {
                        "city": "Seattle",
                        "zip": "98101"
                    }
                })],
            )
            .await
            .unwrap();

        assert_eq!(result, json!("Seattle"));
    }
}

#[cfg(test)]
mod builtin_tests {
    use super::*;

    #[tokio::test]
    async fn test_steel_filter() {
        let executor = SteelExecutor::new().unwrap();
        let r = executor
            .execute_source("(filter (lambda (x) (> x 2)) '(1 2 3 4))")
            .await;
        println!("filter result: {:?}", r);
    }

    #[tokio::test]
    async fn test_steel_member() {
        let executor = SteelExecutor::new().unwrap();
        let r = executor.execute_source("(member 2 '(1 2 3))").await;
        println!("member result: {:?}", r);
    }

    #[tokio::test]
    async fn test_simple_lib() {
        let executor = SteelExecutor::new().unwrap();
        // Try without provide
        let r = executor
            .execute_source(
                r#"
            (define (note-title note) (hash-ref note 'title))
            (note-title (hash 'title "Test"))
        "#,
            )
            .await;
        println!("simple lib result: {:?}", r);
    }
}

#[cfg(test)]
mod prelude_tests {
    use super::*;
    use serde_json::json;

    // Include prelude
    const PRELUDE: &str = lib_sources::PRELUDE;

    #[tokio::test]
    async fn test_prelude_identity() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor.execute_source("(identity 42)").await.unwrap();
        assert_eq!(result, json!(42));
    }

    #[tokio::test]
    async fn test_prelude_take() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source("(take 2 '(1 2 3 4 5))")
            .await
            .unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[tokio::test]
    async fn test_prelude_drop() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source("(drop 2 '(1 2 3 4 5))")
            .await
            .unwrap();
        assert_eq!(result, json!([3, 4, 5]));
    }

    #[tokio::test]
    async fn test_prelude_hash_get() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source("(hash-get (hash 'a 1 'b 2) 'a 0)")
            .await
            .unwrap();
        assert_eq!(result, json!(1));
    }

    #[tokio::test]
    async fn test_prelude_hash_get_default() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source("(hash-get (hash 'a 1) 'missing 999)")
            .await
            .unwrap();
        assert_eq!(result, json!(999));
    }

    #[tokio::test]
    async fn test_prelude_make_note() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source(r#"(hash-ref (make-note "Test" "test.md") 'title)"#)
            .await
            .unwrap();
        assert_eq!(result, json!("Test"));
    }

    #[tokio::test]
    async fn test_prelude_ok_result() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor.execute_source("(ok? (ok 42))").await.unwrap();
        assert_eq!(result, json!(true));
    }

    #[tokio::test]
    async fn test_prelude_err_result() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source(r#"(err? (err "something failed"))"#)
            .await
            .unwrap();
        assert_eq!(result, json!(true));
    }

    #[tokio::test]
    async fn test_prelude_find() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source("(find (lambda (x) (> x 5)) '(1 3 7 2 8))")
            .await
            .unwrap();
        assert_eq!(result, json!(7));
    }

    #[tokio::test]
    async fn test_prelude_any() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source("(any? (lambda (x) (> x 5)) '(1 3 7))")
            .await
            .unwrap();
        assert_eq!(result, json!(true));
    }

    #[tokio::test]
    async fn test_prelude_all() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(PRELUDE).await.unwrap();

        let result = executor
            .execute_source("(all? (lambda (x) (> x 0)) '(1 2 3))")
            .await
            .unwrap();
        assert_eq!(result, json!(true));
    }
}
