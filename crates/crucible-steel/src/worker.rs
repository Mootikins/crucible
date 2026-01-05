//! Steel Engine worker thread
//!
//! Steel's Engine is !Send/!Sync, so we use a dedicated worker thread
//! with message passing to maintain a persistent engine instance.
//! This avoids the overhead of recreating the engine and replaying
//! all previous sources on every call.

use crate::error::SteelError;
use serde_json::Value as JsonValue;
use std::collections::HashMap as StdHashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use steel::gc::Gc;
use steel::rvals::SteelVal;
use steel::steel_vm::engine::Engine;
use tokio::sync::oneshot;

// Steel's HashMap type (im_rc::HashMap)
type SteelHashMap = steel::HashMap<SteelVal, SteelVal>;

/// Unique ID for compiled programs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompiledId(u64);

static NEXT_COMPILED_ID: AtomicU64 = AtomicU64::new(0);

impl CompiledId {
    fn next() -> Self {
        Self(NEXT_COMPILED_ID.fetch_add(1, Ordering::SeqCst))
    }
}

/// Commands sent to the Steel worker thread
enum WorkerCommand {
    /// Execute source code directly (for one-off executions)
    ExecuteSource {
        source: String,
        reply: oneshot::Sender<Result<JsonValue, SteelError>>,
    },

    /// Compile source and cache it, returns compiled ID
    Compile {
        source: String,
        reply: oneshot::Sender<Result<CompiledId, SteelError>>,
    },

    /// Run a previously compiled program
    RunCompiled {
        id: CompiledId,
        reply: oneshot::Sender<Result<JsonValue, SteelError>>,
    },

    /// Call a function by name with JSON arguments (converted on worker thread)
    CallFunction {
        name: String,
        args: Vec<JsonValue>,
        reply: oneshot::Sender<Result<JsonValue, SteelError>>,
    },

    /// Register a JSON value in the engine's environment
    RegisterValue {
        name: String,
        value: JsonValue,
        reply: oneshot::Sender<Result<(), SteelError>>,
    },

    /// Shutdown the worker
    Shutdown,
}

/// Handle to communicate with the Steel worker thread
pub struct SteelWorker {
    sender: mpsc::Sender<WorkerCommand>,
    _handle: JoinHandle<()>,
}

impl SteelWorker {
    /// Spawn a new Steel worker thread with a persistent engine
    pub fn spawn() -> Result<Self, SteelError> {
        let (tx, rx) = mpsc::channel::<WorkerCommand>();

        let handle = thread::Builder::new()
            .name("steel-worker".to_string())
            .spawn(move || {
                let mut engine = Engine::new();
                let mut compiled_cache: StdHashMap<CompiledId, String> = StdHashMap::new();

                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        WorkerCommand::ExecuteSource { source, reply } => {
                            let result = execute_source_impl(&mut engine, &source);
                            let _ = reply.send(result);
                        }

                        WorkerCommand::Compile { source, reply } => {
                            // Cache the source for later execution
                            // Steel doesn't have a simple "compile only" API that we can
                            // easily cache, but we can at least run it once to validate
                            // and define any functions, then cache the source for replays
                            let id = CompiledId::next();

                            // Execute to define functions/values
                            let result = engine.run(source.clone());
                            match result {
                                Ok(_) => {
                                    compiled_cache.insert(id, source);
                                    let _ = reply.send(Ok(id));
                                }
                                Err(e) => {
                                    let _ = reply.send(Err(SteelError::Compile(e.to_string())));
                                }
                            }
                        }

                        WorkerCommand::RunCompiled { id, reply } => {
                            let result = if let Some(source) = compiled_cache.get(&id) {
                                execute_source_impl(&mut engine, source)
                            } else {
                                Err(SteelError::Execution(format!(
                                    "Compiled program {:?} not found",
                                    id
                                )))
                            };
                            let _ = reply.send(result);
                        }

                        WorkerCommand::CallFunction { name, args, reply } => {
                            // Convert JSON args to SteelVal on worker thread
                            let steel_args: Vec<SteelVal> =
                                args.iter().map(json_to_steel).collect();
                            let result = call_function_impl(&mut engine, &name, steel_args);
                            let _ = reply.send(result);
                        }

                        WorkerCommand::RegisterValue { name, value, reply } => {
                            let steel_val = json_to_steel(&value);
                            engine.register_value(&name, steel_val);
                            let _ = reply.send(Ok(()));
                        }

                        WorkerCommand::Shutdown => {
                            break;
                        }
                    }
                }
            })
            .map_err(|e| SteelError::Engine(format!("Failed to spawn worker thread: {}", e)))?;

        Ok(Self {
            sender: tx,
            _handle: handle,
        })
    }

    /// Execute source code directly
    pub async fn execute_source(&self, source: &str) -> Result<JsonValue, SteelError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(WorkerCommand::ExecuteSource {
                source: source.to_string(),
                reply: tx,
            })
            .map_err(|_| SteelError::Engine("Worker thread disconnected".into()))?;

        rx.await
            .map_err(|_| SteelError::Engine("Worker response channel closed".into()))?
    }

    /// Compile source code and return an ID for later execution
    pub async fn compile(&self, source: &str) -> Result<CompiledId, SteelError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(WorkerCommand::Compile {
                source: source.to_string(),
                reply: tx,
            })
            .map_err(|_| SteelError::Engine("Worker thread disconnected".into()))?;

        rx.await
            .map_err(|_| SteelError::Engine("Worker response channel closed".into()))?
    }

    /// Run a previously compiled program
    pub async fn run_compiled(&self, id: CompiledId) -> Result<JsonValue, SteelError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(WorkerCommand::RunCompiled { id, reply: tx })
            .map_err(|_| SteelError::Engine("Worker thread disconnected".into()))?;

        rx.await
            .map_err(|_| SteelError::Engine("Worker response channel closed".into()))?
    }

    /// Call a function by name with JSON arguments
    pub async fn call_function(
        &self,
        name: &str,
        args: Vec<JsonValue>,
    ) -> Result<JsonValue, SteelError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(WorkerCommand::CallFunction {
                name: name.to_string(),
                args,
                reply: tx,
            })
            .map_err(|_| SteelError::Engine("Worker thread disconnected".into()))?;

        rx.await
            .map_err(|_| SteelError::Engine("Worker response channel closed".into()))?
    }

    /// Register a JSON value in the engine's environment
    pub async fn register_value(&self, name: &str, value: JsonValue) -> Result<(), SteelError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(WorkerCommand::RegisterValue {
                name: name.to_string(),
                value,
                reply: tx,
            })
            .map_err(|_| SteelError::Engine("Worker thread disconnected".into()))?;

        rx.await
            .map_err(|_| SteelError::Engine("Worker response channel closed".into()))?
    }

    /// Shutdown the worker thread
    pub fn shutdown(&self) {
        let _ = self.sender.send(WorkerCommand::Shutdown);
    }
}

