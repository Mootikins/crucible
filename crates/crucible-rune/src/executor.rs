//! Execute Rune scripts

use crate::types::{RuneExecutionResult, RuneTool};
use crate::RuneError;
use rune::ast;
use rune::compile;
use rune::parse::Parser;
use rune::runtime::{RuntimeContext, ToValue, VmError};
use rune::{Context, Diagnostics, Module, Source, Sources, Unit, Value, Vm};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

/// Execute Rune tools
pub struct RuneExecutor {
    /// Shared Rune context
    context: Arc<Context>,
    /// Runtime context for VM
    runtime: Arc<RuntimeContext>,
}

impl RuneExecutor {
    /// Create a new executor with default context
    pub fn new() -> Result<Self, RuneError> {
        let mut context = Context::with_default_modules()
            .map_err(|e| RuneError::Context(e.to_string()))?;

        // Add rune-modules for extra functionality
        // The bool argument indicates whether to use stdio (false = use strings)
        context
            .install(rune_modules::json::module(false)?)
            .map_err(|e| RuneError::Context(e.to_string()))?;
        context
            .install(rune_modules::toml::module(false)?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        // Register our custom metadata attributes so Rune accepts them
        // These are no-op macros - they just pass through the item unchanged
        // The discovery system parses them via regex, not via Rune's compiler
        context
            .install(Self::metadata_macros_module()?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        let runtime = Arc::new(context.runtime().map_err(|e| RuneError::Context(e.to_string()))?);

        Ok(Self {
            context: Arc::new(context),
            runtime,
        })
    }

    /// Create a module with no-op attribute macros for tool metadata
    ///
    /// These macros (`#[tool(...)]`, `#[param(...)]`, `#[hook(...)]`) are parsed by the
    /// discovery system but need to be accepted by Rune's compiler. They simply pass
    /// through the decorated item unchanged.
    fn metadata_macros_module() -> Result<Module, compile::ContextError> {
        let mut module = Module::new();

        // #[tool(...)] - marks a function as a tool with metadata
        // No-op: just return the item unchanged (metadata extracted by discovery regex)
        module.attribute_macro(["tool"], |cx, _input, item| {
            // Parse the item and re-emit it unchanged
            let mut parser = Parser::from_token_stream(item, cx.macro_span());
            let item_fn = parser.parse::<ast::Item>()?;
            let output = rune::macros::quote!(#item_fn);
            Ok(output.into_token_stream(cx)?)
        })?;

        // #[param(...)] - describes a parameter
        // No-op: just return the item unchanged
        module.attribute_macro(["param"], |cx, _input, item| {
            let mut parser = Parser::from_token_stream(item, cx.macro_span());
            let item_fn = parser.parse::<ast::Item>()?;
            let output = rune::macros::quote!(#item_fn);
            Ok(output.into_token_stream(cx)?)
        })?;

        // #[hook(...)] - marks a function as an event handler
        // No-op: just return the item unchanged (metadata extracted by discovery regex)
        module.attribute_macro(["hook"], |cx, _input, item| {
            let mut parser = Parser::from_token_stream(item, cx.macro_span());
            let item_fn = parser.parse::<ast::Item>()?;
            let output = rune::macros::quote!(#item_fn);
            Ok(output.into_token_stream(cx)?)
        })?;

        Ok(module)
    }

    /// Execute a tool with the given arguments
    pub async fn execute(
        &self,
        tool: &RuneTool,
        args: JsonValue,
    ) -> Result<RuneExecutionResult, RuneError> {
        let start = Instant::now();

        // Read the script
        let source_code = std::fs::read_to_string(&tool.path)
            .map_err(|e| RuneError::Io(e.to_string()))?;

        // Compile the script
        let unit = self.compile(&tool.name, &source_code)?;

        // Execute
        let result = self.run_unit(&unit, &tool.entry_point, args).await;

        let duration = start.elapsed().as_millis() as u64;

        match result {
            Ok(value) => Ok(RuneExecutionResult::success(&tool.name, value, duration)),
            Err(e) => Ok(RuneExecutionResult::failure(&tool.name, e.to_string(), duration)),
        }
    }

    /// Compile a Rune script to a reusable Unit
    pub fn compile(&self, name: &str, source: &str) -> Result<Arc<Unit>, RuneError> {
        let mut sources = Sources::new();
        sources
            .insert(Source::new(name, source).map_err(|e| RuneError::Compile(e.to_string()))?)
            .map_err(|e| RuneError::Compile(e.to_string()))?;

        let mut diagnostics = Diagnostics::new();

        let result = rune::prepare(&mut sources)
            .with_context(&self.context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut errors = Vec::new();
            for diagnostic in diagnostics.diagnostics() {
                errors.push(format!("{:?}", diagnostic));
            }
            if !errors.is_empty() {
                debug!("Rune diagnostics: {:?}", errors);
            }
        }

        let unit = result.map_err(|e| {
            // Include diagnostics in the error message
            let diag_str: Vec<String> = diagnostics
                .diagnostics()
                .iter()
                .map(|d| format!("{:?}", d))
                .collect();
            RuneError::Compile(format!("{}\nDiagnostics: {}", e, diag_str.join("\n")))
        })?;

        Ok(Arc::new(unit))
    }

    /// Call a function on a compiled unit with the given arguments
    ///
    /// This allows calling arbitrary functions in Rune scripts, not just
    /// tool entry points. Useful for plugin init() functions and event handlers.
    ///
    /// # Arguments
    /// * `unit` - The compiled Rune unit
    /// * `fn_name` - Name of the function to call
    /// * `args` - Tuple of arguments (use () for no args, (val,) for one, (a, b) for two, etc.)
    ///
    /// # Returns
    /// The function's return value as JSON. Option::None becomes JSON null.
    pub async fn call_function<A>(
        &self,
        unit: &Arc<Unit>,
        fn_name: &str,
        args: A,
    ) -> Result<JsonValue, RuneError>
    where
        A: rune::runtime::Args + rune::runtime::GuardedArgs,
    {
        let mut vm = Vm::new(self.runtime.clone(), unit.clone());

        let hash = rune::Hash::type_hash([fn_name]);

        let output = vm
            .call(hash, args)
            .map_err(|e| RuneError::Execution(format_vm_error(e)))?;

        // Check if the return value is a generator/future (async function)
        // Only call async_complete() if it's actually an async function
        let type_info = output.type_info();
        let type_name = format!("{}", type_info);

        let output = if type_name.contains("Generator") || type_name.contains("Future") {
            // This is an async function, complete it
            vm.async_complete().await
                .map_err(|e| RuneError::Execution(format_vm_error(e)))?
        } else {
            // Synchronous function, use the output directly
            output
        };

        // Convert output to JSON
        self.rune_to_json(output)
    }

    /// Run a compiled unit
    ///
    /// Argument passing strategy:
    /// - Empty args (null): Call with no arguments `()`
    /// - Single value: Call with one argument `(value,)`
    /// - Multiple values: Call with multiple arguments as tuple
    ///
    /// JSON objects have their VALUES extracted (keys discarded) and passed positionally.
    async fn run_unit(
        &self,
        unit: &Arc<Unit>,
        entry_point: &str,
        args: JsonValue,
    ) -> Result<JsonValue, RuneError> {
        let mut vm = Vm::new(self.runtime.clone(), unit.clone());

        // Convert JSON args to Rune values
        let rune_args = self.json_to_rune_args(args)?;

        // Look up the entry point function
        let hash = rune::Hash::type_hash([entry_point]);

        // Execute the function based on argument count
        // The key insight: vm.call expects a tuple of arguments, NOT a Vec wrapped in tuple
        let output = match rune_args.len() {
            0 => vm
                .call(hash, ())
                .map_err(|e| RuneError::Execution(format_vm_error(e)))?,
            1 => {
                // Single argument - pass directly with trailing comma for tuple
                let arg = rune_args.into_iter().next().unwrap();
                vm.call(hash, (arg,))
                    .map_err(|e| RuneError::Execution(format_vm_error(e)))?
            }
            2 => {
                // Two arguments
                let mut iter = rune_args.into_iter();
                let a1 = iter.next().unwrap();
                let a2 = iter.next().unwrap();
                vm.call(hash, (a1, a2))
                    .map_err(|e| RuneError::Execution(format_vm_error(e)))?
            }
            3 => {
                // Three arguments
                let mut iter = rune_args.into_iter();
                let a1 = iter.next().unwrap();
                let a2 = iter.next().unwrap();
                let a3 = iter.next().unwrap();
                vm.call(hash, (a1, a2, a3))
                    .map_err(|e| RuneError::Execution(format_vm_error(e)))?
            }
            4 => {
                // Four arguments
                let mut iter = rune_args.into_iter();
                let a1 = iter.next().unwrap();
                let a2 = iter.next().unwrap();
                let a3 = iter.next().unwrap();
                let a4 = iter.next().unwrap();
                vm.call(hash, (a1, a2, a3, a4))
                    .map_err(|e| RuneError::Execution(format_vm_error(e)))?
            }
            _ => {
                // More than 4 arguments - pass as array (function must accept array)
                // This is a limitation; most tools have ≤4 params
                debug!(
                    "Function has {} args, passing as array",
                    rune_args.len()
                );
                vm.call(hash, (rune_args,))
                    .map_err(|e| RuneError::Execution(format_vm_error(e)))?
            }
        };

        // Convert output to JSON
        self.rune_to_json(output)
    }

    /// Convert JSON value to Rune arguments
    fn json_to_rune_args(&self, value: JsonValue) -> Result<Vec<Value>, RuneError> {
        match value {
            JsonValue::Object(map) => {
                // Convert object to vec of values
                let mut args = Vec::new();
                for (_key, val) in map {
                    args.push(self.json_value_to_rune(val)?);
                }
                Ok(args)
            }
            JsonValue::Array(arr) => arr
                .into_iter()
                .map(|v| self.json_value_to_rune(v))
                .collect(),
            JsonValue::Null => Ok(vec![]),
            other => Ok(vec![self.json_value_to_rune(other)?]),
        }
    }

    /// Convert a JSON value to a Rune Value
    ///
    /// This is exposed publicly so callers can prepare complex arguments for call_function.
    pub fn json_to_rune_value(&self, value: JsonValue) -> Result<Value, RuneError> {
        self.json_value_to_rune(value)
    }

    /// Convert a single JSON value to Rune (internal)
    fn json_value_to_rune(&self, value: JsonValue) -> Result<Value, RuneError> {
        match value {
            JsonValue::Null => Ok(Value::empty()),
            JsonValue::Bool(b) => b.to_value().map_err(|e| RuneError::Conversion(e.to_string())),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    i.to_value().map_err(|e| RuneError::Conversion(e.to_string()))
                } else if let Some(f) = n.as_f64() {
                    f.to_value().map_err(|e| RuneError::Conversion(e.to_string()))
                } else {
                    Err(RuneError::Conversion("Invalid number".to_string()))
                }
            }
            JsonValue::String(s) => {
                s.to_value().map_err(|e| RuneError::Conversion(e.to_string()))
            }
            JsonValue::Array(arr) => {
                let values: Vec<Value> = arr
                    .into_iter()
                    .map(|v| self.json_value_to_rune(v))
                    .collect::<Result<Vec<_>, _>>()?;
                values.to_value().map_err(|e| RuneError::Conversion(e.to_string()))
            }
            JsonValue::Object(map) => {
                let obj: std::collections::HashMap<String, Value> = map
                    .into_iter()
                    .map(|(k, v)| Ok((k, self.json_value_to_rune(v)?)))
                    .collect::<Result<_, RuneError>>()?;
                obj.to_value().map_err(|e| RuneError::Conversion(e.to_string()))
            }
        }
    }

    /// Convert Rune output to JSON
    ///
    /// Properly converts Rune runtime values to their JSON equivalents:
    /// - String → JSON string
    /// - Integer (i64) → JSON number
    /// - Float (f64) → JSON number
    /// - Boolean → JSON boolean
    /// - Unit/empty → JSON null
    /// - Vec → JSON array (recursive)
    /// - HashMap → JSON object (recursive)
    fn rune_to_json(&self, value: Value) -> Result<JsonValue, RuneError> {
        // Get the type name as a string using Display trait
        let type_info = value.type_info();
        let type_name = format!("{}", type_info);

        // Match on the type name
        // Common patterns: "::std::string::String", "::std::i64", "::std::f64", "::std::bool", etc.
        if type_name.contains("String") {
            let s: String = rune::from_value(value)
                .map_err(|e| RuneError::Conversion(e.to_string()))?;
            Ok(JsonValue::String(s))
        } else if type_name == "i64" || type_name == "::std::i64" || type_name.ends_with("::i64") {
            let i: i64 = rune::from_value(value)
                .map_err(|e| RuneError::Conversion(e.to_string()))?;
            Ok(JsonValue::Number(i.into()))
        } else if type_name == "f64" || type_name == "::std::f64" || type_name.ends_with("::f64") {
            let f: f64 = rune::from_value(value)
                .map_err(|e| RuneError::Conversion(e.to_string()))?;
            if let Some(n) = serde_json::Number::from_f64(f) {
                Ok(JsonValue::Number(n))
            } else {
                // NaN or infinity - return as null
                Ok(JsonValue::Null)
            }
        } else if type_name == "bool" || type_name == "::std::bool" || type_name.ends_with("::bool") {
            let b: bool = rune::from_value(value)
                .map_err(|e| RuneError::Conversion(e.to_string()))?;
            Ok(JsonValue::Bool(b))
        } else if type_name == "unit" || type_name == "()" || type_name.contains("Unit") {
            Ok(JsonValue::Null)
        } else if type_name.contains("Vec") {
            let vec: Vec<Value> = rune::from_value(value)
                .map_err(|e| RuneError::Conversion(e.to_string()))?;
            let arr: Vec<JsonValue> = vec
                .into_iter()
                .map(|v| self.rune_to_json(v))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(JsonValue::Array(arr))
        } else if type_name.contains("Object") || type_name.contains("HashMap") {
            let map: std::collections::HashMap<String, Value> = rune::from_value(value)
                .map_err(|e| RuneError::Conversion(e.to_string()))?;
            let obj: serde_json::Map<String, JsonValue> = map
                .into_iter()
                .map(|(k, v)| Ok((k, self.rune_to_json(v)?)))
                .collect::<Result<_, RuneError>>()?;
            Ok(JsonValue::Object(obj))
        } else if type_name.contains("Option") {
            // Handle Option<T> - check if it's Some or None
            // Try to convert as Option - if it's None, return null
            // The value might be a variant, we need to check if it's the None variant
            match rune::from_value::<Option<Value>>(value.clone()) {
                Ok(Some(inner)) => self.rune_to_json(inner),
                Ok(None) => Ok(JsonValue::Null),
                Err(_) => {
                    // Fallback: might be Some variant, try to extract inner value
                    debug!("Option type but couldn't convert directly: {}", type_name);
                    Ok(JsonValue::Null)
                }
            }
        } else {
            // Unknown type - fall back to debug representation
            debug!(
                "Unknown Rune type '{}', using debug representation",
                type_name
            );
            Ok(JsonValue::String(format!("{:?}", value)))
        }
    }
}

impl Default for RuneExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default RuneExecutor")
    }
}

/// Format VM error for display
fn format_vm_error(error: VmError) -> String {
    format!("{}", error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_executor_creation() {
        let executor = RuneExecutor::new();
        assert!(executor.is_ok());
    }

    #[tokio::test]
    async fn test_simple_script_execution() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("test.rn");
        std::fs::write(&script_path, "pub fn main() { 42 }").unwrap();

        let executor = RuneExecutor::new().unwrap();
        let tool = RuneTool::new("test", script_path);

        let result = executor.execute(&tool, JsonValue::Null).await.unwrap();
        assert!(result.success);
    }

    // =========================================================================
    // TDD: Failing tests to reproduce Rune tool output issues
    // =========================================================================

    /// Test that string return values are properly converted to JSON strings
    /// NOT debug format like "String(\"Hello\")"
    #[tokio::test]
    async fn test_string_return_value_converts_to_json_string() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("string_test.rn");
        std::fs::write(
            &script_path,
            r#"pub fn main() { "Hello, World!" }"#,
        )
        .unwrap();

        let executor = RuneExecutor::new().unwrap();
        let tool = RuneTool::new("string_test", script_path);

        let result = executor.execute(&tool, JsonValue::Null).await.unwrap();
        assert!(result.success, "Execution should succeed");

        // The result should be a proper JSON string, NOT a debug representation
        let output = result.result.expect("Should have result");
        assert_eq!(
            output,
            JsonValue::String("Hello, World!".to_string()),
            "String return value should be a proper JSON string, not debug format"
        );
    }

