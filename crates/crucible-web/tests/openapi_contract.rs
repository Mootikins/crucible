//! The OpenAPI document that the axum router generates.
//!
//! Task A1 proves the chain on one route, `GET /api/models`: the `ToSchema`
//! derive, the `#[utoipa::path]` attribute, the `OpenApiRouter`, this test and
//! the committed `openapi.json`. Later tasks add the other routes.
//!
//! Task A3 adds the staleness gate: the committed `openapi.json` must equal
//! the document this router builds, and `just lint types` holds the generated
//! TypeScript to the same rule.
//!
//! Task A11 adds the streams: every SSE route names `text/event-stream` and
//! the event union it carries, and the browser's event-name lists are compared
//! against those unions in both directions.
//!
//! Task A4 adds the completeness gate: the router and the browser must both
//! name only routes the document describes. Task A10 converted the last route
//! group, so all three of its tests demand an empty missing list.
//!
//! To regenerate the committed document, run:
//! `cargo test -p crucible-web --test openapi_contract -- --ignored write_openapi_json`

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crucible_web::fs_events::FsEvent;
use crucible_web::server::api_spec;
use crucible_web::ChatEvent;

/// The document is JSON, so the test reads it as JSON.
fn spec_json() -> serde_json::Value {
    serde_json::to_value(api_spec()).expect("the spec serialises to JSON")
}

fn openapi_json_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi.json")
}

