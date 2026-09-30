//! Generate the TS method map from the `rpc_methods!` rows.
//!
//! Reads the params/reply type of each [`RpcMethod`] from
//! [`RpcMethod::params_type`] and [`RpcMethod::reply_type`] — the exact
//! source text of its row in `crates/crucible-core/src/protocol/rpc/method.rs`
//! — and writes one TS method map entry per row. A bare type name becomes a
//! reference into the generated OpenAPI schema (`api-schema.d.ts`) when that
//! schema already has it; otherwise the cell is `unknown`, because a row
//! whose reply is `serde_json::Value` (or whose params/reply core type has no
//! route yet, so `utoipa` never emitted it) has no TS shape to name honestly.
//!
//! Usage: `cargo run -p crucible-core --example gen_rpc_methods_ts -- <path-to-api-schema.d.ts>`.
//! Writes to stdout; the caller redirects it into
//! `crates/crucible-web/web/src/lib/rpc-methods.d.ts`.

use crucible_core::protocol::rpc::type_text::{split_generic, strip_path};
use crucible_core::protocol::RpcMethod;
use std::collections::HashSet;

fn schema_names(api_schema_path: &str) -> HashSet<String> {
    let src = std::fs::read_to_string(api_schema_path)
        .unwrap_or_else(|e| panic!("reading {api_schema_path}: {e}"));
    let Some(start) = src.find("schemas: {") else {
        return HashSet::new();
    };
    src[start..]
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            // Schema members sit 8 spaces in, one level under `schemas: {`.
            if indent != 8 {
                return None;
            }
            let name: String = trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
        .collect()
}

fn rust_to_ts(ty: &str, known: &HashSet<String>, missing: &mut Vec<String>) -> String {
    let ty = ty.trim();
    if ty == "()" {
        return "null".to_string();
    }
    if matches!(ty, "String" | "&str" | "str") {
        return "string".to_string();
    }
    if ty == "bool" {
        return "boolean".to_string();
    }
    if matches!(
        ty,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "isize"
            | "f32"
            | "f64"
    ) {
        return "number".to_string();
    }
    if ty == "serde_json::Value" {
        return "unknown".to_string();
    }
    if let Some((outer, inner)) = split_generic(ty) {
        let outer_last = strip_path(outer);
        match outer_last {
            "Vec" => return format!("({})[]", rust_to_ts(inner, known, missing)),
            "Option" => return format!("({} | null)", rust_to_ts(inner, known, missing)),
            "Scoped" => {
                let body = if inner.trim() == "()" {
                    "Record<string, never>".to_string()
                } else {
                    rust_to_ts(inner, known, missing)
                };
                return format!("WithSessionId<{body}>");
            }
            "BTreeMap" | "HashMap" => {
                // `K, V` — the daemon's maps are always string-keyed on the wire.
                let value = inner.split_once(',').map(|(_, v)| v).unwrap_or(inner);
                return format!("Record<string, {}>", rust_to_ts(value, known, missing));
            }
            _ => {}
        }
    }
    let last = strip_path(ty);
    if known.contains(last) {
        format!("Schemas['{last}']")
    } else {
        missing.push(last.to_string());
        "unknown".to_string()
    }
}

fn main() {
    let api_schema_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "crates/crucible-web/web/src/lib/api-schema.d.ts".to_string());
    let known = schema_names(&api_schema_path);
    let mut missing = Vec::new();

    println!("// GENERATED FILE. Do not hand-edit.");
    println!("//");
    println!("// Regenerate with:");
    println!("//   cargo run -p crucible-core --example gen_rpc_methods_ts -- \\");
    println!("//     crates/crucible-web/web/src/lib/api-schema.d.ts \\");
    println!("//     > crates/crucible-web/web/src/lib/rpc-methods.d.ts");
    println!("//");
    println!("// Source of truth: the `rpc_methods!` table in");
    println!("// crates/crucible-core/src/protocol/rpc/method.rs. Each entry names the");
    println!("// wire params and reply shape of one RpcMethod row. A cell is `unknown`");
    println!("// when the row's type is `serde_json::Value`, or when `api-schema.d.ts`");
    println!("// has no schema for it yet (no route emits it through utoipa today).");
    println!("import type {{ components }} from './api-schema';");
    println!();
    println!("type Schemas = components['schemas'];");
    println!();
    println!("/** A session-scoped request body: the daemon reads `session_id` at the");
    println!(" * top level, flattened beside the method's own fields (`Scoped<T>` on the");
    println!(" * Rust side). `Scoped<()>` (no fields of its own) maps `T` to `unknown`");
    println!(" * rather than intersecting `Record<string, never>` directly: TS checks a");
    println!(" * named property of an intersection against a sibling index signature, so");
    println!(" * `{{ session_id: string }} & Record<string, never>` refused every");
    println!(" * `session_id` as not assignable to the index signature's `never`. */");
    println!(
        "export type WithSessionId<T> = {{ session_id: string }} & (T extends Record<string, never> ? unknown : T);"
    );
    println!();
    println!("export interface RpcMethods {{");
    for method in RpcMethod::ALL {
        let params = rust_to_ts(method.params_type(), &known, &mut missing);
        let result = rust_to_ts(method.reply_type(), &known, &mut missing);
        println!(
            "  '{}': {{ params: {params}; result: {result} }};",
            method.as_str()
        );
    }
    println!("}}");

    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        eprintln!(
            "note: {} type name(s) have no schema in {api_schema_path} yet, mapped to `unknown`:",
            missing.len()
        );
        for name in &missing {
            eprintln!("  {name}");
        }
    }
}
