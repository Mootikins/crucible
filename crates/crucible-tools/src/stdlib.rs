//! Standard library extensions for Rune
//!
//! This module provides standard library functions and modules for Rune scripts
//! used as tools in the Crucible system.

use crate::errors::{RuneError, RuneResult};
use rune::runtime::VmError;
use rune::{Context, Value};
use std::sync::Arc;

/// Build the Crucible standard library module
pub fn build_crucible_module() -> RuneResult<rune::Module> {
    let mut module = rune::Module::with_crate_item("crucible", []);

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
    module.function(["http", "get"]?, http_get)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register http_get function: {}", e),
            source: None,
        })?;
    module.function(["http", "post"]?, http_post)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register http_post function: {}", e),
            source: None,
        })?;
    module.function(["http", "json_get"]?, http_json_get)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register http_json_get function: {}", e),
            source: None,
        })?;

    Ok(())
}

/// HTTP GET function for Rune
async fn http_get(url: String) -> Result<Value, VmError> {
    match reqwest::get(&url).await {
        Ok(response) => {
            match response.text().await {
                Ok(text) => Ok(Value::from(text)),
                Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP response error: {}", e))),
            }
        }
        Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP request error: {}", e))),
    }
}

/// HTTP POST function for Rune
async fn http_post(url: String, body: String) -> Result<Value, VmError> {
    let client = reqwest::Client::new();
    match client.post(&url).body(body).send().await {
        Ok(response) => {
            match response.text().await {
                Ok(text) => Ok(Value::from(text)),
                Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP response error: {}", e))),
            }
        }
        Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP request error: {}", e))),
    }
}

/// HTTP JSON GET function for Rune
async fn http_json_get(url: String) -> Result<Value, VmError> {
    match reqwest::get(&url).await {
        Ok(response) => {
            match response.json::<serde_json::Value>().await {
                Ok(json) => Ok(rune_value_from_json(&json)?),
                Err(e) => Err(VmError::from(anyhow::anyhow!("JSON parsing error: {}", e))),
            }
        }
        Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP request error: {}", e))),
    }
}

/// Install file system functions
fn install_file_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function(["file", "read"]?, file_read)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register file_read function: {}", e),
            source: None,
        })?;
    module.function(["file", "write"]?, file_write)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register file_write function: {}", e),
            source: None,
        })?;
    module.function(["file", "exists"]?, file_exists)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register file_exists function: {}", e),
            source: None,
        })?;
    module.function(["file", "is_dir"]?, file_is_dir)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register file_is_dir function: {}", e),
            source: None,
        })?;
    module.function(["file", "is_file"]?, file_is_file)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register file_is_file function: {}", e),
            source: None,
        })?;
    module.function(["file", "list_dir"]?, file_list_dir)
        .map_err(|e| RuneError::CompilationError {
            message: format!("Failed to register file_list_dir function: {}", e),
            source: None,
        })?;

    Ok(())
}

/// File read function for Rune
fn file_read(path: String) -> Result<String, VmError> {
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(content),
        Err(e) => Err(VmError::from(anyhow::anyhow!("File read error: {}", e))),
    }
}

/// File write function for Rune
fn file_write(path: String, content: String) -> Result<(), VmError> {
    match std::fs::write(&path, content) {
        Ok(_) => Ok(()),
        Err(e) => Err(VmError::from(anyhow::anyhow!("File write error: {}", e))),
    }
}

/// File exists function for Rune
fn file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

/// File is directory function for Rune
fn file_is_dir(path: String) -> bool {
    std::path::Path::new(&path).is_dir()
}

/// File is file function for Rune
fn file_is_file(path: String) -> bool {
    std::path::Path::new(&path).is_file()
}

/// File list directory function for Rune
fn file_list_dir(path: String) -> Result<Vec<String>, VmError> {
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            let mut files = Vec::new();
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        if let Some(name) = entry.file_name().to_str() {
                            files.push(name.to_string());
                        }
                    }
                    Err(e) => return Err(VmError::from(anyhow::anyhow!("Directory read error: {}", e))),
                }
            }
            Ok(files)
        }
        Err(e) => Err(VmError::from(anyhow::anyhow!("Directory read error: {}", e))),
    }
}

