//! Steel script executor
//!
//! Note: Steel's Engine is !Send/!Sync, so we use spawn_blocking
//! to run Steel code on a dedicated thread. Each operation creates
//! a fresh engine to maintain thread safety.

use crate::error::SteelError;
use serde_json::Value as JsonValue;
use steel::steel_vm::engine::Engine;
use steel::SteelVal;
use std::sync::Arc;
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
    pub async fn execute_source(&self, source: &str) -> Result<JsonValue, SteelError> {
        let source = source.to_string();
        let sources = self.sources.clone();

        // Store the source for later function calls
        {
            let mut s = sources.lock().await;
            s.push(source.clone());
        }

        // Run on blocking thread since Engine is !Send
        let result = tokio::task::spawn_blocking(move || {
            let mut engine = Engine::new();

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

        // Run on blocking thread
        let result = tokio::task::spawn_blocking(move || {
            let mut engine = Engine::new();

            // Replay all sources to define the functions
            // Clone each source to satisfy 'static lifetime requirement
            for source in all_sources {
                engine
                    .run(source)
                    .map_err(|e| SteelError::Execution(e.to_string()))?;
            }

            // Convert args to SteelVal
            let steel_args: Vec<SteelVal> = args
                .into_iter()
                .map(json_to_steel)
                .collect::<Result<Vec<_>, _>>()?;

            // Call the function
            let result = engine
                .call_function_by_name_with_args(&name, steel_args)
                .map_err(|e| {
                    let err_str = e.to_string();
                    if err_str.contains("contract") || err_str.contains("Contract") {
                        SteelError::Contract(err_str)
                    } else {
                        SteelError::Execution(err_str)
                    }
                })?;

            steel_to_json(&result)
        })
        .await
        .map_err(|e| SteelError::TaskJoin(e.to_string()))??;

        Ok(result)
    }
}

/// Convert SteelVal to JSON
fn steel_to_json(val: &SteelVal) -> Result<JsonValue, SteelError> {
    match val {
        SteelVal::Void => Ok(JsonValue::Null),
        SteelVal::BoolV(b) => Ok(JsonValue::Bool(*b)),
        SteelVal::IntV(i) => Ok(JsonValue::Number((*i).into())),
        SteelVal::NumV(f) => {
            serde_json::Number::from_f64(*f)
                .map(JsonValue::Number)
                .ok_or_else(|| SteelError::Conversion(format!("Invalid float: {}", f)))
        }
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

/// Convert JSON to SteelVal
fn json_to_steel(val: JsonValue) -> Result<SteelVal, SteelError> {
    match val {
        JsonValue::Null => Ok(SteelVal::Void),
        JsonValue::Bool(b) => Ok(SteelVal::BoolV(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(SteelVal::IntV(i as isize))
            } else if let Some(f) = n.as_f64() {
                Ok(SteelVal::NumV(f))
            } else {
                Err(SteelError::Conversion(format!("Invalid number: {}", n)))
            }
        }
        JsonValue::String(s) => Ok(SteelVal::StringV(s.into())),
        JsonValue::Array(arr) => {
            let list: Result<Vec<SteelVal>, _> = arr.into_iter().map(json_to_steel).collect();
            Ok(SteelVal::ListV(list?.into()))
        }
        JsonValue::Object(obj) => {
            // Build a list of key-value pairs for Steel
            // Steel hashmaps are typically constructed from lists of pairs
            let mut pairs = Vec::new();
            for (k, v) in obj {
                pairs.push(SteelVal::ListV(
                    vec![SteelVal::SymbolV(k.into()), json_to_steel(v)?].into(),
                ));
            }
            // Return as a list of pairs - Steel can convert this to a hash
            Ok(SteelVal::ListV(pairs.into()))
        }
    }
}
