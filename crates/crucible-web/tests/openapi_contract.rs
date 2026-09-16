//! The OpenAPI document that the axum router generates.
//!
//! Task A1 proves the chain on one route, `GET /api/models`: the `ToSchema`
//! derive, the `#[utoipa::path]` attribute, the `OpenApiRouter`, this test and
//! the committed `openapi.json`. Later tasks add the other routes.
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