#[test]
fn the_spec_describes_list_all_models() {
    let spec = spec_json();

    let operation = &spec["paths"]["/api/models"]["get"];
    assert!(
        !operation.is_null(),
        "the document has no GET /api/models operation"
    );

    let reference = operation["responses"]["200"]["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap_or_else(|| panic!("the 200 reply names no schema: {operation:#}"));
    assert_eq!(reference, "#/components/schemas/ModelsResponse");

    let schema = &spec["components"]["schemas"]["ModelsResponse"];
    assert_eq!(
        schema["properties"]["models"]["type"], "array",
        "`models` is not an array: {schema:#}"
    );
    assert_eq!(
        schema["properties"]["models"]["items"]["type"], "string",
        "`models` does not hold strings: {schema:#}"
    );
}

/// The spelling the writer uses, so a diff is a real difference.
fn rendered_document() -> String {
    let mut document = serde_json::to_string_pretty(&api_spec()).expect("the spec serialises");
    document.push('\n');
    document
}

/// Set when `just web-contract` runs this file's gate in write mode.
///
/// The writer is a mode of the gate rather than an `#[ignore]`d sibling,
/// because an ignored test must declare a machine-readable prerequisite that
/// `just test gated` can satisfy, and "somebody asked for it" is not one.
/// Writing and checking then share one renderer, so they cannot disagree.
/// The recipe scopes the variable to its child process; nothing calls
/// `set_var`.
const WRITE_DOCUMENT_ENV: &str = "CRUCIBLE_WRITE_OPENAPI";

/// The committed document says what the router says.
///
/// `openapi.json` drifts whenever a handler changes and nobody runs
/// `just web-contract`, and the generated TypeScript then describes a route
/// that no longer exists. This test is the gate; `just lint types` gates the
/// TypeScript half. With `CRUCIBLE_WRITE_OPENAPI` set it writes the document
/// instead of comparing it.
#[test]
fn the_committed_openapi_json_is_current() {
    let path = openapi_json_path();

    if std::env::var_os(WRITE_DOCUMENT_ENV).is_some() {
        std::fs::write(&path, rendered_document()).expect("the writer writes openapi.json");
        println!("wrote {}", path.display());
        return;
    }

    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the test reads {}: {error}", path.display()));

    let rendered = rendered_document();
    if committed == rendered {
        return;
    }

    // Not `assert_eq!`: the two documents run to thousands of lines, and the
    // reader needs the first line that differs, not both copies of the file.
    let (line, committed_line, rendered_line) = first_difference(&committed, &rendered);
    panic!(
        "{} is stale; run `just web-contract`\n\
         line {line}\n\
         committed: {committed_line}\n\
         router:    {rendered_line}",
        path.display()
    );
}

/// The first line that differs, as `(line number, committed, rendered)`.
///
/// A missing line reads as `<end of file>` so that a truncated document names
/// the place it stops rather than an empty string.
fn first_difference(committed: &str, rendered: &str) -> (usize, String, String) {
    let mut committed_lines = committed.lines();
    let mut rendered_lines = rendered.lines();
    let mut line = 0;
    loop {
        line += 1;
        let left = committed_lines.next();
        let right = rendered_lines.next();
        if left == right {
            if left.is_none() {
                return (
                    line,
                    "<end of file>".to_string(),
                    "<end of file>".to_string(),
                );
            }
            continue;
        }
        let shown = |entry: Option<&str>| {
            entry.map_or_else(
                || "<end of file>".to_string(),
                |text| text.trim().to_string(),
            )
        };
        return (line, shown(left), shown(right));
    }
}

/// The path of a TypeScript source file under `crates/crucible-web/web`.
fn web_src(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("web/src")
        .join(relative)
}


/// Every `type` tag value an internally-tagged enum schema can take.
///
/// utoipa renders such an enum as a `oneOf` of variants, and each variant
/// declares its tag as a single-value enum on the `type` property. A variant
/// may arrive wrapped in an `allOf`, so this walks both shapes.
fn discriminator_values(schema: &serde_json::Value) -> Vec<String> {
    fn tag_of(variant: &serde_json::Value, found: &mut Vec<String>) {
        if let Some(members) = variant["allOf"].as_array() {
            for member in members {
                tag_of(member, found);
            }
        }
        let tag = &variant["properties"]["type"];
        if let Some(value) = tag["const"].as_str() {
            found.push(value.to_string());
        }
        if let Some(values) = tag["enum"].as_array() {
            for value in values {
                if let Some(value) = value.as_str() {
                    found.push(value.to_string());
                }
            }
        }
    }

    let variants = schema["oneOf"]
        .as_array()
        .unwrap_or_else(|| panic!("the schema is not a `oneOf` union: {schema:#}"));
    let mut found = Vec::new();
    for variant in variants {
        tag_of(variant, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

/// One value of every `ChatEvent` variant.
///
/// The list is held complete by the document: the assertions below compare it
/// to the union `utoipa` derives from the enum, so a variant added in Rust and
/// not added here fails as a name the document has and this list does not.
fn one_chat_event_per_variant() -> Vec<ChatEvent> {
    vec![
        ChatEvent::Token {
            content: String::new(),
        },
        ChatEvent::ToolCall {
            id: String::new(),
            title: String::new(),
            arguments: None,
        },
        ChatEvent::ToolResult {
            id: String::new(),
            result: None,
            terminate: false,
        },
        ChatEvent::ToolResultDelta {
            id: String::new(),
            delta: String::new(),
        },
        ChatEvent::ToolResultComplete { id: String::new() },
        ChatEvent::ToolResultError {
            id: String::new(),
            error: String::new(),
        },
        ChatEvent::Thinking {
            content: String::new(),
        },
        ChatEvent::SegmentComplete {
            message_id: String::new(),
            index: 0,
            content: String::new(),
        },
        ChatEvent::MessageComplete {
            id: String::new(),
            content: String::new(),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            cache_read_tokens: None,
            cache_creation_tokens: None,
            stop_reason: None,
            stop_notice: None,
        },
        ChatEvent::Error {
            code: String::new(),
            message: String::new(),
        },
        ChatEvent::InteractionRequested {
            id: String::new(),
            request: serde_json::json!({}),
        },
        ChatEvent::SubagentSpawned {
            id: String::new(),
            prompt: String::new(),
        },
        ChatEvent::SubagentCompleted {
            id: String::new(),
            summary: String::new(),
        },
        ChatEvent::SubagentFailed {
            id: String::new(),
            error: String::new(),
        },
        ChatEvent::DelegationSpawned {
            id: String::new(),
            prompt: String::new(),
            target_agent: None,
        },
        ChatEvent::DelegationCompleted {
            id: String::new(),
            summary: String::new(),
        },
        ChatEvent::DelegationFailed {
            id: String::new(),
            error: String::new(),
        },
        ChatEvent::ContextUsage { used: 0, total: 0 },
        ChatEvent::PrecognitionResult {
            notes_count: 0,
            notes: Vec::new(),
        },
        ChatEvent::ModeChanged {
            mode: String::new(),
        },
        ChatEvent::TitleChanged {
            title: String::new(),
        },
        ChatEvent::SessionEvent {
            event: String::new(),
            data: serde_json::Value::Null,
        },
    ]
}

/// The serde tag one event serialises under.
fn serde_tag(event: &impl serde::Serialize) -> String {
    serde_json::to_value(event).expect("the event serialises")["type"]
        .as_str()
        .expect("an internally-tagged enum carries a `type` string")
        .to_string()
}

/// The SSE `event:` name and the payload's `type` tag name the same event.
///
/// The browser installs a listener per `event_name()` and then switches on the
/// payload's `type`, so the two have to agree or a listener fires for a shape
/// its handler cannot read. The document stands in for the tag, because
/// `utoipa` derives it from the same enum the serde tag comes from.
///
/// The browser's own side of this contract is no longer read here.
/// `SSE_EVENT_TYPES` in `web/src/lib/api.ts` is now bound to the generated
/// `ChatEvent` union by a `satisfies` clause and an `Exclude` check, so
/// `bun run typecheck` fails in both directions and this test does not have to
/// parse TypeScript to say the same thing.
#[test]
fn every_sse_event_name_is_in_the_document() {
    let spec = spec_json();

    let chat = one_chat_event_per_variant();
    for event in &chat {
        assert_eq!(
            event.event_name(),
            serde_tag(event),
            "`ChatEvent::event_name` and the serde tag disagree"
        );
    }
    let mut chat_names: Vec<String> = chat.iter().map(serde_tag).collect();
    chat_names.sort();
    chat_names.dedup();
    assert_eq!(
        discriminator_values(&spec["components"]["schemas"]["ChatEvent"]),
        chat_names,
        "`ChatEvent` and the document name different events"
    );

    // The filesystem stream's SSE `event:` names carry an `fs_` prefix that the
    // serde tag does not: `fs_changed` on the wire envelope, `changed` in the
    // payload. `FsEvent::event_name` writes the prefix, so the test strips it.
    let fs = vec![
        FsEvent::Changed {
            path: String::new(),
            kind: String::new(),
        },
        FsEvent::Deleted {
            path: String::new(),
        },
        FsEvent::Moved {
            from: String::new(),
            to: String::new(),
        },
    ];
    for event in &fs {
        assert_eq!(
            event.event_name().strip_prefix("fs_"),
            Some(serde_tag(event).as_str()),
            "`FsEvent::event_name` and the serde tag disagree"
        );
    }
    let mut fs_names: Vec<String> = fs.iter().map(serde_tag).collect();
    fs_names.sort();
    fs_names.dedup();
    assert_eq!(
        discriminator_values(&spec["components"]["schemas"]["FsEvent"]),
        fs_names,
        "`FsEvent` and the document name different events"
    );
}

/// Every stream route says it streams, and names the union it streams.
#[test]
fn every_stream_route_answers_with_an_event_stream() {
    let spec = spec_json();

    for (method, path, schema) in [
        ("get", "/api/chat/events/{session_id}", "ChatEvent"),
        ("get", "/api/fs/events", "FsEvent"),
        ("get", "/api/plugins/events", "PublicationChangedEvent"),
        ("get", "/api/surfaces/events", "SurfaceChangedEvent"),
        ("post", "/api/shell/exec", "ShellEvent"),
    ] {
        let operation = &spec["paths"][path][method];
        assert!(
            !operation.is_null(),
            "the document has no {method} {path} operation"
        );

        let content = &operation["responses"]["200"]["content"]["text/event-stream"];
        assert!(
            !content.is_null(),
            "{method} {path} does not answer `text/event-stream`: {operation:#}"
        );
        assert_eq!(
            content["schema"]["$ref"].as_str(),
            Some(format!("#/components/schemas/{schema}").as_str()),
            "{method} {path} does not name `{schema}`"
        );
    }
}

// ===========================================================================
// Task A4 — the router and the document describe the same routes.
//
// A handler with no `#[utoipa::path]` is absent from the document, so the
// generated TypeScript never learns the route exists and `bun run typecheck`
// stays green while the client calls a route it cannot see. Three tests close
// that hole:
//
//   1. every route the router registers has an operation;
//   2. every `/api` path the browser asks for has an operation;
//   3. every `/api` path the browser asks for reaches a route, which holds
//      even for a route the document does not describe, because a registered
//      route serves its path either way.
//
// They replace `every_frontend_api_path_has_a_backend_route`
// (`crates/crucible-cli/tests/architecture_tests.rs`), which read one file,
// ignored the method and compared paths against a second regex scan. Test 3
// is that gate, widened; tests 1 and 2 are what it could not ask.
// ===========================================================================

/// A path the router serves that the document deliberately never describes,
/// and the reason.
///
/// `health_routes` and `auth_routes` merge onto the application in
/// `build_router`, outside the `api_router` group that `api_spec` describes,
/// so no conversion puts them in the document. The catch-all sits inside the
/// group and answers 404 for every `/api` path no route claims.
///
/// The list names paths, not `(method, path)` pairs: an entry exempts every
/// method on that path.
const PATHS_OUTSIDE_THE_DOCUMENT: &[(&str, &str)] = &[
    (
        "/health",
        "a public operator probe, merged outside the API group",
    ),
    (
        "/ready",
        "a public operator probe, merged outside the API group",
    ),
    (
        "/api/auth/login",
        "the public login bootstrap, merged outside the API group",
    ),
    (
        "/api/auth/logout",
        "the public logout, merged outside the API group",
    ),
    (
        "/api/{*unmatched}",
        "the catch-all that refuses every other /api path",
    ),
];

/// The method-router constructors that name an HTTP method in an axum
/// `.route(...)` call.
///
/// `any` is here because the catch-all uses it. It is not an OpenAPI method,
/// so a route that answers with `any` can never be described; only the
/// allow-listed catch-all does that today.
const AXUM_METHOD_ROUTERS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "head", "options", "trace", "any",
];

/// The keys an OpenAPI path item uses for an operation. A path item also holds
/// `parameters`, `summary` and `$ref`, which name no method.
const OPENAPI_METHODS: &[&str] = &[
    "get", "post", "put", "delete", "patch", "head", "options", "trace",
];

fn path_is_outside_the_document(path: &str) -> bool {
    PATHS_OUTSIDE_THE_DOCUMENT
        .iter()
        .any(|(listed, _)| *listed == path)
}

// --- Reading the Rust sources ----------------------------------------------

/// Every Rust source that registers a route: the files under `src/routes`,
/// plus `src/server.rs`, each with its test modules removed.
///
/// The test modules build routers of their own (`/api/test`, `/health`, `/`)
/// that no server serves, and a scan that counted them would demand
/// operations for them.
fn route_sources() -> Vec<(PathBuf, String)> {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths: Vec<PathBuf> = walkdir::WalkDir::new(crate_root.join("src/routes"))
        .into_iter()
        .filter_map(Result::ok)
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.extension().and_then(|end| end.to_str()) == Some("rs"))
        .filter(|path| !file_holds_only_tests(path))
        .collect();
    paths.sort();
    paths.push(crate_root.join("src/server.rs"));

    paths
        .into_iter()
        .map(|path| {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("the test reads {}: {error}", path.display()));
            let source = without_test_modules(&source);
            (path, source)
        })
        .collect()
}

/// Whether the file is a test module of its own, named `tests.rs` or
/// `<something>_tests.rs`. A `#[cfg(test)] mod tests;` declaration puts the
/// test code in such a file, where no attribute marks it.
fn file_holds_only_tests(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name == "tests.rs" || name.ends_with("_tests.rs")
}

/// The source with every inline `#[cfg(test)]` module removed.
///
/// Two spellings appear, and only one opens a body here. A
/// `#[cfg(test)] mod tests;` declaration points at a separate file and must
/// be stepped over: `session_config/mod.rs` declares its tests at line 30 and
/// registers six routes at line 50. Cutting at that attribute dropped all six
/// and left the gates green about them.
///
/// An inline module is removed rather than truncated at, so a route declared
/// after a test module is still read. The module ends at the next line that
/// is a lone `}` in the first column, which is where rustfmt closes a
/// top-level item, and `cargo fmt --check` gates that. Without such a line the
/// module runs to the end of the file, which is the common case.
fn without_test_modules(source: &str) -> String {
    const ATTRIBUTE: &str = "#[cfg(test)]";
    let mut kept = String::new();
    let mut rest = source;
    while let Some(at) = rest.find(ATTRIBUTE) {
        let after = &rest[at + ATTRIBUTE.len()..];
        let declares_a_file = match (after.find('{'), after.find(';')) {
            (Some(brace), Some(semicolon)) => semicolon < brace,
            (None, Some(_)) => true,
            _ => false,
        };
        if declares_a_file {
            let step = at + ATTRIBUTE.len();
            kept.push_str(&rest[..step]);
            rest = &rest[step..];
            continue;
        }
        kept.push_str(&rest[..at]);
        rest = match after.find("\n}\n") {
            Some(end) => &after[end + "\n}\n".len()..],
            None => "",
        };
    }
    kept.push_str(rest);
    kept
}

/// The text between the parenthesis at `open` and the one that closes it.
///
/// A `"` opens a string literal and a `//` opens a line comment; neither's
/// contents count as structure. The route and nest calls this reads hold no
/// raw string literals and no block comments.
fn call_arguments(source: &str, open: usize) -> Option<&str> {
    let bytes = source.as_bytes();
    assert_eq!(bytes[open], b'(', "the offset does not name a parenthesis");
    let mut depth = 0usize;
    let mut index = open;
    let mut in_string = false;
    let mut escaped = false;
    while index < bytes.len() {
        let character = bytes[index] as char;
        if in_string {
            match (escaped, character) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            index += 1;
            continue;
        }
        match character {
            '"' => in_string = true,
            '/' if bytes.get(index + 1) == Some(&b'/') => {
                index += source[index..].find('\n').unwrap_or(source.len() - index);
                continue;
            }
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&source[open + 1..index]);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// The first `"…"` literal in the text, and the offset just past its closing
/// quote.
///
/// Route and nest paths hold no escapes, so the literal reads through as
/// written.
fn first_string_literal(text: &str) -> Option<(String, usize)> {
    let start = text.find('"')? + 1;
    let length = text[start..].find('"')?;
    Some((text[start..start + length].to_string(), start + length + 1))
}

/// Whether the identifier at `start` stands alone rather than inside a longer
/// name: `get(` is a method router, the `get` in `get_config` is not.
fn is_whole_identifier(text: &str, start: usize, length: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[start + length..].chars().next();
    let joins = |character: Option<char>| {
        character.is_some_and(|character| character.is_alphanumeric() || character == '_')
    };
    !joins(before) && after == Some('(')
}

/// Every HTTP method the method-router expression names, in source order.
fn methods_in(expression: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (index, _) in expression.char_indices() {
        for method in AXUM_METHOD_ROUTERS {
            if expression[index..].starts_with(method)
                && is_whole_identifier(expression, index, method.len())
                && !found.iter().any(|seen| seen == method)
            {
                found.push((*method).to_string());
            }
        }
    }
    found
}

/// The name of the function whose body holds `offset`.
///
/// The scan reads whole route-builder functions, and every `.route(...)` call
/// in these files sits in one, so the nearest preceding `fn` owns it.
fn enclosing_function(source: &str, offset: usize) -> String {
    let mut best: Option<String> = None;
    for (index, _) in source[..offset].char_indices() {
        if !source[index..].starts_with("fn ") || !is_word_start(source, index) {
            continue;
        }
        let rest = &source[index + 3..];
        let name: String = rest
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if !name.is_empty() {
            best = Some(name);
        }
    }
    best.unwrap_or_else(|| "<no enclosing function>".to_string())
}

fn is_word_start(text: &str, start: usize) -> bool {
    !text[..start]
        .chars()
        .next_back()
        .is_some_and(|character| character.is_alphanumeric() || character == '_')
}

/// Each nested router function and the prefix `api_router` mounts it under.
///
/// Resolving the prefix by name is the point: the test this replaces joined
/// every prefix onto every relative path, which invented paths no router
/// serves. `/api/terminal` plus `terminal.rs`'s `/ws` is one real pair, not
/// two candidates.
fn nest_prefixes(server: &str) -> std::collections::BTreeMap<String, String> {
    let mut prefixes = std::collections::BTreeMap::new();
    let mut search = 0usize;
    while let Some(offset) = server[search..].find(".nest(") {
        let open = search + offset + ".nest".len();
        let arguments = call_arguments(server, open).expect("the nest call closes its parenthesis");
        let (prefix, _) = first_string_literal(arguments).expect("the nest call names a prefix");
        let function = nested_function_name(arguments)
            .unwrap_or_else(|| panic!("no `*_routes()` call in `.nest(\"{prefix}\", …)`"));
        prefixes.insert(function, prefix);
        search = open + 1;
    }
    prefixes
}

/// The first `…_routes(` call in a nest call's arguments.
fn nested_function_name(arguments: &str) -> Option<String> {
    let mut search = 0usize;
    while let Some(offset) = arguments[search..].find("_routes(") {
        let end = search + offset + "_routes".len();
        let start = arguments[..end]
            .char_indices()
            .rev()
            .take_while(|(_, character)| character.is_alphanumeric() || *character == '_')
            .map(|(index, _)| index)
            .last()?;
        if is_word_start(arguments, start) {
            return Some(arguments[start..end].to_string());
        }
        search = end;
    }
    None
}

/// Each nested router function is defined once in the whole scan.
///
/// The prefix map is keyed by function name, and `enclosing_function` answers
/// with a name too. Two files that both defined `shell_routes` would make
/// every route in the wrong one inherit `/api/shell`, and the gate would then
/// demand an operation at a path no router serves while missing the real one.
fn assert_nested_names_are_unique(
    sources: &[(PathBuf, String)],
    prefixes: &std::collections::BTreeMap<String, String>,
) {
    for (name, prefix) in prefixes {
        let needle = format!("fn {name}(");
        let homes: Vec<String> = sources
            .iter()
            .filter(|(_, source)| source.contains(&needle))
            .map(|(path, _)| path.display().to_string())
            .collect();
        assert_eq!(
            homes.len(),
            1,
            "`{name}` is nested at `{prefix}` and defined in {} files ({}); \
             the scan resolves the prefix by name, so every route in the other \
             file would take `{prefix}` too. Rename one.",
            homes.len(),
            homes.join(", ")
        );
    }
}

/// Every `(method, path)` pair an axum `.route(…)` call registers.
///
/// `.routes(routes!(handler))` is absent on purpose. That form takes its path
/// from the handler's `#[utoipa::path]` attribute and writes the same
/// operation into the document, so it cannot be registered and undescribed.
/// `.route(…)` is the form that can, and the one A5 to A10 convert away.
fn routes_the_router_serves() -> BTreeSet<(String, String)> {
    let sources = route_sources();
    let server = sources
        .iter()
        .find(|(path, _)| path.ends_with("server.rs"))
        .map(|(_, source)| source.clone())
        .expect("the scan reads server.rs");
    let prefixes = nest_prefixes(&server);
    assert_nested_names_are_unique(&sources, &prefixes);

    let mut registered = BTreeSet::new();
    for (file, source) in &sources {
        let mut found_here = 0usize;
        let mut search = 0usize;
        while let Some(offset) = source[search..].find(".route(") {
            let open = search + offset + ".route".len();
            let arguments = call_arguments(source, open).unwrap_or_else(|| {
                panic!(
                    "a `.route(` call in {} closes no parenthesis",
                    file.display()
                )
            });
            let (path, after_path) = first_string_literal(arguments)
                .unwrap_or_else(|| panic!("a `.route(` call in {} names no path", file.display()));
            let methods = methods_in(&arguments[after_path..]);
            assert!(
                !methods.is_empty(),
                "`.route(\"{path}\", …)` in {} names no HTTP method",
                file.display()
            );

            let owner = enclosing_function(source, open);
            let full = match prefixes.get(&owner) {
                Some(prefix) => format!("{prefix}{path}"),
                None => path.clone(),
            };
            assert!(
                full.starts_with("/api") || path_is_outside_the_document(&full),
                "`{path}` in `{owner}` ({}) resolves to `{full}`, which names no \
                 served prefix; teach `nest_prefixes` where `{owner}` mounts",
                file.display()
            );

            for method in methods {
                registered.insert((method, full.clone()));
                found_here += 1;
            }
            search = open + 1;
        }

        // The scan reads text, so a parser that stops parsing reports an empty
        // missing list and looks like success. A file that still writes
        // `.route("/api…` must still yield a pair from it.
        assert!(
            !source.contains(".route(\"/api") || found_here > 0,
            "the scan found no route in {}, which still registers one; \
             the parser has broken",
            file.display()
        );
    }
    registered
}

// --- Reading the document ---------------------------------------------------

/// Every `(method, path)` pair the document describes.
fn documented_operations() -> BTreeSet<(String, String)> {
    let spec = spec_json();
    let paths = spec["paths"]
        .as_object()
        .expect("the document holds a `paths` object")
        .clone();

    let mut described = BTreeSet::new();
    for (path, item) in paths {
        let item = item.as_object().expect("a path item is an object");
        for method in item.keys() {
            if OPENAPI_METHODS.contains(&method.as_str()) {
                described.insert((method.clone(), path.clone()));
            }
        }
    }
    described
}

/// The `(method, path)` pairs the router serves and the document does not,
/// as `METHOD /path` lines.
fn undescribed_routes() -> Vec<String> {
    let described = documented_operations();
    routes_the_router_serves()
        .iter()
        .filter(|(_, path)| !path_is_outside_the_document(path))
        .filter(|pair| !described.contains(*pair))
        .map(|(method, path)| format!("{} {path}", method.to_uppercase()))
        .collect()
}

/// Every route the router registers has an operation in the document.
///
/// This is the test that makes "a route added in Rust fails `bun run
/// typecheck`" true: an undescribed route reaches no generated type, so
/// nothing downstream can notice it.
#[test]
fn every_route_the_router_serves_is_in_the_document() {
    let missing = undescribed_routes();

    assert!(
        missing.is_empty(),
        "{} routes the router serves have no OpenAPI operation. Give the \
         handler a `#[utoipa::path]` and register it with \
         `.routes(routes!(handler))`:\n  - {}",
        missing.len(),
        missing.join("\n  - ")
    );
}

// --- Reading the browser's API calls ----------------------------------------

/// The TypeScript modules that name `/api` paths as literals.
///
/// `review-api.ts` is here because the regex scan this test replaces never
/// read it, and its seven paths went unchecked.
const CLIENT_API_MODULES: &[(&str, usize)] = &[("lib/api.ts", 70), ("lib/review-api.ts", 7)];

/// A path with its parameter names removed: `/api/session/{id}` and
/// `/api/session/${id}` both read as `/api/session/{}`.
///
/// TypeScript interpolates a value where OpenAPI names a parameter, so the
/// two sides compare by shape. The query string carries no route, so it goes.
/// Two adjacent holes collapse into one: `/api/plugins/${name}${query}` ends
/// in a conditional query suffix, not a second segment.
fn path_shape(raw: &str) -> String {
    let mut shaped = String::new();
    let mut rest = raw;
    while let Some(open) = rest.find('{') {
        let head = &rest[..open];
        // `${` and `{` both open a hole; the `$` is not part of the path.
        shaped.push_str(head.strip_suffix('$').unwrap_or(head));
        shaped.push_str("{}");
        let close = rest[open..]
            .find('}')
            .unwrap_or_else(|| panic!("`{raw}` opens a hole it never closes"));
        rest = &rest[open + close + 1..];
    }
    shaped.push_str(rest);

    let shaped = shaped.split('?').next().unwrap_or(&shaped).to_string();
    let mut collapsed = shaped;
    while collapsed.contains("{}{}") {
        collapsed = collapsed.replace("{}{}", "{}");
    }
    collapsed
        .strip_suffix('/')
        .unwrap_or(&collapsed)
        .to_string()
}

/// Every quoted literal that could name an API path, as
/// `(quote, body, body start)`.
///
/// The scan finds an OPENING quote by what follows it, rather than by
/// counting quotes from the start of the file. TypeScript prose is full of
/// apostrophes — "doesn't", "the daemon's" — and a parity count over `'`
/// therefore reads the wrong halves of the file as strings and drops real
/// paths without saying so. `'/api/providers'` and `'/api/kilns'` were both
/// lost that way.
///
/// A literal qualifies when it opens with `/api`, or when it is a template
/// that opens with a `${…}` hole, which is how a module reuses a path prefix.
fn api_path_literals(source: &str) -> Vec<(char, String, usize)> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let quote = bytes[index] as char;
        if !matches!(quote, '\'' | '"' | '`') {
            index += 1;
            continue;
        }
        let rest = &source[index + 1..];
        let opens_a_path = rest.starts_with("/api") || (quote == '`' && rest.starts_with("${"));
        // A quote that opens nothing is stepped over, never used to stop. An
        // apostrophe in prose has no partner, and a scan that gave up on it
        // would drop every path in the rest of the file in silence.
        if !opens_a_path {
            index += 1;
            continue;
        }
        let Some(length) = rest.find(quote) else {
            index += 1;
            continue;
        };
        found.push((quote, rest[..length].to_string(), index + 1));
        index += 1 + length + 1;
    }
    found
}

/// The path prefixes a module builds once and reuses, as
/// `const base = (id: string) => \`/api/…\`;`.
///
/// `review-api.ts` writes all seven of its paths as `${base(sessionId)}/…`.
/// A scan that read literals alone would find the prefix and none of the
/// paths, and would report that the file calls nothing.
fn path_prefix_aliases(source: &str) -> std::collections::BTreeMap<String, (String, usize)> {
    let mut aliases = std::collections::BTreeMap::new();
    for (quote, body, start) in api_path_literals(source) {
        if quote != '`' || !body.starts_with("/api") {
            continue;
        }
        let Some(head) = source[..start - 1].trim_end().strip_suffix("=>") else {
            continue;
        };
        let Some(declaration) = head.rfind("const ") else {
            continue;
        };
        let name: String = head[declaration + "const ".len()..]
            .trim_start()
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if !name.is_empty() {
            aliases.insert(name, (body, start));
        }
    }
    aliases
}

/// Every `/api` path shape a module asks for.
fn client_api_paths(source: &str) -> BTreeSet<String> {
    let aliases = path_prefix_aliases(source);
    // The alias's own template is a prefix, not a path anybody fetches:
    // `review-api.ts` never asks for `/api/session/{id}/review` itself.
    let declarations: BTreeSet<usize> = aliases.values().map(|(_, start)| *start).collect();

    let mut called = BTreeSet::new();
    for (_, body, start) in api_path_literals(source) {
        if declarations.contains(&start) {
            continue;
        }
        let resolved = resolve_alias(&body, &aliases);
        if resolved.starts_with("/api") {
            called.insert(path_shape(&resolved));
        }
    }
    called
}

/// A literal that opens with `${alias(…)}`, rewritten with the alias's own
/// template in front. Any other literal reads through unchanged.
fn resolve_alias(
    literal: &str,
    aliases: &std::collections::BTreeMap<String, (String, usize)>,
) -> String {
    let Some(rest) = literal.strip_prefix("${") else {
        return literal.to_string();
    };
    let Some(close) = rest.find('}') else {
        return literal.to_string();
    };
    let call = &rest[..close];
    let name = call
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect::<String>();
    match aliases.get(&name) {
        Some((prefix, _)) => format!("{prefix}{}", &rest[close + 1..]),
        None => literal.to_string(),
    }
}

/// The `/api` path shapes the browser asks for and the document does not, as
/// `module  /path` lines.
fn undescribed_client_paths() -> Vec<String> {
    let described: BTreeSet<String> = documented_operations()
        .into_iter()
        .map(|(_, path)| path_shape(&path))
        .collect();
    client_paths_outside(&described)
}

/// Every `/api` path the browser asks for, less the ones `served` covers and
/// the ones the allow-list exempts, as `module  /path` lines.
fn client_paths_outside(served: &BTreeSet<String>) -> Vec<String> {
    let exempt: BTreeSet<String> = PATHS_OUTSIDE_THE_DOCUMENT
        .iter()
        .map(|(path, _)| path_shape(path))
        .collect();

    let mut outside: Vec<String> = Vec::new();
    for (module, least) in CLIENT_API_MODULES {
        let source = std::fs::read_to_string(web_src(module))
            .unwrap_or_else(|error| panic!("the test reads {module}: {error}"));
        let called = client_api_paths(&source);
        // A resolver that stops resolving finds nothing and proves nothing.
        assert!(
            called.len() >= *least,
            "the scan found only {} `/api` paths in {module}, and expected at \
             least {least}; the literal scan or the prefix resolver has broken",
            called.len()
        );
        outside.extend(
            called
                .into_iter()
                .filter(|path| !served.contains(path) && !exempt.contains(path))
                .map(|path| format!("{module}  {path}")),
        );
    }
    outside.sort();
    outside
}

/// Every `/api` path the browser asks for has an operation in the document.
///
/// This runs the direction the deleted regex scan ran, and fixes what it
/// missed: it compares against the generated document rather than a second
/// regex scan of the same Rust, and it reads `review-api.ts`.
#[test]
fn every_api_path_the_client_calls_is_in_the_document() {
    let missing = undescribed_client_paths();

    assert!(
        missing.is_empty(),
        "{} `/api` paths the browser calls have no OpenAPI operation. Give \
         the route a `#[utoipa::path]`, or fix the client path:\n  - {}",
        missing.len(),
        missing.join("\n  - ")
    );
}

/// Every `/api` path the browser asks for reaches a route.
///
/// This is the claim the deleted regex scan made, and the one that does not
/// read the document alone: a route the document leaves out still serves its
/// path. It compares against the nest-resolved scan joined to the document, so
/// an allow-listed path reads the same as a described one.
///
/// It also holds the scan honest. A scan that quietly stopped reading a file
/// would shorten the other two gates' missing lists, which looks like
/// progress; here the same loss names the paths that suddenly reach nothing.
#[test]
fn every_api_path_the_client_calls_reaches_a_route() {
    let registered = routes_the_router_serves();

    // The allow-list is the route scan's own sanity check. Every entry is a
    // `.route(…)` call that stays outside the document, so a scan that stops
    // finding them has broken, and the other two gates would pass for the
    // wrong reason. The check lives here because this test compares against
    // the scan rather than against the document alone.
    for (path, reason) in PATHS_OUTSIDE_THE_DOCUMENT {
        assert!(
            registered.iter().any(|(_, found)| found == path),
            "the scan no longer finds `{path}` ({reason}); \
             fix the scan or drop the allow-list entry"
        );
    }

    let served: BTreeSet<String> = registered
        .into_iter()
        .chain(documented_operations())
        .map(|(_, path)| path_shape(&path))
        .collect();
    let missing = client_paths_outside(&served);

    assert!(
        missing.is_empty(),
        "{} `/api` paths the browser calls reach no route. Add the route, fix \
         the client path, or fix the scan that reads them:\n  - {}",
        missing.len(),
        missing.join("\n  - ")
    );
}
