//! Parsing of an `rpc_methods!` row's params/reply type text.
//!
//! [`crate::protocol::rpc::RpcMethod::params_type`] and
//! [`crate::protocol::rpc::RpcMethod::reply_type`] hand back the exact source
//! text of one row (`stringify!` of the macro argument), for example
//! `"Scoped<crucible_core::protocol::requests::Page>"`. Two generators read
//! that text and both need the same two answers: a generic's outer/inner
//! split, and the named types inside it that could need an OpenAPI schema.
//! This module is the one place that answers them, so
//! `examples/gen_rpc_methods_ts.rs` (the TS method-map generator) and
//! `examples/gen_rpc_schema_types.rs` (the OpenAPI schema-list generator)
//! cannot drift apart on what a row's text means.

/// The last path segment: `a::b::Foo` becomes `Foo`.
#[must_use]
pub fn strip_path(ty: &str) -> &str {
    ty.rsplit("::").next().unwrap_or(ty)
}

/// Split `Outer<Inner>` into `("Outer", "Inner")`, respecting nested `<>`.
///
/// `Inner` may itself hold commas (`BTreeMap<K, V>`) or a further generic; the
/// caller decides how to read it.
#[must_use]
pub fn split_generic(ty: &str) -> Option<(&str, &str)> {
    let ty = ty.trim();
    let open = ty.find('<')?;
    if !ty.ends_with('>') {
        return None;
    }
    Some((&ty[..open], &ty[open + 1..ty.len() - 1]))
}

/// Whether `ty` is a Rust primitive, `()` or `serde_json::Value` — none of
/// which needs a named OpenAPI schema of its own. `utoipa` already renders
/// each of these inline wherever it appears as a field or a generic argument.
#[must_use]
pub fn is_schemaless(ty: &str) -> bool {
    matches!(
        ty.trim(),
        "()" | "serde_json::Value"
            | "String"
            | "&str"
            | "str"
            | "bool"
            | "u8"
            | "u16"
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
    )
}

/// Every named type inside `ty` that could need its own OpenAPI schema.
///
/// Recurses through the wrapper generics a row's params/reply type is built
/// from — `Vec<T>`, `Option<T>`, `Scoped<T>` (the session-scoped envelope) and
/// the value half of `BTreeMap<K, V>`/`HashMap<K, V>` — because none of those
/// wrappers is itself a schema either generator names; only what they wrap
/// is. An [`is_schemaless`] type is dropped rather than collected. An
/// unrecognised generic (there is none today) is kept whole, so a future
/// wrapper still gets a schema entry rather than silently vanishing.
#[must_use]
pub fn named_schema_types(ty: &str) -> Vec<String> {
    let mut found = Vec::new();
    collect_named_schema_types(ty, &mut found);
    found
}