impl Drop for SteelWorker {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Execute source code on the engine and return JSON result
fn execute_source_impl(engine: &mut Engine, source: &str) -> Result<JsonValue, SteelError> {
    let results = engine
        .run(source.to_string())
        .map_err(|e| SteelError::Execution(e.to_string()))?;

    if let Some(val) = results.last() {
        steel_to_json(val)
    } else {
        Ok(JsonValue::Null)
    }
}

/// Call a function by name with arguments
fn call_function_impl(
    engine: &mut Engine,
    name: &str,
    args: Vec<SteelVal>,
) -> Result<JsonValue, SteelError> {
    let result = engine
        .call_function_by_name_with_args(name, args)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("contract") || err_str.contains("Contract") {
                SteelError::Contract(err_str)
            } else {
                SteelError::Execution(err_str)
            }
        })?;

    steel_to_json(&result)
}

// =============================================================================
// JSON <-> SteelVal Conversion
// =============================================================================

/// Convert JSON to SteelVal directly (no text parsing)
pub fn json_to_steel(val: &JsonValue) -> SteelVal {
    match val {
        JsonValue::Null => SteelVal::Void,
        JsonValue::Bool(b) => SteelVal::BoolV(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                SteelVal::IntV(i as isize)
            } else if let Some(f) = n.as_f64() {
                SteelVal::NumV(f)
            } else {
                SteelVal::IntV(0)
            }
        }
        JsonValue::String(s) => SteelVal::StringV(s.clone().into()),
        JsonValue::Array(arr) => {
            let list: Vec<SteelVal> = arr.iter().map(json_to_steel).collect();
            SteelVal::ListV(list.into())
        }
        JsonValue::Object(obj) => {
            // Create hashmap with symbol keys (matching Steel's (hash 'key val) syntax)
            let map: SteelHashMap = obj
                .iter()
                .map(|(k, v)| (SteelVal::SymbolV(k.clone().into()), json_to_steel(v)))
                .collect();
            SteelVal::HashMapV(Gc::new(map).into())
        }
    }
}

/// Convert SteelVal to JSON
pub fn steel_to_json(val: &SteelVal) -> Result<JsonValue, SteelError> {
    match val {
        SteelVal::Void => Ok(JsonValue::Null),
        SteelVal::BoolV(b) => Ok(JsonValue::Bool(*b)),
        SteelVal::IntV(i) => Ok(JsonValue::Number((*i as i64).into())),
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
        other => Ok(JsonValue::String(format!("{:?}", other))),
    }
}