    /// Test that integer return values are properly converted to JSON numbers
    #[tokio::test]
    async fn test_integer_return_value_converts_to_json_number() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("int_test.rn");
        std::fs::write(&script_path, "pub fn main() { 42 }").unwrap();

        let executor = RuneExecutor::new().unwrap();
        let tool = RuneTool::new("int_test", script_path);

        let result = executor.execute(&tool, JsonValue::Null).await.unwrap();
        assert!(result.success, "Execution should succeed");

        // The result should be a proper JSON number
        let output = result.result.expect("Should have result");
        assert_eq!(
            output,
            JsonValue::Number(42.into()),
            "Integer return value should be a JSON number"
        );
    }

    /// Test that functions with a single named parameter receive the argument correctly
    #[tokio::test]
    async fn test_function_receives_single_string_argument() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("greet_test.rn");
        // Function that takes a name parameter and returns a greeting
        std::fs::write(
            &script_path,
            r#"
pub fn greet(name) {
    `Hello, ${name}!`
}
"#,
        )
        .unwrap();

        let executor = RuneExecutor::new().unwrap();
        let mut tool = RuneTool::new("greet_test", script_path);
        tool = tool.with_entry_point("greet");

        // Pass argument as JSON object with named parameter
        let args = serde_json::json!({"name": "Claude"});

        let result = executor.execute(&tool, args).await.unwrap();
        assert!(
            result.success,
            "Execution should succeed, got error: {:?}",
            result.error
        );

        // The result should be the greeting string
        let output = result.result.expect("Should have result");
        assert_eq!(
            output,
            JsonValue::String("Hello, Claude!".to_string()),
            "Function should receive argument and return greeting"
        );
    }

    /// Test that the greet function from utils.rn pattern works correctly
    /// Now with #[tool] and #[param] attributes which are registered as no-op macros
    #[tokio::test]
    async fn test_utils_greet_pattern_with_attributes() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("utils.rn");
        // Exact pattern from utils.rn - NOW WORKS because we register #[tool] and #[param]
        std::fs::write(
            &script_path,
            r#"
/// Greet the user with a message
#[tool(desc = "Returns a friendly greeting")]
#[param(name = "name", type = "string", desc = "Name to greet")]
pub fn greet(name) {
    `Hello, ${name}! Welcome to Crucible.`
}
"#,
        )
        .unwrap();

        let executor = RuneExecutor::new().unwrap();
        let mut tool = RuneTool::new("greet", script_path);
        tool = tool.with_entry_point("greet");

        let args = serde_json::json!({"name": "Claude"});

        let result = executor.execute(&tool, args).await.unwrap();
        assert!(
            result.success,
            "Execution should succeed, got error: {:?}",
            result.error
        );

        let output = result.result.expect("Should have result");
        assert_eq!(
            output,
            JsonValue::String("Hello, Claude! Welcome to Crucible.".to_string()),
            "Greet function should return proper greeting string"
        );
    }

    // =========================================================================
    // TDD: Tests for call_function API
    // =========================================================================

    #[tokio::test]
    async fn test_compile_returns_arc_unit() {
        let executor = RuneExecutor::new().unwrap();
        let source = "pub fn hello() { 42 }";
        let unit = executor.compile("test", source);
        assert!(unit.is_ok());
    }

    #[tokio::test]
    async fn test_call_function_no_args() {
        let executor = RuneExecutor::new().unwrap();
        let source = "pub fn get_value() { 42 }";
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "get_value", ()).await.unwrap();
        assert_eq!(result, serde_json::json!(42));
    }

    #[tokio::test]
    async fn test_call_function_single_arg() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"pub fn double(n) { n * 2 }"#;
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "double", (21i64,)).await.unwrap();
        assert_eq!(result, serde_json::json!(42));
    }

    #[tokio::test]
    async fn test_call_function_two_args() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"pub fn add(a, b) { a + b }"#;
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "add", (10i64, 32i64)).await.unwrap();
        assert_eq!(result, serde_json::json!(42));
    }

    #[tokio::test]
    async fn test_call_function_with_json_arg() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"