/// The full text of `schema_types.rs`, freshly computed from every row of
/// [`crate::protocol::rpc::RpcMethod::ALL`].
///
/// `examples/gen_rpc_schema_types.rs` prints this to build the committed
/// file, and `method.rs`'s own `the_committed_schema_types_file_matches_the_rows`
/// test compares it against the committed file's text — one renderer for
/// both, so the writer and the staleness gate cannot disagree.
#[must_use]
pub fn render_schema_types_file() -> String {
    let mut types = std::collections::BTreeSet::new();
    for method in crate::protocol::rpc::RpcMethod::ALL {
        types.extend(named_schema_types(method.params_type()));
        types.extend(named_schema_types(method.reply_type()));
    }

    let mut out = String::new();
    out.push_str("// GENERATED FILE. Do not hand-edit.\n");
    out.push_str("//\n");
    out.push_str("// Regenerate with:\n");
    out.push_str("//   cargo run -p crucible-core --features openapi \\\n");
    out.push_str("//     --example gen_rpc_schema_types \\\n");
    out.push_str("//     > crates/crucible-core/src/protocol/rpc/schema_types.rs\n");
    out.push_str("//\n");
    out.push_str("// Source of truth: the `rpc_methods!` table in\n");
    out.push_str("// crates/crucible-core/src/protocol/rpc/method.rs. Every named params/reply\n");
    out.push_str("// type of every row is listed here, so `crucible-web`'s `api_spec` can merge\n");
    out.push_str("// a schema for it whether or not a live route names the type too. A row's\n");
    out.push_str("// `()`/`serde_json::Value`/primitive type, and the wrapper generics\n");
    out.push_str(
        "// (`Vec<T>`/`Option<T>`/`Scoped<T>`/the map types), are unwrapped rather than\n",
    );
    out.push_str("// listed — see `type_text.rs` for the rule.\n");
    out.push('\n');
    out.push_str("/// Every `rpc_methods!` row's params and reply schema, for\n");
    out.push_str("/// `crucible-web::server::api_spec` to merge into its own document.\n");
    out.push_str("///\n");
    out.push_str("/// Carries no routes and no `info` of its own; nothing reads it besides its\n");
    out.push_str("/// `components.schemas`. A row whose type gains no `ToSchema` fails this\n");
    out.push_str("/// struct's derive, so the list stays complete by construction.\n");
    out.push_str("#[derive(utoipa::OpenApi)]\n");
    out.push_str("#[openapi(components(schemas(\n");
    for ty in &types {
        out.push_str("    ");
        out.push_str(ty);
        out.push_str(",\n");
    }
    out.push_str(")))]\n");
    out.push_str("pub struct RpcMethodSchemas;\n");
    out
}

fn collect_named_schema_types(ty: &str, found: &mut Vec<String>) {
    let ty = ty.trim();
    if is_schemaless(ty) {
        return;
    }
    if let Some((outer, inner)) = split_generic(ty) {
        match strip_path(outer) {
            "Vec" | "Option" | "Scoped" => {
                collect_named_schema_types(inner, found);
                return;
            }
            "BTreeMap" | "HashMap" => {
                let value = inner.split_once(',').map_or(inner, |(_, v)| v);
                collect_named_schema_types(value, found);
                return;
            }
            _ => {}
        }
    }
    found.push(ty.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_path() {
        assert_eq!(strip_path("a::b::Foo"), "Foo");
        assert_eq!(strip_path("Foo"), "Foo");
    }

    #[test]
    fn splits_a_generic() {
        assert_eq!(split_generic("Vec<Foo>"), Some(("Vec", "Foo")));
        assert_eq!(
            split_generic("BTreeMap<String, Foo>"),
            Some(("BTreeMap", "String, Foo"))
        );
        assert_eq!(split_generic("Foo"), None);
    }

    #[test]
    fn drops_a_schemaless_type() {
        assert_eq!(named_schema_types("()"), Vec::<String>::new());
        assert_eq!(
            named_schema_types("serde_json::Value"),
            Vec::<String>::new()
        );
        assert_eq!(named_schema_types("String"), Vec::<String>::new());
        assert_eq!(named_schema_types("u64"), Vec::<String>::new());
    }

    #[test]
    fn keeps_a_named_type() {
        assert_eq!(
            named_schema_types("crucible_core::protocol::requests::Page"),
            vec!["crucible_core::protocol::requests::Page"]
        );
    }

    #[test]
    fn recurses_through_a_wrapper_generic() {
        assert_eq!(
            named_schema_types("Vec<crucible_core::protocol::requests::VectorHit>"),
            vec!["crucible_core::protocol::requests::VectorHit"]
        );
        assert_eq!(
            named_schema_types("Option<crucible_core::project::Project>"),
            vec!["crucible_core::project::Project"]
        );
        assert_eq!(
            named_schema_types(
                "crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::Page>"
            ),
            vec!["crucible_core::protocol::requests::Page"]
        );
        assert_eq!(
            named_schema_types("crucible_core::protocol::requests::Scoped<()>"),
            Vec::<String>::new()
        );
    }
}
