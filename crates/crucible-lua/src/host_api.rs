//! The signatures the host declares for `cru.*`, and the Luau declaration
//! file generated from them.
//!
//! Every `cru.*` function used to be described to a plugin author as
//! `---@param ... any` / `---@return any`, which is not a description. Luau
//! can check a call against a real signature, so the host states one — in
//! Rust, next to the code that registers the function, rather than in a
//! hand-maintained `.luau` file that drifts.
//!
//! Two properties keep this honest:
//!
//! - **A signature must name a function the VM really has.** The generator
//!   walks the live `cru` table; a signature for a path that is not there is a
//!   test failure (`every_declared_signature_exists_on_the_vm`), because a
//!   declaration for a function nobody registered reads as authoritative and
//!   is worse than none.
//! - **An unsigned function is declared `(...any) -> any`, and counted.** The
//!   generated file says how many of its functions are unsigned, so the gap
//!   is visible rather than implied.
//!
//! `cru.log` carries no signature on purpose: it is a callable TABLE (it also
//! holds `levels`, `notify` and the rest), so the walker sees a namespace
//! rather than a function, and a function declaration would misstate its
//! shape.

use crate::signature::{LuaType, Param, Signature};
use std::collections::BTreeMap;

/// One entry of the host's declared surface.
struct Declared {
    path: &'static str,
    signature: fn() -> Signature,
}

fn param(name: &str, ty: LuaType) -> Param {
    Param {
        name: name.to_string(),
        ty,
        description: None,
        optional: false,
    }
}

fn optional(name: &str, ty: LuaType) -> Param {
    Param {
        name: name.to_string(),
        ty,
        description: None,
        optional: true,
    }
}

fn string() -> LuaType {
    LuaType::String
}

fn any() -> LuaType {
    LuaType::Any
}

/// `cru.shell.exec` and `cru.shell.spawn` both answer with this.
fn shell_result() -> LuaType {
    LuaType::parse("{ success: boolean, exit_code: number, stdout: string, stderr: string }")
        .expect("the shell result type is well formed")
}

/// The declared surface. Ordered by path so the generated file is stable.
const DECLARED: &[Declared] = &[
    Declared {
        path: "cru.fs.exists",
        signature: || Signature {
            params: vec![param("path", string())],
            returns: vec![LuaType::Boolean],
        },
    },
    Declared {
        path: "cru.fs.is_dir",
        signature: || Signature {
            params: vec![param("path", string())],
            returns: vec![LuaType::Boolean],
        },
    },
    Declared {
        path: "cru.fs.is_file",
        signature: || Signature {
            params: vec![param("path", string())],
            returns: vec![LuaType::Boolean],
        },
    },
    Declared {
        path: "cru.fs.mkdir",
        signature: || Signature {
            params: vec![param("path", string())],
            returns: vec![LuaType::Boolean],
        },
    },
    Declared {
        path: "cru.fs.remove_all",
        signature: || Signature {
            params: vec![param("path", string())],
            returns: vec![LuaType::Boolean],
        },
    },
    Declared {
        path: "cru.json.decode",
        signature: || Signature {
            params: vec![param("text", string())],
            returns: vec![any()],
        },
    },
    Declared {
        path: "cru.json.encode",
        signature: || Signature {
            params: vec![param("value", any())],
            returns: vec![string()],
        },
    },
    Declared {
        path: "cru.kiln.active",
        signature: || Signature {
            params: Vec::new(),
            returns: vec![LuaType::Optional(Box::new(string()))],
        },
    },
    Declared {
        path: "cru.kiln.path",
        signature: || Signature {
            params: vec![optional("name", string())],
            returns: vec![LuaType::Optional(Box::new(string()))],
        },
    },
    Declared {
        path: "cru.on",
        signature: || Signature {
            params: vec![
                param("event", string()),
                param(
                    "handler",
                    LuaType::Function(Box::new(Signature {
                        params: vec![param("payload", any())],
                        returns: vec![any()],
                    })),
                ),
                optional(
                    "opts",
                    LuaType::parse("table<string, any>").expect("well formed"),
                ),
            ],
            returns: Vec::new(),
        },
    },
    Declared {
        path: "cru.paths.workspace",
        signature: || Signature {
            params: Vec::new(),
            returns: vec![LuaType::Optional(Box::new(string()))],
        },
    },
    Declared {
        path: "cru.plugin.set_status",
        signature: || Signature {
            params: vec![param("status", string())],
            returns: Vec::new(),
        },
    },
    Declared {
        path: "cru.shell.exec",
        signature: || Signature {
            params: vec![
                param("command", string()),
                optional("args", LuaType::Array(Box::new(string()))),
                optional(
                    "options",
                    LuaType::parse("{ cwd: string?, env: table<string, string>?, stdin: string? }")
                        .expect("well formed"),
                ),
            ],
            returns: vec![shell_result()],
        },
    },
    Declared {
        path: "cru.shell.which",
        signature: || Signature {
            params: vec![param("command", string())],
            returns: vec![LuaType::Optional(Box::new(string()))],
        },
    },
    Declared {
        path: "cru.timer.clock",
        signature: || Signature {
            params: Vec::new(),
            returns: vec![LuaType::Number],
        },
    },
    Declared {
        path: "cru.timer.sleep",
        signature: || Signature {
            params: vec![param("milliseconds", LuaType::Number)],
            returns: Vec::new(),
        },
    },
    Declared {
        path: "cru.timer.spawn",
        signature: || Signature {
            params: vec![param(
                "task",
                LuaType::Function(Box::default()),
            )],
            returns: Vec::new(),
        },
    },
];

