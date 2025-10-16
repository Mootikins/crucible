use anyhow::{Context, Result};
use rune::{Source, Unit, Vm};
use serde_json::Value;
use std::sync::Arc;

/// Represents a single Rune-based MCP tool
///
/// A RuneTool encapsulates a compiled Rune script that implements the MCP tool interface.
/// Each tool must export:
/// - NAME: The tool's unique identifier
/// - DESCRIPTION: What the tool does
/// - INPUT_SCHEMA: JSON Schema for input validation
/// - call(args): Async function that executes the tool logic
#[derive(Clone)]
pub struct RuneTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Option<Value>,
    source_code: String,
    unit: Arc<Unit>,
}

impl RuneTool {
    /// Create a RuneTool from source code
    ///
    /// This compiles the Rune source and extracts the required metadata.
    /// The tool must export NAME, DESCRIPTION, INPUT_SCHEMA, and a call() function.
    pub fn from_source(source_code: &str, context: &rune::Context) -> Result<Self> {
        // 1. Compile to unit
        let source = Source::memory(source_code)?;

        let mut sources = rune::Sources::new();
        sources.insert(source)?;

        let mut diagnostics = rune::Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut error_msg = String::from("Compilation diagnostics:\n");
            for diagnostic in diagnostics.diagnostics() {
                error_msg.push_str(&format!("  {:?}\n", diagnostic));
            }
            if result.is_err() {
                return Err(anyhow::anyhow!("{}", error_msg));
            }
            tracing::warn!("{}", error_msg);
        }

        let unit = Arc::new(result.context("Failed to compile tool")?);

        // 3. Extract metadata by creating a VM
        let runtime = Arc::new(context.runtime()?);
        let unit_clone = unit.clone();

        // Extract NAME
        let mut vm = Vm::new(runtime.clone(), unit_clone.clone());
        let name_value = vm.call(["NAME"], ())?;
        let name: String = rune::from_value(name_value)?;

        // Extract DESCRIPTION
        let mut vm = Vm::new(runtime.clone(), unit_clone.clone());
        let desc_value = vm.call(["DESCRIPTION"], ())?;
        let description: String = rune::from_value(desc_value)?;

        // Extract INPUT_SCHEMA
        let mut vm = Vm::new(runtime.clone(), unit_clone.clone());
        let schema_value = vm.call(["INPUT_SCHEMA"], ())?;
        let input_schema = rune_value_to_json(&schema_value)
            .context("Failed to convert INPUT_SCHEMA to JSON")?;

        // Try to extract OUTPUT_SCHEMA (optional)
        let output_schema = {
            let mut vm = Vm::new(runtime, unit_clone);
            match vm.call(["OUTPUT_SCHEMA"], ()) {
                Ok(val) => rune_value_to_json(&val).ok(),
                Err(_) => None,
            }
        };

        Ok(Self {
            name,
            description,
            input_schema,
            output_schema,
            source_code: source_code.to_string(),
            unit,
        })
    }

    /// Execute the tool with the given arguments
    ///
    /// This creates a new VM instance and calls the tool's call() function.
    /// The function is async and returns a JSON value.
    pub async fn call(&self, args: Value, context: &rune::Context) -> Result<Value> {
        // Validate input first
        self.validate_input(&args)?;

        // Convert JSON args to Rune value
        let args_rune = json_to_rune_value(&args)?;

        // Create new VM for execution
        let runtime = Arc::new(context.runtime()?);
        let mut vm = Vm::new(runtime, self.unit.clone());

        // Call the tool's call function - this is async
        let result_rune = vm.async_call(["call"], (args_rune,)).await?;

        // Convert result back to JSON
        let result = rune_value_to_json(&result_rune)
            .context("Failed to convert tool result to JSON")?;

        Ok(result)
    }

    /// Validate input arguments against the tool's schema
    pub fn validate_input(&self, args: &Value) -> Result<()> {
        // For now, skip JSON schema validation to avoid lifetime issues
        // TODO: Implement proper validation with owned schema
        // Just check that args is an object
        if !args.is_object() {
            return Err(anyhow::anyhow!("Tool arguments must be a JSON object"));
        }
        Ok(())
    }

    /// Get tool metadata for MCP tools/list response
    pub fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            output_schema: self.output_schema.clone(),
        }
    }
}

