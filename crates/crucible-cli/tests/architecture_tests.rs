//! Architecture invariant gates for the CLI/TUI crate (source-scan tests).
//!
//! Companion to `crucible-daemon/tests/architecture_tests.rs`. These encode
//! TUI-side invariants CLAUDE.md states in prose:
//!   A2a — every `ChatAppMsg` variant is handled somewhere (no dead messages).
//!   A2b — canonical parser types are defined only in crucible-core/parser.
//!
//! Source-scan style: read files and match, so they are fast and build-free.
//! When one fails, fix the code, not the test — see each failure message.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::WalkDir;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("Cargo.toml").is_file() && p.join("crates").is_dir())
        .expect("workspace root (dir containing crates/ and Cargo.toml)")
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The text between the first `{` following `header` and its matching `}`.
/// Ignores braces inside string literals and `//` line comments.
fn braced_body(src: &str, header: &str) -> String {
    let start = src
        .find(header)
        .unwrap_or_else(|| panic!("header not found: {header}"));
    let open = start + src[start..].find('{').expect("opening brace");
    let bytes = src.as_bytes();
    let (mut depth, mut in_str, mut escaped, mut i) = (0usize, false, false, open);
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_str = false,
                _ => {}
            }
            i += 1;
            continue;
        }
        if c == '/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return src[open..=i].to_string();
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("unbalanced braces after: {header}");
}

// ===========================================================================
// A2a — ChatAppMsg variant handling parity.
//
// Every variant of the daemon↔TUI message enum must be referenced in a
// handler file (chat_runner/ or a chat_app handler other than the definition
// itself). A variant that appears nowhere is a message no one handles — the
// class of bug where a feature is wired into the enum but silently dropped.
//
// View-only variants that legitimately have no handler go in
// VIEW_ONLY_VARIANTS with a justification. It is currently empty: every
// variant is handled.
// ===========================================================================

/// Variants intentionally not handled in chat_runner/chat_app (e.g. consumed
/// only by a renderer). Each entry needs a `// why:` justification. Empty by
/// design — add here only with a reason, never to silence a real gap.
const VIEW_ONLY_VARIANTS: &[&str] = &[
    // (none)
];

fn chat_app_msg_variants(root: &Path) -> BTreeSet<String> {
    let src = read(&root.join("crates/crucible-cli/src/tui/oil/chat_app/messages.rs"));
    let body = braced_body(&src, "pub enum ChatAppMsg");
    // Top-level variants sit at 4-space indent; struct-variant fields are
    // deeper, so a line-anchored 4-space + CamelCase match picks out variants.
    let re = Regex::new(r"(?m)^    ([A-Z][A-Za-z0-9]+)\s*[({,]").unwrap();
    re.captures_iter(&body).map(|c| c[1].to_string()).collect()
}

fn handler_sources(root: &Path) -> Vec<String> {
    let dirs = [
        root.join("crates/crucible-cli/src/tui/oil/chat_runner"),
        root.join("crates/crucible-cli/src/tui/oil/chat_app"),
    ];
    let mut out = Vec::new();
    for dir in dirs {
        for entry in WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            // The enum definition is not a handler.
            if p.file_name().and_then(|n| n.to_str()) == Some("messages.rs") {
                continue;
            }
            out.push(read(p));
        }
    }
    out
}

#[test]
fn every_chat_app_msg_variant_is_handled() {
    let root = workspace_root();
    let variants = chat_app_msg_variants(&root);
    assert!(
        variants.len() > 40,
        "sanity: expected to parse many ChatAppMsg variants, got {}",
        variants.len()
    );
    let handlers = handler_sources(&root);
    let allow: BTreeSet<&str> = VIEW_ONLY_VARIANTS.iter().copied().collect();

    let mut unhandled = Vec::new();
    for v in &variants {
        if allow.contains(v.as_str()) {
            continue;
        }
        let re = Regex::new(&format!(r"\b{}\b", regex::escape(v))).unwrap();
        if !handlers.iter().any(|h| re.is_match(h)) {
            unhandled.push(v.clone());
        }
    }

    // Guard the allowlist against rot: an entry that no longer exists as a
    // variant should be removed.
    let stale: Vec<&str> = allow
        .iter()
        .copied()
        .filter(|a| !variants.contains(*a))
        .collect();

    assert!(
        unhandled.is_empty() && stale.is_empty(),
        "ChatAppMsg handling parity:\n  unhandled variants (add a handler in \
         chat_runner/ or chat_app, or an allowlisted view-only entry with a \
         reason): {unhandled:?}\n  stale allowlist entries (remove): {stale:?}"
    );
}

// ===========================================================================
// A2b — canonical parser types live only in crucible-core/parser.
//
// ParsedNote / Wikilink / Tag / BlockHash have exactly one definition site.
// Re-defining them elsewhere (even a local shim) is the duplicate-type
// anti-pattern CLAUDE.md bans.
// ===========================================================================

