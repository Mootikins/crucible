//! What a settings tree must be before the daemon will register it.
//!
//! The tree is untrusted input: it comes from a plugin's Lua, it is walked by
//! the daemon on every settings read, and its shape decides what the frontends
//! draw. Before this, the only check was that the root carried an `args` table
//! — every other field was taken on faith and passed through to the renderers
//! verbatim.
//!
//! Everything here refuses at DECLARATION, never at render. A tree that is
//! wrong is wrong the moment it is written, and telling the plugin author at
//! load is the difference between a message naming the path and a settings
//! pane that silently draws the wrong control for the life of the install.
//!
//! The limits are not paranoia about size for its own sake. `describe_node`
//! evaluates every function-valued field on every read, so tree size is
//! directly the cost of opening the settings pane.

use super::control::Control;
use mlua::{Table, Value};

/// Deepest a tree may nest. Four is two more than any shipped plugin uses, and
/// deep enough for `group -> group -> leaf` with room to spare.
const MAX_DEPTH: usize = 4;
/// Most nodes one plugin may declare.
const MAX_NODES: usize = 200;
/// Longest a `name`, `desc` or `usage` string may be.
const MAX_TEXT: usize = 4096;

/// Why a tree was refused. Always names the path, because "invalid options
/// tree" in a plugin with forty leaves is not an error message.
#[derive(Debug)]
pub struct DeclarationError {
    pub path: Vec<String>,
    pub reason: String,
}

impl std::fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.reason)
        } else {
            write!(f, "at '{}': {}", self.path.join("."), self.reason)
        }
    }
}

fn err(path: &[String], reason: impl Into<String>) -> DeclarationError {
    DeclarationError {
        path: path.to_vec(),
        reason: reason.into(),
    }
}

/// Check a whole tree, root first.
pub fn validate_tree(root: &Table) -> Result<(), DeclarationError> {
    if !matches!(root.get::<Value>("args"), Ok(Value::Table(_))) {
        return Err(err(&[], "the root must be a group with an `args` table"));
    }
    let mut budget = MAX_NODES;
    validate_node(root, &mut Vec::new(), 0, &mut budget)
}

fn validate_node(
    node: &Table,
    path: &mut Vec<String>,
    depth: usize,
    budget: &mut usize,
) -> Result<(), DeclarationError> {
    if depth > MAX_DEPTH {
        return Err(err(path, format!("nested deeper than {MAX_DEPTH} levels")));
    }
    if *budget == 0 {
        return Err(err(
            path,
            format!("more than {MAX_NODES} options in one plugin"),
        ));
    }
    *budget -= 1;

    // The whole point of the closed set. A `type` that names no control is a
    // mistake in the plugin's source, and it is refused rather than rendered
    // as whatever the frontend guesses.
    let declared: String = node.get("type").unwrap_or_else(|_| "group".to_string());
    let Some(control) = Control::parse(&declared) else {
        let known: Vec<&str> = Control::ALL.iter().map(|c| c.as_str()).collect();
        return Err(err(
            path,
            format!(
                "unknown option type '{declared}'. Declare one of: {}",
                known.join(", ")
            ),
        ));
    };

    for field in ["name", "desc", "usage"] {
        if let Ok(Value::String(text)) = node.get::<Value>(field) {
            if text.as_bytes().len() > MAX_TEXT {
                return Err(err(
                    path,
                    format!("`{field}` is longer than {MAX_TEXT} bytes"),
                ));
            }
        }
    }

    let args = node.get::<Value>("args");
    let has_args = matches!(args, Ok(Value::Table(_)));

    if control == Control::Group {
        if !has_args {
            return Err(err(path, "a group needs an `args` table"));
        }
    } else if has_args {
        return Err(err(
            path,
            format!("only a group may carry `args`, and this is a '{declared}'"),
        ));
    }

    if control == Control::Execute && !matches!(node.get::<Value>("func"), Ok(Value::Function(_))) {
        return Err(err(path, "an execute option needs a `func`"));
    }

    // A `values` FUNCTION satisfies this: the choices are a property of the box
    // and cannot be read here. Only a statically absent `values` is refused.
    if control.requires_values() {
        match node.get::<Value>("values") {
            Ok(Value::Table(_)) | Ok(Value::Function(_)) => {}
            _ => {
                return Err(err(
                    path,
                    format!("a '{declared}' option needs `values` (a table or a function)"),
                ))
            }
        }
    }

    if control == Control::Range {
        let number = |field: &str| match node.get::<Value>(field) {
            Ok(Value::Number(n)) => Some(n),
            Ok(Value::Integer(i)) => Some(i as f64),
            _ => None,
        };
        if let (Some(min), Some(max)) = (number("min"), number("max")) {
            if min > max {
                return Err(err(
                    path,
                    format!("min ({min}) is greater than max ({max})"),
                ));
            }
        }
        if let Some(step) = number("step") {
            if step <= 0.0 {
                return Err(err(path, format!("step must be positive, not {step}")));
            }
        }
    }

    if let Ok(args) = node.get::<Table>("args") {
        for pair in args.pairs::<String, Value>() {
            let (key, child) = pair.map_err(|e| err(path, format!("unreadable `args`: {e}")))?;
            let Value::Table(child) = child else {
                return Err(err(path, format!("`args.{key}` is not a table")));
            };
            path.push(key);
            validate_node(&child, path, depth + 1, budget)?;
            path.pop();
        }
    }

    Ok(())
}
