//! Standard library extensions for Rune
//!
//! This module provides standard library functions and modules for Rune scripts
//! used as tools in the Crucible system.

use anyhow::Result;
use rune::runtime::{Output, VmError};
use rune::{Context, Value};
use std::sync::Arc;

/// Build the Crucible standard library module
pub fn build_crucible_module() -> Result<rune::Module> {
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
fn install_http_functions(module: &mut rune::Module) -> Result<()> {
    module.function([
        "http", "get"
    ])? = async fn http_get(url: String) -> Result<Value, VmError> {
        match reqwest::get(&url).await {
            Ok(response) => {
                match response.text().await {
                    Ok(text) => Ok(Value::from(text)),
                    Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP response error: {}", e))),
                }
            }
            Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP request error: {}", e))),
        }
    };

    module.function([
        "http", "post"
    ])? = async fn http_post(url: String, body: String) -> Result<Value, VmError> {
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
    };

    module.function([
        "http", "json_get"
    ])? = async fn http_json_get(url: String) -> Result<Value, VmError> {
        match reqwest::get(&url).await {
            Ok(response) => {
                match response.json::<serde_json::Value>().await {
                    Ok(json) => Ok(rune_value_from_json(&json)?),
                    Err(e) => Err(VmError::from(anyhow::anyhow!("JSON parsing error: {}", e))),
                }
            }
            Err(e) => Err(VmError::from(anyhow::anyhow!("HTTP request error: {}", e))),
        }
    };

    Ok(())
}

/// Install file system functions
fn install_file_functions(module: &mut rune::Module) -> Result<()> {
    module.function([
        "file", "read"
    ])? = fn file_read(path: String) -> Result<String, VmError> {
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(content),
            Err(e) => Err(VmError::from(anyhow::anyhow!("File read error: {}", e))),
        }
    };

    module.function([
        "file", "write"
    ])? = fn file_write(path: String, content: String) -> Result<(), VmError> {
        match std::fs::write(&path, content) {
            Ok(_) => Ok(()),
            Err(e) => Err(VmError::from(anyhow::anyhow!("File write error: {}", e))),
        }
    };

    module.function([
        "file", "exists"
    ])? = fn file_exists(path: String) -> bool {
        std::path::Path::new(&path).exists()
    };

    module.function([
        "file", "is_dir"
    ])? = fn file_is_dir(path: String) -> bool {
        std::path::Path::new(&path).is_dir()
    };

    module.function([
        "file", "is_file"
    ])? = fn file_is_file(path: String) -> bool {
        std::path::Path::new(&path).is_file()
    };

    module.function([
        "file", "list_dir"
    ])? = fn file_list_dir(path: String) -> Result<Vec<String>, VmError> {
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
    };

    Ok(())
}

/// Install JSON manipulation functions
fn install_json_functions(module: &mut rune::Module) -> Result<()> {
    module.function([
        "json", "parse"
    ])? = fn json_parse(text: String) -> Result<Value, VmError> {
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(json) => Ok(rune_value_from_json(&json)?),
            Err(e) => Err(VmError::from(anyhow::anyhow!("JSON parsing error: {}", e))),
        }
    };

    module.function([
        "json", "stringify"
    ])? = fn json_stringify(value: Value) -> Result<String, VmError> {
        let json = json_value_from_rune(&value)?;
        match serde_json::to_string(&json) {
            Ok(s) => Ok(s),
            Err(e) => Err(VmError::from(anyhow::anyhow!("JSON serialization error: {}", e))),
        }
    };

    module.function([
        "json", "stringify_pretty"
    ])? = fn json_stringify_pretty(value: Value) -> Result<String, VmError> {
        let json = json_value_from_rune(&value)?;
        match serde_json::to_string_pretty(&json) {
            Ok(s) => Ok(s),
            Err(e) => Err(VmError::from(anyhow::anyhow!("JSON serialization error: {}", e))),
        }
    };

    Ok(())
}

/// Install string manipulation functions
fn install_string_functions(module: &mut rune::Module) -> Result<()> {
    module.function([
        "string", "trim"
    ])? = fn string_trim(s: String) -> String {
        s.trim().to_string()
    };

    module.function([
        "string", "to_upper"
    ])? = fn string_to_upper(s: String) -> String {
        s.to_uppercase()
    };

    module.function([
        "string", "to_lower"
    ])? = fn string_to_lower(s: String) -> String {
        s.to_lowercase()
    };

    module.function([
        "string", "contains"
    ])? = fn string_contains(s: String, pattern: String) -> bool {
        s.contains(&pattern)
    };

    module.function([
        "string", "starts_with"
    ])? = fn string_starts_with(s: String, prefix: String) -> bool {
        s.starts_with(&prefix)
    };

    module.function([
        "string", "ends_with"
    ])? = fn string_ends_with(s: String, suffix: String) -> bool {
        s.ends_with(&suffix)
    };

    module.function([
        "string", "split"
    ])? = fn string_split(s: String, delimiter: String) -> Vec<String> {
        s.split(&delimiter).map(|s| s.to_string()).collect()
    };

    module.function([
        "string", "join"
    ])? = fn string_join(parts: Vec<String>, delimiter: String) -> String {
        parts.join(&delimiter)
    };

    module.function([
        "string", "replace"
    ])? = fn string_replace(s: String, from: String, to: String) -> String {
        s.replace(&from, &to)
    };

    Ok(())
}