pub fn process(event) {
    event.value * 2
}
"#;
        let unit = executor.compile("test", source).unwrap();

        let event = serde_json::json!({"value": 21});
        let rune_val = executor.json_to_rune_value(event).unwrap();
        let result = executor.call_function(&unit, "process", (rune_val,)).await.unwrap();
        assert_eq!(result, serde_json::json!(42));
    }

    #[tokio::test]
    async fn test_call_function_returns_none() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"pub fn maybe_none() { None }"#;
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "maybe_none", ()).await.unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn test_call_function_returns_some_object() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"
pub fn get_config() {
    Some(#{ name: "test", value: 42 })
}
"#;
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "get_config", ()).await.unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(result["value"], 42);
    }

    #[tokio::test]
    async fn test_call_async_function() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"
pub async fn async_compute() {
    42
}
"#;
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "async_compute", ()).await.unwrap();
        assert_eq!(result, serde_json::json!(42));
    }

    // =========================================================================
    // TDD: Tests for Option, Result, and iterator patterns
    // =========================================================================

    #[tokio::test]
    async fn test_option_some_returns_value() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"
pub fn maybe_value(x) {
    if x > 0 { Some(x * 2) } else { None }
}
"#;
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "maybe_value", (5i64,)).await.unwrap();
        assert_eq!(result, serde_json::json!(10));
    }

    #[tokio::test]
    async fn test_option_none_returns_null() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"
