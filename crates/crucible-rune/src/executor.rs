//! Execute Rune scripts

use crate::types::{RuneExecutionResult, RuneTool};
use crate::RuneError;
use rune::runtime::{RuntimeContext, ToValue, VmError};
use rune::{Context, Diagnostics, Source, Sources, Unit, Value, Vm};
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

        let runtime = Arc::new(context.runtime().map_err(|e| RuneError::Context(e.to_string()))?);

        Ok(Self {
            context: Arc::new(context),
            runtime,
        })
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

    /// Compile a Rune script
    fn compile(&self, name: &str, source: &str) -> Result<Arc<Unit>, RuneError> {
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

        let unit = result.map_err(|e| RuneError::Compile(e.to_string()))?;

        Ok(Arc::new(unit))
    }

    /// Run a compiled unit
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

        // Execute the function
        let output = if rune_args.is_empty() {
            vm.call(hash, ())
                .map_err(|e| RuneError::Execution(format_vm_error(e)))?
        } else {
            // For now, pass args as a single tuple if there's more than one
            vm.call(hash, (rune_args,))
                .map_err(|e| RuneError::Execution(format_vm_error(e)))?
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

    /// Convert a single JSON value to Rune
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
    fn rune_to_json(&self, value: Value) -> Result<JsonValue, RuneError> {
        // Fallback to debug representation for now
        // A more complete implementation would inspect the value type
        Ok(JsonValue::String(format!("{:?}", value)))
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
}