const CANONICAL_PARSER_TYPES: &[&str] = &["ParsedNote", "Wikilink", "Tag", "BlockHash"];
const CANONICAL_HOME: &str = "crates/crucible-core/src/parser/";

#[test]
fn canonical_parser_types_are_not_redefined() {
    let root = workspace_root();
    let alt = CANONICAL_PARSER_TYPES.join("|");
    let re = Regex::new(&format!(r"\b(?:struct|enum)\s+(?:{alt})\b")).unwrap();

    let mut offenders = Vec::new();
    for entry in WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let rel = p
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.contains("/src/") || rel.contains(CANONICAL_HOME) {
            continue;
        }
        for m in re.find_iter(&read(p)) {
            offenders.push(format!("{rel}: {}", m.as_str()));
        }
    }
    assert!(
        offenders.is_empty(),
        "Canonical parser types may only be defined in {CANONICAL_HOME}. Use the \
         crucible-core re-export instead of redefining:\n  - {}",
        offenders.join("\n  - ")
    );
}

// ===========================================================================
// A2c — every /api path the web frontend calls exists as a backend route.
// This mismatch class shipped twice (generate-title vs auto-title 405,
// /api/layout with no backend route at all): the frontend degrades silently,
// so nothing but a console warning catches it. Source-scan both sides.
// ===========================================================================

/// `${...}` interpolations and `{param}` segments both normalize to `{}` so
/// the two sides compare structurally. Query strings are stripped. Adjacent
/// interpolations collapse (`/api/plugins/${name}${query}` → `/api/plugins/{}`
/// — the trailing one is a conditionally-appended query suffix).
fn normalize_api_path(raw: &str) -> String {
    let no_query = raw.split('?').next().unwrap_or(raw);
    let re = Regex::new(r"\$\{[^}]*\}|\{[^}]*\}").unwrap();
    let braced = re.replace_all(no_query, "{}").to_string();
    let mut collapsed = braced;
    while collapsed.contains("{}{}") {
        collapsed = collapsed.replace("{}{}", "{}");
    }
    collapsed.trim_end_matches('/').to_string()
}

fn frontend_api_paths(root: &Path) -> BTreeSet<String> {
    let src = read(&root.join("crates/crucible-web/web/src/lib/api.ts"));
    let re = Regex::new(r#"['"`](/api/[^'"`]*)['"`]"#).unwrap();
    re.captures_iter(&src)
        .map(|c| normalize_api_path(&c[1]))
        .collect()
}

fn backend_api_paths(root: &Path) -> BTreeSet<String> {
    let route_re = Regex::new(r#"\.route\(\s*"([^"]+)""#).unwrap();
    let nest_re = Regex::new(r#"\.nest\(\s*"([^"]+)""#).unwrap();

    let mut sources = Vec::new();
    let routes_dir = root.join("crates/crucible-web/src/routes");
    for entry in WalkDir::new(&routes_dir).into_iter().filter_map(Result::ok) {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("rs") {
            sources.push(read(entry.path()));
        }
    }
    sources.push(read(&root.join("crates/crucible-web/src/server.rs")));

    let mut absolute = BTreeSet::new();
    let mut relative = BTreeSet::new();
    let mut nest_prefixes = BTreeSet::new();
    for src in &sources {
        for c in route_re.captures_iter(src) {
            let path = normalize_api_path(&c[1]);
            if path.starts_with("/api") {
                absolute.insert(path);
            } else {
                relative.insert(path);
            }
        }
        for c in nest_re.captures_iter(src) {
            nest_prefixes.insert(normalize_api_path(&c[1]));
        }
    }
    // Routers mounted via .nest() register relative paths; join every relative
    // path with every nest prefix. Over-approximates (harmless: this set is
    // only checked for membership), avoids resolving which router nests where.
    for prefix in &nest_prefixes {
        for rel in &relative {
            absolute.insert(format!("{prefix}{rel}"));
        }
    }
    absolute
}

#[test]
fn every_frontend_api_path_has_a_backend_route() {
    let root = workspace_root();
    let frontend = frontend_api_paths(&root);
    let backend = backend_api_paths(&root);

    assert!(
        frontend.len() >= 20,
        "extraction sanity check: expected 20+ /api paths in api.ts, found {} — \
         the scan regex probably broke, fix the test",
        frontend.len()
    );

    let missing: Vec<_> = frontend.difference(&backend).cloned().collect();
    assert!(
        missing.is_empty(),
        "web/src/lib/api.ts calls /api paths that no backend route serves \
         (routes/*.rs + server.rs). Add the route or fix the frontend path:\n  - {}",
        missing.join("\n  - ")
    );
}