pub fn maybe_value(x) {
    if x > 0 { Some(x * 2) } else { None }
}
"#;
        let unit = executor.compile("test", source).unwrap();

        let result = executor.call_function(&unit, "maybe_value", (0i64,)).await.unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn test_vec_iter_map_collect() {
        let executor = RuneExecutor::new().unwrap();
        let source = r#"
pub fn double_all(nums) {
    nums.iter().map(|x| x * 2).collect::<Vec>()
}
"#;
        let unit = executor.compile("test", source).unwrap();

        let nums = executor.json_to_rune_value(serde_json::json!([1, 2, 3])).unwrap();
        let result = executor.call_function(&unit, "double_all", (nums,)).await.unwrap();
        assert_eq!(result, serde_json::json!([2, 4, 6]));
    }

    #[tokio::test]
    async fn test_vec_iter_filter_collect() {
        let executor = RuneExecutor::new().unwrap();
        // Rune filter syntax - no dereference needed
        let source = r#"
pub fn filter_positive(nums) {
    nums.iter().filter(|x| x > 0).collect::<Vec>()
}
"#;
        let unit = executor.compile("test", source).unwrap();

        let nums = executor.json_to_rune_value(serde_json::json!([-1, 2, -3, 4])).unwrap();
        let result = executor.call_function(&unit, "filter_positive", (nums,)).await.unwrap();
        assert_eq!(result, serde_json::json!([2, 4]));
    }

    #[tokio::test]
    async fn test_string_split_iteration() {
        let executor = RuneExecutor::new().unwrap();
        // In Rune, variables are mutable by default (no `mut` keyword)
        // Test basic split and iteration
        let source = r#"
pub fn count_words(text) {
    let words = text.split(' ');
    let count = 0;
    for word in words {
        count = count + 1;
    }
    count
}
"#;
        let unit = executor.compile("test", source).expect("compile failed");

        let text = executor.json_to_rune_value(serde_json::json!("hello world foo")).unwrap();
        let result = executor.call_function(&unit, "count_words", (text,)).await.unwrap();
        assert_eq!(result, serde_json::json!(3));
    }

    #[tokio::test]
    async fn test_string_concat_in_loop() {
        let executor = RuneExecutor::new().unwrap();
        // Test building string in loop
        let source = r#"
pub fn join_words(text) {
    let words = text.split(' ');
    let result = "";
    let first = true;
    for word in words {
        if !first {
            result = result + "-";
        }
        result = result + word;
        first = false;
    }
    result
}
"#;
        let unit = executor.compile("test", source).expect("compile failed");

        let text = executor.json_to_rune_value(serde_json::json!("hello world")).unwrap();
        let result = executor.call_function(&unit, "join_words", (text,)).await.unwrap();
        assert_eq!(result, serde_json::json!("hello-world"));
    }

    #[tokio::test]
    async fn test_iter_map_collect() {
        let executor = RuneExecutor::new().unwrap();
        // Test iter().map().collect() - join is done manually
        let source = r#"
pub fn double_to_strings(nums) {
    // Map numbers to strings
    let doubled = nums.iter().map(|x| (x * 2).to_string()).collect::<Vec>();
    // Manual join since Vec.join() isn't available
    let result = "";
    let first = true;
    for s in doubled {
        if !first {
            result = result + ", ";
        }
        result = result + s;
        first = false;
    }
    result
}
"#;
        let unit = executor.compile("test", source).expect("compile failed");

        let nums = executor.json_to_rune_value(serde_json::json!([1, 2, 3])).unwrap();
        let result = executor.call_function(&unit, "double_to_strings", (nums,)).await.unwrap();
        assert_eq!(result, serde_json::json!("2, 4, 6"));
    }
}
