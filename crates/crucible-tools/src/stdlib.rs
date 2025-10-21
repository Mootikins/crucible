//! Standard library extensions for Rune
//!
//! This module provides standard library functions and modules for Rune scripts
//! used as tools in the Crucible system.

use crate::errors::RuneResult;
use crate::types::ToolResult;
use rune::runtime::{Value, Shared};
use rune::{Context, Module};

// === Memory Management Convenience Functions ===

/// Create a shared string value with proper error handling
pub fn create_shared_string(s: rune::alloc::String) -> ToolResult<Shared<rune::alloc::String>> {
    Shared::new(s).map_err(|e| anyhow::anyhow!("Failed to create shared string: {}", e))
}

/// Create a shared vec value with proper error handling
pub fn create_shared_vec(v: rune::runtime::Vec) -> ToolResult<Shared<rune::runtime::Vec>> {
    Shared::new(v).map_err(|e| anyhow::anyhow!("Failed to create shared vec: {}", e))
}

/// Create a shared object value with proper error handling
pub fn create_shared_obj(o: rune::runtime::Object) -> ToolResult<Shared<rune::runtime::Object>> {
    Shared::new(o).map_err(|e| anyhow::anyhow!("Failed to create shared object: {}", e))
}

/// Build the Crucible standard library module
pub fn build_crucible_module() -> RuneResult<rune::Module> {
    let mut module = rune::Module::with_crate("crucible")?;

    // Install core functions
    install_http_functions(&mut module)?;
    install_file_functions(&mut module)?;
    install_json_functions(&mut module)?;
    install_string_functions(&mut module)?;
    install_time_functions(&mut module)?;
    install_math_functions(&mut module)?;
    install_validation_functions(&mut module)?;

    Ok(module)
}

/// Install HTTP-related functions
fn install_http_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function_meta(http_get)?;
    module.function_meta(http_post)?;
    module.function_meta(http_json_get)?;

    Ok(())
}

/// HTTP GET function for Rune
#[rune::function]
pub async fn http_get(url: String) -> ToolResult<Value> {
    match reqwest::get(&url).await {
        Ok(response) => {
            match response.text().await {
                Ok(text) => {
                    let rune_str = rune::alloc::String::try_from(text.as_str())
                        .map_err(|e| anyhow::anyhow!("String conversion failed: {}", e))?;
                    let shared_str = create_shared_string(rune_str)?;
                    Ok(Value::String(shared_str))
                },
                Err(e) => {
                    Err(anyhow::anyhow!("HTTP response error: {}", e))
                },
            }
        }
        Err(e) => {
            Err(anyhow::anyhow!("HTTP request error: {}", e))
        },
    }
}

/// HTTP POST function for Rune
#[rune::function]
async fn http_post(url: String, body: String) -> ToolResult<Value> {
    let client = reqwest::Client::new();
    match client.post(&url).body(body).send().await {
        Ok(response) => {
            match response.text().await {
                Ok(text) => {
                    let rune_str = rune::alloc::String::try_from(text.as_str())
                        .map_err(|e| anyhow::anyhow!("String conversion failed: {}", e))?;
                    let shared_str = create_shared_string(rune_str)?;
                    Ok(Value::String(shared_str))
                },
                Err(e) => {
                    Err(anyhow::anyhow!("HTTP response error: {}", e))
                }
            }
        }
        Err(e) => {
            Err(anyhow::anyhow!("HTTP request error: {}", e))
        }
    }
}

/// HTTP JSON GET function for Rune
#[rune::function]
async fn http_json_get(url: String) -> ToolResult<Value> {
    match reqwest::get(&url).await {
        Ok(response) => {
            match response.json::<serde_json::Value>().await {
                Ok(json) => Ok(rune_value_from_json(&json)?),
                Err(e) => {
                    Err(anyhow::anyhow!("JSON parsing error: {}", e))
                }
            }
        }
        Err(e) => {
            Err(anyhow::anyhow!("HTTP request error: {}", e))
        }
    }
}

/// Install file system functions
fn install_file_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function_meta(file_read)?;
    module.function_meta(file_write)?;
    module.function_meta(file_exists)?;
    module.function_meta(file_is_dir)?;
    module.function_meta(file_is_file)?;
    module.function_meta(file_list_dir)?;

    Ok(())
}

