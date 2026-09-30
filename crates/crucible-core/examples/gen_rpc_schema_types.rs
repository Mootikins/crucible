//! Generate the schema list `crucible-web` merges into its OpenAPI document.
//!
//! `utoipa` emits a schema only for a type at least one live web route names
//! in a `#[utoipa::path]` attribute — the params/reply type of an
//! `rpc_methods!` row that no route touches yet has no entry in
//! `api-schema.d.ts`, so `gen_rpc_methods_ts.rs` calls it `unknown` even
//! though the Rust side already names a real type. This prints one
//! `#[derive(utoipa::OpenApi)]` struct that lists every row's named
//! params/reply types under `components(schemas(...))`
//! ([`crucible_core::protocol::rpc::type_text::render_schema_types_file`]).
//! `crucible-web`'s `api_spec` merges that document's schemas into the
//! router's own, so a row's type gets a schema whether or not a route uses
//! it too — and a row whose type has no `ToSchema` fails this generated
//! file's build, because `schemas(...)` requires the trait.
//!
//! Usage: `cargo run -p crucible-core --features openapi --example gen_rpc_schema_types`.
//! Writes to stdout; the caller redirects it into
//! `crates/crucible-core/src/protocol/rpc/schema_types.rs`.

fn main() {
    print!(
        "{}",
        crucible_core::protocol::rpc::type_text::render_schema_types_file()
    );
}