/// Install JSON manipulation functions
fn install_json_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function(["json", "parse"]?, json_parse)?;
    module.function(["json", "stringify"]?, json_stringify)?;
    module.function(["json", "stringify_pretty"]?, json_stringify_pretty)?;

    Ok(())
}

/// JSON parse function for Rune
fn json_parse(text: String) -> Result<Value, VmError> {
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(json) => Ok(rune_value_from_json(&json)?),
        Err(e) => Err(VmError::from(anyhow::anyhow!("JSON parsing error: {}", e))),
    }
}

/// JSON stringify function for Rune
fn json_stringify(value: Value) -> Result<String, VmError> {
    let json = json_value_from_rune(&value)?;
    match serde_json::to_string(&json) {
        Ok(s) => Ok(s),
        Err(e) => Err(VmError::from(anyhow::anyhow!("JSON serialization error: {}", e))),
    }
}

/// JSON stringify pretty function for Rune
fn json_stringify_pretty(value: Value) -> Result<String, VmError> {
    let json = json_value_from_rune(&value)?;
    match serde_json::to_string_pretty(&json) {
        Ok(s) => Ok(s),
        Err(e) => Err(VmError::from(anyhow::anyhow!("JSON serialization error: {}", e))),
    }
}

/// Install string manipulation functions
fn install_string_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function(["string", "trim"]?, string_trim)?;
    module.function(["string", "to_upper"]?, string_to_upper)?;
    module.function(["string", "to_lower"]?, string_to_lower)?;
    module.function(["string", "contains"]?, string_contains)?;
    module.function(["string", "starts_with"]?, string_starts_with)?;
    module.function(["string", "ends_with"]?, string_ends_with)?;
    module.function(["string", "split"]?, string_split)?;
    module.function(["string", "join"]?, string_join)?;
    module.function(["string", "replace"]?, string_replace)?;

    Ok(())
}

/// String trim function for Rune
fn string_trim(s: String) -> String {
    s.trim().to_string()
}

/// String to uppercase function for Rune
fn string_to_upper(s: String) -> String {
    s.to_uppercase()
}

/// String to lowercase function for Rune
fn string_to_lower(s: String) -> String {
    s.to_lowercase()
}

/// String contains function for Rune
fn string_contains(s: String, pattern: String) -> bool {
    s.contains(&pattern)
}

/// String starts with function for Rune
fn string_starts_with(s: String, prefix: String) -> bool {
    s.starts_with(&prefix)
}

/// String ends with function for Rune
fn string_ends_with(s: String, suffix: String) -> bool {
    s.ends_with(&suffix)
}

/// String split function for Rune
fn string_split(s: String, delimiter: String) -> Vec<String> {
    s.split(&delimiter).map(|s| s.to_string()).collect()
}

/// String join function for Rune
fn string_join(parts: Vec<String>, delimiter: String) -> String {
    parts.join(&delimiter)
}

/// String replace function for Rune
fn string_replace(s: String, from: String, to: String) -> String {
    s.replace(&from, &to)
}

/// Install time-related functions
fn install_time_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function(["time", "now"]?, time_now)?;
    module.function(["time", "timestamp"]?, time_timestamp)?;
    module.function(["time", "parse"]?, time_parse)?;
    module.function(["time", "format"]?, time_format)?;

    Ok(())
}

/// Time now function for Rune
fn time_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Time timestamp function for Rune
fn time_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Time parse function for Rune
fn time_parse(s: String) -> Result<String, VmError> {
    match chrono::DateTime::parse_from_rfc3339(&s) {
        Ok(dt) => Ok(dt.to_rfc3339()),
        Err(e) => Err(VmError::from(anyhow::anyhow!("Time parsing error: {}", e))),
    }
}

/// Time format function for Rune
fn time_format(s: String, format: String) -> Result<String, VmError> {
    match chrono::DateTime::parse_from_rfc3339(&s) {
        Ok(dt) => Ok(dt.format(&format).to_string()),
        Err(e) => Err(VmError::from(anyhow::anyhow!("Time parsing error: {}", e))),
    }
}