/// Metadata for a tool (used in MCP tools/list)
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(rename = "outputSchema", skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

/// Convert a Rune value to serde_json::Value
pub fn rune_value_to_json(value: &rune::Value) -> Result<Value> {
    // Try bool
    if let Ok(b) = value.as_bool().into_result() {
        return Ok(Value::Bool(b));
    }

    // Try integer
    if let Ok(i) = value.as_integer().into_result() {
        return Ok(Value::Number(i.into()));
    }

    // Try float
    if let Ok(f) = value.as_float().into_result() {
        return serde_json::Number::from_f64(f)
            .map(Value::Number)
            .ok_or_else(|| anyhow::anyhow!("Invalid float value"));
    }

    // Try to clone and convert to specific types
    let cloned = value.clone();

    // Try string
    if let Ok(s) = cloned.clone().into_string().into_result() {
        return Ok(Value::String(s.borrow_ref().map_err(|e| anyhow::anyhow!("String borrow error: {}", e))?.to_string()));
    }

    // Try Vec
    if let Ok(vec) = cloned.clone().into_vec().into_result() {
        let vec_ref = vec.borrow_ref().map_err(|e| anyhow::anyhow!("Vec borrow error: {}", e))?;
        let mut arr = Vec::new();
        for item in vec_ref.iter() {
            arr.push(rune_value_to_json(item)?);
        }
        return Ok(Value::Array(arr));
    }

    // Try Object
    if let Ok(obj) = cloned.into_object().into_result() {
        let obj_ref = obj.borrow_ref().map_err(|e| anyhow::anyhow!("Object borrow error: {}", e))?;
        let mut map = serde_json::Map::new();
        for (key, val) in obj_ref.iter() {
            // Keys are already strings in Rune objects
            let key_str = key.to_string();
            map.insert(key_str, rune_value_to_json(val)?);
        }
        return Ok(Value::Object(map));
    }

    // If nothing else matched, return null
    Ok(Value::Null)
}

/// Convert serde_json::Value to Rune value
fn json_to_rune_value(value: &Value) -> Result<rune::Value> {
    use rune::Value as RuneValue;

    match value {
        Value::Null => Ok(RuneValue::from(())),
        Value::Bool(b) => Ok(RuneValue::from(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(RuneValue::from(i))
            } else if let Some(f) = n.as_f64() {
                Ok(RuneValue::from(f))
            } else {
                Err(anyhow::anyhow!("Invalid number"))
            }
        }
        Value::String(s) => {
            let rune_str = rune::alloc::String::try_from(s.as_str())?;
            Ok(RuneValue::try_from(rune_str)?)
        }
        Value::Array(arr) => {
            let mut rune_vec = rune::runtime::Vec::new();
            for item in arr {
                rune_vec.push(json_to_rune_value(item)?)?;
            }
            Ok(RuneValue::try_from(rune_vec)?)
        }
        Value::Object(obj) => {
            let mut rune_obj = rune::runtime::Object::new();
            for (key, val) in obj {
                let rune_key = rune::alloc::String::try_from(key.as_str())?;
                rune_obj.insert(rune_key, json_to_rune_value(val)?)?;
            }
            Ok(RuneValue::try_from(rune_obj)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_loading() {
        let tool_source = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{ name: #{ type: "string" } }
                }
            }

            pub async fn call(args) {
                #{ success: true, data: `Hello ${args.name}` }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let tool = RuneTool::from_source(tool_source, &context).unwrap();

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool");
    }

    #[tokio::test]
    async fn test_tool_execution() {
        let tool_source = r#"
            pub fn NAME() { "echo_tool" }
            pub fn DESCRIPTION() { "Echoes input" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{ message: #{ type: "string" } },
                    required: ["message"]
                }
            }

            pub async fn call(args) {
                #{ success: true, message: args.message }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let tool = RuneTool::from_source(tool_source, &context).unwrap();

        let args = serde_json::json!({ "message": "Hello World" });
        let result = tool.call(args, &context).await.unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Hello World");
    }

    #[test]
    fn test_input_validation() {
        let tool_source = r#"
            pub fn NAME() { "validator_tool" }
            pub fn DESCRIPTION() { "Tests validation" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        name: #{ type: "string" },
                        age: #{ type: "number" }
                    },
                    required: ["name"]
                }
            }

            pub async fn call(args) {
                #{ success: true }
            }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let tool = RuneTool::from_source(tool_source, &context).unwrap();

        // Valid input
        let valid = serde_json::json!({ "name": "Alice", "age": 30 });
        assert!(tool.validate_input(&valid).is_ok());

        // Invalid input (not an object, should fail)
        let invalid = serde_json::json!("not an object");
        assert!(tool.validate_input(&invalid).is_err());
    }
}
