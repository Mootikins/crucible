//! Check whether the SolidJS frontend has been built. Never build it.
//!
//! `web/dist` is a bundler artifact and is gitignored, so it is absent in a fresh
//! clone. `assets.rs` embeds it with rust-embed and `allow_missing = true`, so a
//! build without it succeeds and serves a placeholder — that is what `1dff5c28b`
//! bought, after a hard-erroring derive broke `cargo install --git` for every new
//! user.
//!
//! An earlier version of this script ran `bun run build` when a toolchain was
//! present, so that `cargo install --git` would produce a working UI. That was
//! reverted, for reasons that outweigh the convenience:
//!
//! - **It writes outside `OUT_DIR`.** The Cargo book is explicit that build
//!   scripts "should not modify any files outside of that directory", and under
//!   `cargo install --git` the source tree is a shared, cargo-managed checkout in
//!   `$CARGO_HOME/git/checkouts`. maplibre/martin hit the enforced version of
//!   this as "Source directory was modified by build.rs during cargo publish".
//! - **It punished the better-equipped user.** No toolchain got the placeholder;
//!   a toolchain got `bun run build` inside a checkout with no `node_modules`,
//!   which then wanted a network install into cargo's own directory.
//! - **The npm fallback was unsound.** With only `bun.lock` and no
//!   `package-lock.json` it resolved unpinned ranges at install time, on someone
//!   else's machine, inside a command they ran with `--locked`.
//! - **Bundlers in build scripts are a maintenance tail.** `rerun-if-changed` on
//!   a JS package directory also watches `node_modules` and `dist`, which the
//!   build itself writes, so it needs an exclusion list to avoid rebuild storms.
//!
//! So the frontend is built by `just web-build` for development and by
//! cargo-dist's `github-build-setup` hook for releases. This script only reports
//! what happened, and fails loudly when a build that *must* ship the UI has not
//! had it built.

use std::path::PathBuf;

/// Set in any build that must not ship without the web UI — release CI, and
/// cargo-dist's build environment. Absent for an ordinary `cargo build`, which
/// must keep working with no JS toolchain.
const REQUIRE: &str = "CRUCIBLE_REQUIRE_WEB_UI";

fn main() {
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("web")
        .join("dist");

    // Watch `dist`, not `web/src`: watching sources would imply cargo rebuilds
    // the bundle when they change, which it does not, and would drag
    // `node_modules` into the fingerprint.
    println!("cargo:rerun-if-changed={}", dist.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed={REQUIRE}");

    let embedded = dist.join("index.html").is_file();

    // An env var rather than a `cfg`, to stay MSRV-safe: `rustc-check-cfg` needs
    // a newer cargo than this crate supports, and an unchecked `rustc-cfg` trips
    // `unexpected_cfgs`.
    println!(
        "cargo:rustc-env=CRUCIBLE_WEB_UI_EMBEDDED={}",
        u8::from(embedded)
    );

    if embedded {
        return;
    }

    if std::env::var_os(REQUIRE).is_some() {
        panic!(
            "{REQUIRE} is set but {} does not exist.\n\
             This build is meant to ship the web UI. Build the frontend first:\n\
             \n    just web-build\n\n\
             or `cd crates/crucible-web/web && bun install && bun run build`.",
            dist.display()
        );
    }

    println!(
        "cargo:warning=web/dist is absent, so `cru web` will serve a placeholder. \
         Run `just web-build` to compile the UI into this binary, or install a \
         release build."
    );
}
