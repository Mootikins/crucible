//! Core Rune tool definitions and execution
//!
//! This module provides the fundamental types and functionality for working with
//! Rune-based tools in the Crucible system.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use crate::types::*;
use rune::{Source, Unit, Vm};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

// Import ToolDependency from types module
use crate::types::ToolDependency;

/// Represents a single Rune-based tool
///
/// A RuneTool encapsulates a compiled Rune script that implements the tool interface.
/// Each tool must export:
/// - NAME: The tool's unique identifier
/// - DESCRIPTION: What the tool does
/// - INPUT_SCHEMA: JSON Schema for input validation
/// - call(args): Async function that executes the tool logic
#[derive(Debug, Clone)]
pub struct RuneTool {
    /// Unique identifier for this tool instance
    pub id: String,
    /// Tool name from metadata
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for input validation
    pub input_schema: Value,
    /// Optional JSON schema for output validation
    pub output_schema: Option<Value>,
    /// Tool category for organization
    pub category: String,
    /// Tags for discovery and filtering
    pub tags: Vec<String>,
    /// Source code of the tool
    pub source_code: String,
    /// File path if loaded from disk
    pub file_path: Option<PathBuf>,
    /// Compiled Rune unit
    unit: Arc<Unit>,
    /// Tool metadata
    pub metadata: ToolMetadata,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last modified timestamp
    pub modified_at: DateTime<Utc>,
    /// Tool version
    pub version: String,
    /// Author information
    pub author: Option<String>,
    /// Tool dependencies
    pub dependencies: Vec<ToolDependency>,
    /// Tool permissions
    pub permissions: Vec<String>,
    /// Whether the tool is enabled
    pub enabled: bool,
}