/// Every declared path, with its signature.
pub fn declared_signatures() -> BTreeMap<&'static str, Signature> {
    DECLARED
        .iter()
        .map(|entry| (entry.path, (entry.signature)()))
        .collect()
}

/// The Luau declaration for one function path, signed or not.
pub fn declaration_for(path: &str) -> Signature {
    declared_signatures()
        .get(path)
        .cloned()
        .unwrap_or_else(unsigned)
}

/// What an unsigned function is declared as: it takes anything and answers
/// anything, which is exactly as much as the host currently knows.
pub fn unsigned() -> Signature {
    Signature {
        params: vec![Param {
            name: "...".to_string(),
            ty: LuaType::Any,
            description: None,
            optional: false,
        }],
        returns: vec![LuaType::Any],
    }
}

/// Whether a path carries a real signature.
pub fn is_signed(path: &str) -> bool {
    declared_signatures().contains_key(path)
}

/// Render a Luau declaration file for the function paths given.
///
/// The shape is a nested `declare` of the `cru` table, so `luau-analyze` reads
/// `cru.shell.exec("git", { "status" })` as a call with a known result type.
pub fn render_declarations(paths: &[String]) -> String {
    let mut tree = Node::default();
    for path in paths {
        let segments: Vec<&str> = path.split('.').collect();
        if segments.first() != Some(&"cru") {
            continue;
        }
        tree.insert(&segments[1..], path);
    }

    let signed = paths.iter().filter(|path| is_signed(path)).count();
    let mut out = String::new();
    out.push_str("--!strict\n");
    out.push_str("-- Generated by `cru plugin stubs`. Do not edit.\n");
    out.push_str(&format!(
        "-- {} of {} functions carry a declared signature; the rest are\n\
         -- `(...any) -> any` until one is written in `crucible-lua/src/host_api.rs`.\n\n",
        signed,
        paths.len()
    ));
    out.push_str("declare cru: ");
    tree.render(&mut out, 0);
    out.push('\n');
    out
}

/// One level of the `cru` table while it is being rendered.
#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    /// Set on a leaf: the full path, which is what carries the signature.
    path: Option<String>,
}

impl Node {
    fn insert(&mut self, segments: &[&str], path: &str) {
        match segments {
            [] => self.path = Some(path.to_string()),
            [head, tail @ ..] => self
                .children
                .entry((*head).to_string())
                .or_default()
                .insert(tail, path),
        }
    }

    fn render(&self, out: &mut String, depth: usize) {
        if let Some(path) = &self.path {
            out.push_str(&declaration_for(path).to_luau());
            return;
        }
        let indent = "    ".repeat(depth + 1);
        let closing = "    ".repeat(depth);
        out.push_str("{\n");
        for (name, child) in &self.children {
            out.push_str(&indent);
            out.push_str(name);
            out.push_str(": ");
            child.render(out, depth + 1);
            out.push_str(",\n");
        }
        out.push_str(&closing);
        out.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signed_function_declares_its_real_types() {
        let rendered =
            render_declarations(&["cru.shell.exec".to_string(), "cru.shell.which".to_string()]);
        assert!(
            rendered.contains("exec: (command: string, args: { string }?"),
            "the signature must reach the declaration: {rendered}"
        );
        assert!(
            rendered.contains("stdout: string"),
            "the result shape must reach the declaration: {rendered}"
        );
    }

    #[test]
    fn an_unsigned_function_is_declared_as_taking_anything() {
        let rendered = render_declarations(&["cru.nothing.here".to_string()]);
        assert!(
            rendered.contains("here: (...: any) -> any"),
            "an unsigned function must be visible as unsigned: {rendered}"
        );
    }

    /// The header states the gap. A reader must be able to see how much of
    /// the surface is actually described.
    #[test]
    fn the_header_counts_the_signed_functions() {
        let rendered =
            render_declarations(&["cru.shell.exec".to_string(), "cru.nothing.here".to_string()]);
        assert!(
            rendered.contains("1 of 2 functions carry a declared signature"),
            "the header must count: {rendered}"
        );
    }

    #[test]
    fn the_declaration_nests_the_namespaces() {
        let rendered =
            render_declarations(&["cru.fs.exists".to_string(), "cru.json.encode".to_string()]);
        assert!(rendered.starts_with("--!strict\n"));
        assert!(rendered.contains("declare cru: {"));
        assert!(rendered.contains("fs: {"));
        assert!(rendered.contains("json: {"));
        assert!(rendered.contains("exists: (path: string) -> boolean"));
    }

    /// Every declared signature is well formed. A `LuaType::parse` in the
    /// table that raises would take the generator down with it.
    #[test]
    fn every_declared_signature_renders() {
        for (path, signature) in declared_signatures() {
            let rendered = signature.to_luau();
            assert!(
                rendered.starts_with('('),
                "{path} rendered oddly: {rendered}"
            );
        }
    }
}