/// Install time-related functions
fn install_time_functions(module: &mut rune::Module) -> Result<()> {
    module.function([
        "time", "now"
    ])? = fn time_now() -> String {
        chrono::Utc::now().to_rfc3339()
    };

    module.function([
        "time", "timestamp"
    ])? = fn time_timestamp() -> i64 {
        chrono::Utc::now().timestamp()
    };

    module.function([
        "time", "parse"
    ])? = fn time_parse(s: String) -> Result<String, VmError> {
        match chrono::DateTime::parse_from_rfc3339(&s) {
            Ok(dt) => Ok(dt.to_rfc3339()),
            Err(e) => Err(VmError::from(anyhow::anyhow!("Time parsing error: {}", e))),
        }
    };

    module.function([
        "time", "format"
    ])? = fn time_format(s: String, format: String) -> Result<String, VmError> {
        match chrono::DateTime::parse_from_rfc3339(&s) {
            Ok(dt) => Ok(dt.format(&format).to_string()),
            Err(e) => Err(VmError::from(anyhow::anyhow!("Time parsing error: {}", e))),
        }
    };

    Ok(())
}

/// Install math functions
fn install_math_functions(module: &mut rune::Module) -> Result<()> {
    module.function([
        "math", "abs"
    ])? = fn math_abs(x: f64) -> f64 {
        x.abs()
    };

    module.function([
        "math", "round"
    ])? = fn math_round(x: f64) -> f64 {
        x.round()
    };

    module.function([
        "math", "floor"
    ])? = fn math_floor(x: f64) -> f64 {
        x.floor()
    };

    module.function([
        "math", "ceil"
    ])? = fn math_ceil(x: f64) -> f64 {
        x.ceil()
    };

    module.function([
        "math", "sqrt"
    ])? = fn math_sqrt(x: f64) -> Result<f64, VmError> {
        if x < 0.0 {
            return Err(VmError::from(anyhow::anyhow!("Cannot take square root of negative number")));
        }
        Ok(x.sqrt())
    };

    module.function([
        "math", "pow"
    ])? = fn math_pow(base: f64, exp: f64) -> f64 {
        base.powf(exp)
    };

    module.function([
        "math", "min"
    ])? = fn math_min(a: f64, b: f64) -> f64 {
        a.min(b)
    };

    module.function([
        "math", "max"
    ])? = fn math_max(a: f64, b: f64) -> f64 {
        a.max(b)
    };

    module.function([
        "math", "sin"
    ])? = fn math_sin(x: f64) -> f64 {
        x.sin()
    };

    module.function([
        "math", "cos"
    ])? = fn math_cos(x: f64) -> f64 {
        x.cos()
    };

    module.function([
        "math", "tan"
    ])? = fn math_tan(x: f64) -> f64 {
        x.tan()
    };

    module.function([
        "math", "random"
    ])? = fn math_random() -> f64 {
        rand::random::<f64>()
    };

    module.function([
        "math", "random_range"
    ])? = fn math_random_range(min: f64, max: f64) -> f64 {
        rand::random::<f64>() * (max - min) + min
    };

    Ok(())
}

/// Install validation functions
fn install_validation_functions(module: &mut rune::Module) -> Result<()> {
    module.function([
        "validate", "email"
    ])? = fn validate_email(email: String) -> bool {
        // Simple email validation
        email.contains('@') && email.contains('.') && email.len() > 5
    };

    module.function([
        "validate", "url"
    ])? = fn validate_url(url: String) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    };

    module.function([
        "validate", "json"
    ])? = fn validate_json(text: String) -> bool {
        serde_json::from_str::<serde_json::Value>(&text).is_ok()
    };

    module.function([
        "validate", "numeric"
    ])? = fn validate_numeric(s: String) -> bool {
        s.parse::<f64>().is_ok()
    };

    module.function([
        "validate", "integer"
    ])? = fn validate_integer(s: String) -> bool {
        s.parse::<i64>().is_ok()
    };

    module.function([
        "validate", "non_empty"
    ])? = fn validate_non_empty(s: String) -> bool {
        !s.trim().is_empty()
    };

    Ok(())
}

/// Convert serde_json::Value to rune::Value
fn rune_value_from_json(json: &serde_json::Value) -> Result<Value, VmError> {
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
fn json_value_from_rune(rune_value: &Value) -> Result<serde_json::Value, VmError> {
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
pub fn create_context() -> Result<Context> {
    let mut context = Context::with_default_modules()?;
    let crucible_module = build_crucible_module()?;
    context.install(&crucible_module)?;
    Ok(context)
}

/// Create a context with additional modules
pub fn create_context_with_modules(modules: Vec<rune::Module>) -> Result<Context> {
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