//! The OpenAPI document that the axum router generates.
//!
//! Task A1 proves the chain on one route, `GET /api/models`: the `ToSchema`
//! derive, the `#[utoipa::path]` attribute, the `OpenApiRouter`, this test and
//! the committed `openapi.json`. Later tasks add the other routes.
//!
//! Task A11 adds the streams: every SSE route names `text/event-stream` and
//! the event union it carries, and the browser's event-name lists are compared
//! against those unions in both directions.
//!
//! To regenerate the committed document, run:
//! `cargo test -p crucible-web --test openapi_contract -- --ignored write_openapi_json`

use std::path::{Path, PathBuf};

use crucible_web::server::api_spec;

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

/// The writer, not a test. `--ignored` keeps it out of the normal run.
#[test]
#[ignore = "writer: it regenerates crates/crucible-web/openapi.json"]
fn write_openapi_json() {
    let path = openapi_json_path();
    let mut document = serde_json::to_string_pretty(&api_spec()).expect("the spec serialises");
    document.push('\n');
    std::fs::write(&path, document).expect("the writer writes openapi.json");
    println!("wrote {}", path.display());
}

/// The path of a TypeScript source file under `crates/crucible-web/web`.
fn web_src(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("web/src")
        .join(relative)
}

/// Read a `const NAME = ['a', 'b'] as const;` array of strings out of a
/// TypeScript source file.
///
/// A regex, not a parser: the two lists this test reads are literal string
/// arrays with no interpolation, and a TypeScript parser in a Rust test costs
/// more than it proves.
fn typescript_string_list(source: &str, name: &str) -> Vec<String> {
    let opening = format!("{name} = [");
    let start = source
        .find(&opening)
        .unwrap_or_else(|| panic!("`{name}` is not declared in the file"))
        + opening.len();
    let length = source[start..]
        .find(']')
        .unwrap_or_else(|| panic!("`{name}` has no closing bracket"));

    source[start..start + length]
        .split(',')
        .map(|entry| {
            entry
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .to_string()
        })
        .filter(|entry| !entry.is_empty())
        .collect()
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

/// The browser's event-name lists and the Rust enums say the same thing.
///
/// The check runs both ways on purpose. `api.ts` asked a human to append a
/// name whenever a `ChatEvent` variant arrived; a name in Rust and not in
/// TypeScript is the failure that comment could not catch.
#[test]
fn every_sse_event_name_is_in_the_document() {
    let spec = spec_json();
    let api_ts = std::fs::read_to_string(web_src("lib/api.ts")).expect("the test reads api.ts");

    let chat_names: Vec<String> = typescript_string_list(&api_ts, "SSE_EVENT_TYPES");
    assert_eq!(chat_names.len(), 22, "`SSE_EVENT_TYPES` lost a name");
    let chat_tags = discriminator_values(&spec["components"]["schemas"]["ChatEvent"]);
    let mut expected = chat_names.clone();
    expected.sort();
    expected.dedup();
    assert_eq!(
        chat_tags, expected,
        "`ChatEvent` and `SSE_EVENT_TYPES` name different events"
    );

    // The filesystem stream's SSE `event:` names carry an `fs_` prefix that the
    // serde tag does not: `fs_changed` on the wire envelope, `changed` in the
    // payload. `FsEvent::event_name` writes the prefix, so the test strips it.
    let fs_names: Vec<String> = typescript_string_list(&api_ts, "FS_SSE_EVENT_TYPES")
        .iter()
        .map(|name| {
            name.strip_prefix("fs_")
                .unwrap_or_else(|| panic!("`{name}` is not an `fs_` event name"))
                .to_string()
        })
        .collect();
    assert_eq!(fs_names.len(), 3, "`FS_SSE_EVENT_TYPES` lost a name");
    let fs_tags = discriminator_values(&spec["components"]["schemas"]["FsEvent"]);
    let mut expected = fs_names.clone();
    expected.sort();
    expected.dedup();
    assert_eq!(
        fs_tags, expected,
        "`FsEvent` and `FS_SSE_EVENT_TYPES` name different events"
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