impl RuneTool {
    /// Create a new RuneTool from source code
    ///
    /// This compiles the Rune source and extracts the required metadata.
    /// The tool must export NAME, DESCRIPTION, INPUT_SCHEMA, and a call() function.
    pub fn from_source(
        source_code: &str,
        context: &rune::Context,
        metadata: Option<ToolMetadata>,
    ) -> Result<Self> {
        // Compile to unit
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

        // Extract metadata by creating a VM
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

        // Extract optional metadata
        let tool_metadata = metadata.unwrap_or_else(|| {
            // Try to extract additional metadata from the tool
            Self::extract_metadata_from_unit(unit.clone(), context).unwrap_or_default()
        });

        let now = Utc::now();

        let category = tool_metadata.category.clone().unwrap_or_else(|| "general".to_string());
        let tags = tool_metadata.tags.clone().unwrap_or_default();
        let version = tool_metadata.version.clone().unwrap_or_else(|| "1.0.0".to_string());
        let author = tool_metadata.author.clone();
        let dependencies = tool_metadata.dependencies.clone().unwrap_or_default();
        let permissions = tool_metadata.permissions.clone().unwrap_or_default();

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            input_schema,
            output_schema,
            category,
            tags,
            source_code: source_code.to_string(),
            file_path: None,
            unit,
            metadata: tool_metadata,
            created_at: now,
            modified_at: now,
            version,
            author,
            dependencies,
            permissions,
            enabled: true,
        })
    }

    /// Create a RuneTool from a file
    pub fn from_file<P: Into<PathBuf>>(
        path: P,
        context: &rune::Context,
    ) -> Result<Self> {
        let path_buf = path.into();
        let source_code = std::fs::read_to_string(&path_buf)
            .with_context(|| format!("Failed to read tool file: {:?}", path_buf))?;

        let mut tool = Self::from_source(&source_code, context, None)?;
        tool.file_path = Some(path_buf.clone());

        // Update modified time to match file
        if let Ok(metadata) = std::fs::metadata(&path_buf) {
            if let Ok(modified) = metadata.modified() {
                tool.modified_at = DateTime::from(modified);
            }
        }

        Ok(tool)
    }

    /// Execute the tool with the given arguments
    ///
    /// This creates a new VM instance and calls the tool's call() function.
    /// The function is async and returns a JSON value.
    pub async fn call(&self, args: Value, context: &rune::Context) -> Result<Value> {
        if !self.enabled {
            return Err(anyhow::anyhow!("Tool '{}' is disabled", self.name));
        }

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

        // Validate output if schema is provided
        if let Some(output_schema) = &self.output_schema {
            self.validate_output(&result, output_schema)?;
        }

        Ok(result)
    }

    /// Execute the tool with execution context
    pub async fn call_with_context(
        &self,
        args: Value,
        context: &rune::Context,
        execution_context: &ToolExecutionContext,
    ) -> Result<(ToolExecutionResult, crucible_services::types::tool::ContextRef)> {
        let start_time = std::time::Instant::now();

        // Create context reference for this execution
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("tool_name".to_string(), self.name.clone());
        metadata.insert("execution_id".to_string(), execution_context.execution_id.clone());

        // Extract user and session info from user_context if available
        if let Some(user_context) = &execution_context.user_context {
            if let Some(user_id) = user_context.get("user_id") {
                if let Some(user_id_str) = user_id.as_str() {
                    metadata.insert("user_id".to_string(), user_id_str.to_string());
                }
            }
            if let Some(session_id) = user_context.get("session_id") {
                if let Some(session_id_str) = session_id.as_str() {
                    metadata.insert("session_id".to_string(), session_id_str.to_string());
                }
            }
        }

        let context_ref = crucible_services::types::tool::ContextRef::with_metadata(metadata);
        let execution_duration = start_time.elapsed();

        let result = match self.call(args, context).await {
            Ok(output) => ToolExecutionResult {
                success: true,
                result: Some(output),
                error: None,
                execution_time: execution_duration,
                tool_name: self.name.clone(),
                context_ref: Some(context_ref.clone()),
            },
            Err(e) => ToolExecutionResult {
                success: false,
                result: None,
                error: Some(e.to_string()),
                execution_time: execution_duration,
                tool_name: self.name.clone(),
                context_ref: Some(context_ref.clone()),
            },
        };

        Ok((result, context_ref))
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

    /// Validate output against the tool's output schema
    pub fn validate_output(&self, output: &Value, schema: &Value) -> Result<()> {
        // TODO: Implement JSON schema validation
        // For now, just ensure output is valid JSON
        let _ = serde_json::to_string(output)?;
        Ok(())
    }

    /// Convert to ToolDefinition for service integration
    pub fn to_tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            category: Some(self.category.clone()),
            version: Some(self.version.clone()),
            author: self.author.clone(),
            tags: self.tags.clone(),
            enabled: self.enabled,
            parameters: vec![], // Parameters are defined in input_schema
        }
    }

    /// Reload the tool from its source file
    pub async fn reload(&mut self, context: &rune::Context) -> Result<()> {
        if let Some(file_path) = &self.file_path {
            let new_tool = Self::from_file(file_path, context)?;
            *self = new_tool;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Cannot reload tool: no file path available"))
        }
    }

    /// Check if the tool needs reloading based on file modification time
    pub async fn needs_reload(&self) -> Result<bool> {
        if let Some(file_path) = &self.file_path {
            if let Ok(metadata) = std::fs::metadata(file_path) {
                if let Ok(modified) = metadata.modified() {
                    let file_modified: chrono::DateTime<chrono::Utc> = DateTime::from(modified);
                    return Ok(file_modified > self.modified_at);
                }
            }
        }
        Ok(false)
    }

    /// Extract additional metadata from the compiled unit
    fn extract_metadata_from_unit(unit: Arc<Unit>, context: &rune::Context) -> Result<ToolMetadata> {
        let runtime = Arc::new(context.runtime()?);
        let mut metadata = ToolMetadata::default();

        // Try to extract VERSION
        let mut vm = Vm::new(runtime.clone(), unit.clone());
        if let Ok(version_value) = vm.call(["VERSION"], ()) {
            if let Ok(version) = rune::from_value::<String>(version_value) {
                metadata.version = Some(version);
            }
        }

        // Try to extract CATEGORY
        let mut vm = Vm::new(runtime.clone(), unit.clone());
        if let Ok(category_value) = vm.call(["CATEGORY"], ()) {
            if let Ok(category) = rune::from_value::<String>(category_value) {
                metadata.category = Some(category);
            }
        }

        // Try to extract TAGS
        let mut vm = Vm::new(runtime.clone(), unit.clone());
        if let Ok(tags_value) = vm.call(["TAGS"], ()) {
            if let Ok(tags) = rune::from_value::<Vec<String>>(tags_value) {
                metadata.tags = Some(tags);
            }
        }

        // Try to extract AUTHOR
        let mut vm = Vm::new(runtime, unit.clone());
        if let Ok(author_value) = vm.call(["AUTHOR"], ()) {
            if let Ok(author) = rune::from_value::<String>(author_value) {
                metadata.author = Some(author);
            }
        }

        Ok(metadata)
    }
}