/// Create a basic Rune context with default modules
pub fn create_context() -> RuneResult<Context> {
    let context = Context::with_default_modules()?;
    Ok(context)
}

/// Create a Rune context with additional custom modules
pub fn create_context_with_modules(modules: Vec<Module>) -> RuneResult<Context> {
    let mut context = Context::with_default_modules()?;

    for module in modules {
        context.install(module)?;
    }

    Ok(context)
}

/// Create a default context with crucible module pre-installed
pub fn create_default_context() -> RuneResult<Context> {
    let mut context = Context::with_default_modules()?;
    let crucible_module = build_crucible_module()?;
    context.install(crucible_module)?;
    Ok(context)
}

// === Helper Functions and Implementations ===

/// Convert JSON value to Rune value
fn rune_value_from_json(json: &serde_json::Value) -> ToolResult<Value> {
    match json {
        serde_json::Value::Null => {
            // Create empty tuple for unit value
            let empty_vec = rune::runtime::Vec::new();
            let shared_vec = create_shared_vec(empty_vec)?;
            Ok(Value::Vec(shared_vec))
        },
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(u) = n.as_u64() {
                Ok(Value::Integer(u as i64))  // Convert u64 to i64
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Float(f))
            } else {
                Err(anyhow::anyhow!("Invalid number format"))
            }
        }
        serde_json::Value::String(s) => {
            let rune_str = rune::alloc::String::try_from(s.as_str())
                .map_err(|e| anyhow::anyhow!("String conversion failed: {}", e))?;
            let shared_str = create_shared_string(rune_str)?;
            Ok(Value::String(shared_str))
        },
        serde_json::Value::Array(arr) => {
            let mut rune_vec = rune::runtime::Vec::new();
            for item in arr {
                rune_vec.push(rune_value_from_json(item)?)?;
            }
            let shared_vec = create_shared_vec(rune_vec)?;
            Ok(Value::Vec(shared_vec))
        }
        serde_json::Value::Object(obj) => {
            let mut rune_obj = rune::runtime::Object::new();
            for (key, value) in obj {
                let rune_key = rune::alloc::String::try_from(key.as_str())
                    .map_err(|e| anyhow::anyhow!("String conversion failed: {}", e))?;
                rune_obj.insert(rune_key, rune_value_from_json(value)?)?;
            }
            let shared_obj = create_shared_obj(rune_obj)?;
            Ok(Value::Object(shared_obj))
        }
    }
}

/// Install JSON-related functions
fn install_json_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function_meta(json_parse)?;
    module.function_meta(json_stringify)?;
    Ok(())
}

/// Install string manipulation functions
fn install_string_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function_meta(string_split)?;
    module.function_meta(string_join)?;
    module.function_meta(string_trim)?;
    Ok(())
}

/// Install time-related functions
fn install_time_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function_meta(time_now)?;
    module.function_meta(time_format)?;
    Ok(())
}

/// Install math functions
fn install_math_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function_meta(math_abs)?;
    module.function_meta(math_round)?;
    Ok(())
}

/// Install validation functions
fn install_validation_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function_meta(validate_email)?;
    module.function_meta(validate_url)?;
    Ok(())
}

// === Function Implementations ===

#[rune::function]
async fn json_parse(input: String) -> ToolResult<Value> {
    match serde_json::from_str::<serde_json::Value>(&input) {
        Ok(json) => rune_value_from_json(&json),
        Err(e) => {
            Err(anyhow::anyhow!("JSON parse error: {}", e))
        }
    }
}

#[rune::function]
async fn json_stringify(value: Value) -> ToolResult<String> {
    // Convert Rune value back to JSON
    match value {
        Value::Vec(_) | Value::Tuple(_) => Ok("null".to_string()), // Empty values
        Value::Bool(b) => Ok(serde_json::to_string(&b).unwrap_or_default()),
        Value::Byte(b) => Ok(b.to_string()),
        Value::Char(c) => Ok(format!("\"{}\"", c)),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(f.to_string()),
        Value::String(s) => Ok(format!("\"{}\"", format_args!("{:?}", s))), // Use Debug for Shared<String>
        _ => Ok("\"[complex value]\"".to_string()),
    }
}

