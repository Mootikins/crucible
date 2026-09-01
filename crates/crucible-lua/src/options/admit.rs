//! What a settings WRITE must satisfy before the plugin's setter sees it.
//!
//! Before this, `OptionsRegistry::set` resolved the node, found the inherited
//! setter, and called it with whatever JSON arrived. Every constraint the tree
//! declared was decoration: a `toggle` accepted the string `"yes"`, a `range`
//! declared `min = 60` accepted `0`, and a `select` accepted a choice its own
//! `values` had never offered.
//!
//! The check runs BEFORE the setter, never after. A plugin's setter is
//! arbitrary Lua that usually writes straight into a config table, so "the
//! plugin will validate it" means "nobody validates it" — and by the time a bad
//! value has been stored, `option_store` has already recorded it and will
//! replay it at every boot.
//!
//! Membership is checked against the EVALUATED `values`, not a static list.
//! That is the whole reason `values` may be a function: `oci` offers the
//! container runtimes actually installed on this box, so a runtime that has
//! since been uninstalled must stop being selectable, not merely stop being
//! listed.

use super::control::Control;
use crate::signature::LuaType;
use mlua::{Table, Value};

/// Check a value against its node's declared control.
///
/// `info` is the already-built callback argument, so a `values` or `disabled`
/// function sees exactly what it would see during a read.
pub fn admit_value(node: &Table, info: &Table, value: &serde_json::Value) -> Result<(), String> {
    let declared: String = node.get("type").unwrap_or_else(|_| "group".to_string());
    // `validate_tree` refuses an unknown type at declaration, so reaching this
    // means the tree predates the gate or was mutated in Lua after
    // registration. Refuse the write rather than guessing a control.
    let control = Control::parse(&declared)
        .ok_or_else(|| format!("option has an unknown type '{declared}'"))?;

    if !control.is_leaf() {
        return Err(format!("a '{declared}' option holds no value"));
    }

    if is_truthy(node, info, "disabled") {
        return Err("option is disabled".to_string());
    }

    if let Some(ty) = control.value_type() {
        admit_domain(&ty, value)
            .map_err(|got| format!("expected {}, got {got}", describe_type(&ty)))?;
    }

    if control == Control::Range {
        admit_range(node, info, value)?;
    }

    if control.requires_values() {
        admit_choice(node, info, value, control)?;
    }

    Ok(())
}

/// The declared domain, checked through the ONE type model the repo has.
fn admit_domain(ty: &LuaType, value: &serde_json::Value) -> Result<(), String> {
    let ok = match ty {
        LuaType::Any => true,
        LuaType::String => value.is_string(),
        LuaType::Boolean => value.is_boolean(),
        LuaType::Number => value.is_number(),
        LuaType::Array(inner) => match value.as_array() {
            Some(items) => items.iter().all(|item| admit_domain(inner, item).is_ok()),
            None => false,
        },
        // Nothing else is reachable from `Control::value_type` today, and a
        // control that grows one owes this match an arm rather than a pass.
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(json_kind(value).to_string())
    }
}

fn describe_type(ty: &LuaType) -> String {
    match ty {
        LuaType::String => "a string".into(),
        LuaType::Boolean => "a boolean".into(),
        LuaType::Number => "a number".into(),
        LuaType::Array(_) => "a list".into(),
        other => other.to_luau(),
    }
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "a list",
        serde_json::Value::Object(_) => "a table",
    }
}

fn admit_range(node: &Table, info: &Table, value: &serde_json::Value) -> Result<(), String> {
    let Some(n) = value.as_f64() else {
        return Ok(()); // the domain check already refused a non-number
    };
    let bound = |field: &str| evaluate_number(node, info, field);
    if let Some(min) = bound("min") {
        if n < min {
            return Err(format!("{n} is below the minimum of {min}"));
        }
    }
    if let Some(max) = bound("max") {
        if n > max {
            return Err(format!("{n} is above the maximum of {max}"));
        }
    }
    if let Some(step) = bound("step") {
        if step > 0.0 {
            let base = bound("min").unwrap_or(0.0);
            let offset = n - base;
            let steps = offset / step;
            // Float tolerance: a step of 0.1 cannot divide exactly in binary,
            // and refusing a value the UI's own slider produced would be worse
            // than accepting one a hair off the grid.
            if (steps - steps.round()).abs() > 1e-6 {
                return Err(format!("{n} is not a multiple of {step} from {base}"));
            }
        }
    }
    Ok(())
}

fn admit_choice(
    node: &Table,
    info: &Table,
    value: &serde_json::Value,
    control: Control,
) -> Result<(), String> {
    let Some(choices) = evaluate_choices(node, info) else {
        // A `values` that cannot be evaluated right now is not the writer's
        // fault, and refusing every write until it can be would make a plugin
        // whose choices come from a slow probe unusable.
        return Ok(());
    };
    let check = |candidate: &serde_json::Value| -> Result<(), String> {
        if choices.iter().any(|c| c == candidate) {
            Ok(())
        } else {
            let offered: Vec<String> = choices.iter().map(render_choice).collect();
            Err(format!(
                "{} is not one of the offered choices: {}",
                render_choice(candidate),
                offered.join(", ")
            ))
        }
    };
    if control == Control::MultiSelect {
        for item in value.as_array().into_iter().flatten() {
            check(item)?;
        }
        Ok(())
    } else {
        check(value)
    }
}

fn render_choice(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

/// The choices this box offers right now, as JSON values.
///
/// Mirrors `describe_node`'s two forms: an array (`{"podman", "docker"}`) where
/// the value IS the label, and a hash where the key is the value.
fn evaluate_choices(node: &Table, info: &Table) -> Option<Vec<serde_json::Value>> {
    let Value::Table(values) = evaluate(node, info, "values")? else {
        return None;
    };
    let mut out: Vec<serde_json::Value> = Vec::new();
    for item in values.sequence_values::<Value>().flatten() {
        if let Ok(json) = super::lua_to_json(&item) {
            out.push(json);
        }
    }
    if out.is_empty() {
        for (key, _) in values.pairs::<Value, Value>().flatten() {
            if let Ok(json) = super::lua_to_json(&key) {
                out.push(json);
            }
        }
    }
    Some(out)
}

fn evaluate_number(node: &Table, info: &Table, field: &str) -> Option<f64> {
    match evaluate(node, info, field)? {
        Value::Number(n) => Some(n),
        Value::Integer(i) => Some(i as f64),
        _ => None,
    }
}

fn is_truthy(node: &Table, info: &Table, field: &str) -> bool {
    match evaluate(node, info, field) {
        Some(Value::Boolean(b)) => b,
        Some(Value::Nil) | None => false,
        Some(_) => true,
    }
}

/// Read a field, calling it if it is a function — the same rule the whole tree
/// follows, so a bound or a choice list can be a property of this box.
fn evaluate(node: &Table, info: &Table, field: &str) -> Option<Value> {
    match node.get::<Value>(field) {
        Ok(Value::Function(f)) => f.call::<Value>((info.clone(),)).ok(),
        Ok(Value::Nil) => None,
        Ok(other) => Some(other),
        Err(_) => None,
    }
}
