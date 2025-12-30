//! Steel script executor
//!
//! Note: Steel's Engine is !Send/!Sync, so we use spawn_blocking
//! to run Steel code on a dedicated thread. Each operation creates
//! a fresh engine to maintain thread safety.

use crate::error::SteelError;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use steel::steel_vm::engine::Engine;
use steel::SteelVal;
use tokio::sync::Mutex;

/// Steel script executor
///
/// Wraps Steel's Engine with async-safe execution via spawn_blocking.
/// Since Steel engines are !Send, each async operation runs on a blocking thread.
pub struct SteelExecutor {
    /// Accumulated source code to replay when calling functions
    sources: Arc<Mutex<Vec<String>>>,
}

impl SteelExecutor {
    /// Create a new Steel executor
    pub fn new() -> Result<Self, SteelError> {
        // Verify Steel can create an engine (validates the dependency works)
        std::thread::spawn(|| {
            let _engine = Engine::new();
        })
        .join()
        .map_err(|_| SteelError::Engine("Failed to create Steel engine".into()))?;

        Ok(Self {
            sources: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Execute Steel source code and return the result
    ///
    /// Previous sources are replayed to preserve definitions from earlier calls.
    pub async fn execute_source(&self, source: &str) -> Result<JsonValue, SteelError> {
        let source = source.to_string();
        let sources = self.sources.clone();

        // Get accumulated sources first
        let all_sources: Vec<String> = {
            let s = sources.lock().await;
            s.clone()
        };

        // Store the new source for later calls
        {
            let mut s = sources.lock().await;
            s.push(source.clone());
        }

        // Run on blocking thread since Engine is !Send
        let result = tokio::task::spawn_blocking(move || {
            let mut engine = Engine::new();

            // Replay all previous sources to preserve definitions
            for prev_source in all_sources {
                engine
                    .run(prev_source)
                    .map_err(|e| SteelError::Execution(e.to_string()))?;
            }

            // run() requires Into<Cow<'static, str>>, so pass owned String
            let results = engine
                .run(source)
                .map_err(|e| SteelError::Execution(e.to_string()))?;

            // Return the last result, or null if empty
            if let Some(val) = results.last() {
                steel_to_json(val)
            } else {
                Ok(JsonValue::Null)
            }
        })
        .await
        .map_err(|e| SteelError::TaskJoin(e.to_string()))??;

        Ok(result)
    }

    /// Call a previously defined function with JSON arguments
    ///
    /// JSON objects are converted to Steel hashmaps using `(hash ...)` syntax.
    pub async fn call_function(
        &self,
        name: &str,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, SteelError> {
        let name = name.to_string();
        let sources = self.sources.clone();

        // Get accumulated sources
        let all_sources: Vec<String> = {
            let s = sources.lock().await;
            s.clone()
        };

        // Convert args to Steel code representation
        let arg_exprs: Vec<String> = args.into_iter().map(json_to_steel_code).collect();

        // Build the function call as code: (func-name arg1 arg2 ...)
        let call_code = format!("({} {})", name, arg_exprs.join(" "));

        // Run on blocking thread
        let result = tokio::task::spawn_blocking(move || {
            let mut engine = Engine::new();

            // Replay all sources to define the functions
            for source in all_sources {
                engine
                    .run(source)
                    .map_err(|e| SteelError::Execution(e.to_string()))?;
            }

            // Execute the function call
            let results = engine.run(call_code).map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("contract") || err_str.contains("Contract") {
                    SteelError::Contract(err_str)
                } else {
                    SteelError::Execution(err_str)
                }
            })?;

            // Return the result
            if let Some(val) = results.last() {
                steel_to_json(val)
            } else {
                Ok(JsonValue::Null)
            }
        })
        .await
        .map_err(|e| SteelError::TaskJoin(e.to_string()))??;

        Ok(result)
    }
}

/// Convert JSON value to Steel code string
///
/// This generates Steel source code that, when evaluated, produces the equivalent value.
/// Objects become `(hash ...)` expressions, arrays become `(list ...)`, etc.
fn json_to_steel_code(val: JsonValue) -> String {
    match val {
        JsonValue::Null => "#f".to_string(), // void isn't directly expressible, use #f
        JsonValue::Bool(b) => if b { "#t" } else { "#f" }.to_string(),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                format!("{}", f)
            } else {
                "0".to_string()
            }
        }
        JsonValue::String(s) => format!("\"{}\"", escape_steel_string(&s)),
        JsonValue::Array(arr) => {
            let elements: Vec<String> = arr.into_iter().map(json_to_steel_code).collect();
            format!("(list {})", elements.join(" "))
        }
        JsonValue::Object(obj) => {
            // Convert to (hash 'key1 val1 'key2 val2 ...)
            let pairs: Vec<String> = obj
                .into_iter()
                .map(|(k, v)| format!("'{} {}", k, json_to_steel_code(v)))
                .collect();
            format!("(hash {})", pairs.join(" "))
        }
    }
}

/// Escape special characters in a Steel string
fn escape_steel_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Convert SteelVal to JSON
fn steel_to_json(val: &SteelVal) -> Result<JsonValue, SteelError> {
    match val {
        SteelVal::Void => Ok(JsonValue::Null),
        SteelVal::BoolV(b) => Ok(JsonValue::Bool(*b)),
        SteelVal::IntV(i) => Ok(JsonValue::Number((*i).into())),
        SteelVal::NumV(f) => serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .ok_or_else(|| SteelError::Conversion(format!("Invalid float: {}", f))),
        SteelVal::StringV(s) => Ok(JsonValue::String(s.to_string())),
        SteelVal::ListV(list) => {
            let arr: Result<Vec<JsonValue>, _> = list.iter().map(steel_to_json).collect();
            Ok(JsonValue::Array(arr?))
        }
        SteelVal::VectorV(vec) => {
            let arr: Result<Vec<JsonValue>, _> = vec.iter().map(steel_to_json).collect();
            Ok(JsonValue::Array(arr?))
        }
        SteelVal::HashMapV(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map.iter() {
                let key = match k {
                    SteelVal::StringV(s) => s.to_string(),
                    SteelVal::SymbolV(s) => s.to_string(),
                    _ => return Err(SteelError::Conversion(format!("Non-string key: {:?}", k))),
                };
                obj.insert(key, steel_to_json(v)?);
            }
            Ok(JsonValue::Object(obj))
        }
        SteelVal::SymbolV(s) => Ok(JsonValue::String(s.to_string())),
        SteelVal::CharV(c) => Ok(JsonValue::String(c.to_string())),
        // For other types, return a string representation
        other => Ok(JsonValue::String(format!("{:?}", other))),
    }
}