/// Install math functions
fn install_math_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function(["math", "abs"]?, math_abs)?;
    module.function(["math", "round"]?, math_round)?;
    module.function(["math", "floor"]?, math_floor)?;
    module.function(["math", "ceil"]?, math_ceil)?;
    module.function(["math", "sqrt"]?, math_sqrt)?;
    module.function(["math", "pow"]?, math_pow)?;
    module.function(["math", "min"]?, math_min)?;
    module.function(["math", "max"]?, math_max)?;
    module.function(["math", "sin"]?, math_sin)?;
    module.function(["math", "cos"]?, math_cos)?;
    module.function(["math", "tan"]?, math_tan)?;
    module.function(["math", "random"]?, math_random)?;
    module.function(["math", "random_range"]?, math_random_range)?;

    Ok(())
}

/// Math absolute function for Rune
fn math_abs(x: f64) -> f64 {
    x.abs()
}

/// Math round function for Rune
fn math_round(x: f64) -> f64 {
    x.round()
}

/// Math floor function for Rune
fn math_floor(x: f64) -> f64 {
    x.floor()
}

/// Math ceil function for Rune
fn math_ceil(x: f64) -> f64 {
    x.ceil()
}

/// Math square root function for Rune
fn math_sqrt(x: f64) -> Result<f64, VmError> {
    if x < 0.0 {
        return Err(VmError::from(anyhow::anyhow!("Cannot take square root of negative number")));
    }
    Ok(x.sqrt())
}

/// Math power function for Rune
fn math_pow(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

/// Math minimum function for Rune
fn math_min(a: f64, b: f64) -> f64 {
    a.min(b)
}

/// Math maximum function for Rune
fn math_max(a: f64, b: f64) -> f64 {
    a.max(b)
}

/// Math sine function for Rune
fn math_sin(x: f64) -> f64 {
    x.sin()
}

/// Math cosine function for Rune
fn math_cos(x: f64) -> f64 {
    x.cos()
}

/// Math tangent function for Rune
fn math_tan(x: f64) -> f64 {
    x.tan()
}

/// Math random function for Rune
fn math_random() -> f64 {
    rand::random::<f64>()
}

/// Math random range function for Rune
fn math_random_range(min: f64, max: f64) -> f64 {
    rand::random::<f64>() * (max - min) + min
}

/// Install validation functions
fn install_validation_functions(module: &mut rune::Module) -> RuneResult<()> {
    module.function(["validate", "email"]?, validate_email)?;
    module.function(["validate", "url"]?, validate_url)?;
    module.function(["validate", "json"]?, validate_json)?;
    module.function(["validate", "numeric"]?, validate_numeric)?;
    module.function(["validate", "integer"]?, validate_integer)?;
    module.function(["validate", "non_empty"]?, validate_non_empty)?;

    Ok(())
}

/// Email validation function for Rune
fn validate_email(email: String) -> bool {
    // Simple email validation
    email.contains('@') && email.contains('.') && email.len() > 5
}

/// URL validation function for Rune
fn validate_url(url: String) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// JSON validation function for Rune
fn validate_json(text: String) -> bool {
    serde_json::from_str::<serde_json::Value>(&text).is_ok()
}

/// Numeric validation function for Rune
fn validate_numeric(s: String) -> bool {
    s.parse::<f64>().is_ok()
}

/// Integer validation function for Rune
fn validate_integer(s: String) -> bool {
    s.parse::<i64>().is_ok()
}

/// Non-empty validation function for Rune
fn validate_non_empty(s: String) -> bool {
    !s.trim().is_empty()
}

/// Convert serde_json::Value to rune::Value
fn rune_value_from_json(json: &serde_json::Value) -> RuneResult<Value> {
    match json {
        serde_json::Value::Null => Ok(Value::from(())),
        serde_json::Value::Bool(b) => Ok(Value::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::from(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::from(f))
            } else {
                Err(VmError::from(anyhow::anyhow!("Invalid number")))
            }
        }
        serde_json::Value::String(s) => {
            let rune_str = rune::alloc::String::try_from(s.as_str())?;
            Ok(Value::try_from(rune_str)?)
        }
        serde_json::Value::Array(arr) => {
            let mut rune_vec = rune::runtime::Vec::new();
            for item in arr {
                rune_vec.push(rune_value_from_json(item)?)?;
            }
            Ok(Value::try_from(rune_vec)?)
        }
        serde_json::Value::Object(obj) => {
            let mut rune_obj = rune::runtime::Object::new();
            for (key, val) in obj {
                let rune_key = rune::alloc::String::try_from(key.as_str())?;
                rune_obj.insert(rune_key, rune_value_from_json(val)?)?;
            }
            Ok(Value::try_from(rune_obj)?)
        }
    }
}