/// Extended metadata for Rune tools
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolMetadata {
    /// Tool version
    pub version: Option<String>,
    /// Tool category
    pub category: Option<String>,
    /// Tool tags
    pub tags: Option<Vec<String>>,
    /// Tool author
    pub author: Option<String>,
    /// Tool dependencies
    pub dependencies: Option<Vec<ToolDependency>>,
    /// Tool permissions
    pub permissions: Option<Vec<String>>,
    /// Additional metadata
    pub additional: HashMap<String, Value>,
}

/// Tool execution configuration
#[derive(Debug, Clone)]
pub struct ToolExecutionConfig {
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: Option<u64>,
    /// Whether to capture stdout/stderr
    pub capture_output: bool,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Working directory
    pub working_directory: Option<PathBuf>,
}

impl Default for ToolExecutionConfig {
    fn default() -> Self {
        Self {
            timeout_ms: Some(30000), // 30 seconds default
            max_memory_bytes: Some(100 * 1024 * 1024), // 100MB default
            capture_output: true,
            environment: HashMap::new(),
            working_directory: None,
        }
    }
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
pub fn json_to_rune_value(value: &Value) -> Result<rune::Value> {
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
        let tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        assert_eq!(tool.name, "test_tool");
        assert_eq!(tool.description, "A test tool");
        assert!(tool.enabled);
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
        let tool = RuneTool::from_source(tool_source, &context, None).unwrap();

        let args = serde_json::json!({ "message": "Hello World" });
        let result = tool.call(args, &context).await.unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["message"], "Hello World");
    }

    #[test]
    fn test_value_conversion() {
        let json_value = serde_json::json!({
            "name": "test",
            "numbers": [1, 2, 3],
            "nested": { "key": "value" }
        });

        let rune_value = json_to_rune_value(&json_value).unwrap();
        let converted_back = rune_value_to_json(&rune_value).unwrap();

        assert_eq!(json_value, converted_back);
    }

    #[test]
    fn test_tool_definition_conversion() {
        let tool_source = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool" }
            pub fn CATEGORY() { "testing" }
            pub fn TAGS() { ["test", "demo"] }
            pub fn INPUT_SCHEMA() {
                #{ type: "object", properties: #{} }
            }
            pub async fn call(args) { #{ success: true } }
        "#;

        let context = rune::Context::with_default_modules().unwrap();
        let tool = RuneTool::from_source(tool_source, &context, None).unwrap();
        let definition = tool.to_tool_definition();

        assert_eq!(definition.name, "test_tool");
        assert_eq!(definition.category, "testing");
        assert!(definition.tags.contains(&"test".to_string()));
        assert!(definition.tags.contains(&"demo".to_string()));
    }
}