#[rune::function]
async fn string_split(input: String, delimiter: String) -> ToolResult<Value> {
    let parts: Vec<String> = input.split(&delimiter).map(|s| s.to_string()).collect();
    let mut rune_vec = rune::runtime::Vec::new();
    for part in parts {
        let rune_str = rune::alloc::String::try_from(part.as_str())
            .map_err(|e| anyhow::anyhow!("String conversion failed: {}", e))?;
        let shared_str = create_shared_string(rune_str)?;
        rune_vec.push(Value::String(shared_str))?;
    }
    let shared_vec = create_shared_vec(rune_vec)?;
    Ok(Value::Vec(shared_vec))
}

#[rune::function]
async fn string_join(parts: Vec<String>, delimiter: String) -> ToolResult<String> {
    Ok(parts.join(&delimiter))
}

#[rune::function]
async fn string_trim(input: String) -> ToolResult<String> {
    Ok(input.trim().to_string())
}

#[rune::function]
async fn time_now() -> ToolResult<String> {
    Ok(chrono::Utc::now().to_rfc3339())
}

#[rune::function]
async fn time_format(timestamp: String, format: String) -> ToolResult<String> {
    match chrono::DateTime::parse_from_rfc3339(&timestamp) {
        Ok(dt) => Ok(dt.format(&format).to_string()),
        Err(e) => {
            Err(anyhow::anyhow!("Time format error: {}", e))
        }
    }
}

#[rune::function]
async fn math_abs(num: i64) -> ToolResult<i64> {
    Ok(num.abs())
}

#[rune::function]
async fn math_round(num: f64) -> ToolResult<i64> {
    Ok(num.round() as i64)
}

#[rune::function]
async fn validate_email(email: String) -> ToolResult<bool> {
    // Simple email validation
    Ok(email.contains('@') && email.contains('.'))
}

#[rune::function]
async fn validate_url(url: String) -> ToolResult<bool> {
    // Simple URL validation
    Ok(url.starts_with("http://") || url.starts_with("https://"))
}

// Placeholder file function implementations
#[rune::function]
async fn file_read(path: String) -> ToolResult<Value> {
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let rune_str = rune::alloc::String::try_from(content.as_str())
                .map_err(|e| anyhow::anyhow!("String conversion failed: {}", e))?;
            let shared_str = create_shared_string(rune_str)?;
            Ok(Value::String(shared_str))
        },
        Err(e) => {
            Err(anyhow::anyhow!("File read error: {}", e))
        }
    }
}

#[rune::function]
async fn file_write(path: String, content: String) -> ToolResult<()> {
    match std::fs::write(&path, content) {
        Ok(_) => Ok(()),
        Err(e) => {
            Err(anyhow::anyhow!("File write error: {}", e))
        }
    }
}

#[rune::function]
async fn file_exists(path: String) -> ToolResult<bool> {
    Ok(std::path::Path::new(&path).exists())
}

#[rune::function]
async fn file_is_dir(path: String) -> ToolResult<bool> {
    Ok(std::path::Path::new(&path).is_dir())
}

#[rune::function]
async fn file_is_file(path: String) -> ToolResult<bool> {
    Ok(std::path::Path::new(&path).is_file())
}

#[rune::function]
async fn file_list_dir(path: String) -> ToolResult<Value> {
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            let mut rune_vec = rune::runtime::Vec::new();
            for entry in entries.take(100) { // Limit to prevent excessive results
                if let Ok(entry) = entry {
                    if let Some(name_str) = entry.file_name().to_str() {
                        let rune_str = rune::alloc::String::try_from(name_str)
                            .map_err(|e| anyhow::anyhow!("String conversion failed: {}", e))?;
                        let shared_str = create_shared_string(rune_str)?;
                        rune_vec.push(Value::String(shared_str))?;
                    }
                }
            }
            let shared_vec = create_shared_vec(rune_vec)?;
            Ok(Value::Vec(shared_vec))
        }
        Err(e) => {
            Err(anyhow::anyhow!("Directory read error: {}", e))
        }
    }
}