/// Convert rune::Value to serde_json::Value
fn json_value_from_rune(rune_value: &Value) -> RuneResult<serde_json::Value> {
    // This is a simplified conversion - in a real implementation,
    // you'd need more comprehensive handling of Rune value types
    match rune_value {
        Value::Unit => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Byte(b) => Ok(serde_json::Value::Number((*b as i64).into())),
        Value::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
        Value::Float(f) => Ok(serde_json::Value::Number(serde_json::Number::from_f64(*f)
            .ok_or_else(|| VmError::from(anyhow::anyhow!("Invalid float")))?)),
        Value::Char(c) => Ok(serde_json::Value::String(c.to_string())),
        _ => Ok(serde_json::Value::String(format!("{:?}", rune_value))),
    }
}

/// Create a context with the Crucible standard library
pub fn create_context() -> RuneResult<Context> {
    let mut context = Context::with_default_modules()?;
    let crucible_module = build_crucible_module()?;
    context.install(&crucible_module)?;
    Ok(context)
}

/// Create a context with additional modules
pub fn create_context_with_modules(modules: Vec<rune::Module>) -> RuneResult<Context> {
    let mut context = Context::with_default_modules()?;

    // Install Crucible module
    let crucible_module = build_crucible_module()?;
    context.install(&crucible_module)?;

    // Install additional modules
    for module in modules {
        context.install(&module)?;
    }

    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_crucible_module() {
        let module = build_crucible_module().unwrap();
        // Test that the module was created successfully
        assert!(module.item("http").is_some());
        assert!(module.item("file").is_some());
        assert!(module.item("json").is_some());
    }

    #[test]
    fn test_create_context() {
        let context = create_context().unwrap();
        // Test that context was created with Crucible module
        assert!(context.module("crucible").is_some());
    }

    #[tokio::test]
    async fn test_http_functions() {
        // This test would require a mock HTTP server for reliable testing
        // For now, just test function registration
        let module = build_crucible_module().unwrap();
        assert!(module.item(["http", "get"]).is_some());
        assert!(module.item(["http", "post"]).is_some());
    }

    #[test]
    fn test_string_functions() {
        let module = build_crucible_module().unwrap();
        assert!(module.item(["string", "trim"]).is_some());
        assert!(module.item(["string", "to_upper"]).is_some());
        assert!(module.item(["string", "to_lower"]).is_some());
    }

    #[test]
    fn test_math_functions() {
        let module = build_crucible_module().unwrap();
        assert!(module.item(["math", "abs"]).is_some());
        assert!(module.item(["math", "sqrt"]).is_some());
        assert!(module.item(["math", "random"]).is_some());
    }

    #[test]
    fn test_validation_functions() {
        let module = build_crucible_module().unwrap();
        assert!(module.item(["validate", "email"]).is_some());
        assert!(module.item(["validate", "url"]).is_some());
        assert!(module.item(["validate", "json"]).is_some());
    }

    #[test]
    fn test_rune_json_conversion() {
        let json = serde_json::json!({
            "name": "test",
            "value": 42,
            "active": true
        });

        let rune_value = rune_value_from_json(&json).unwrap();
        let back_to_json = json_value_from_rune(&rune_value).unwrap();

        assert_eq!(json, back_to_json);
    }
}