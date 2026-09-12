//! Architecture invariant gates for the CLI/TUI crate (source-scan tests).
//!
//! Companion to `crucible-daemon/tests/architecture_tests.rs`. These encode
//! TUI-side invariants CLAUDE.md states in prose:
//!   A2a — every `ChatAppMsg` variant is handled somewhere (no dead messages).
//!   A2b — canonical parser types are defined only in crucible-core/parser.
//!   A2c — every `/api` path the web frontend calls has a backend route.
//!   A2d — the CLI does not build its own knowledge-base context block.
//!   A2e — every session config knob the daemon advertises has a web route.
//!   A2f — nobody hand-rolls the "is this file markdown" predicate.
//!   A2g — the hook counts AGENTS.md prints match `StageId`/`EventName`.
//!
//! A2* live here rather than in the daemon's companion file because they scan
//! CLI and web source; the daemon's header lists A1/A3/A4/A5 for the same
//! reason.
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

// UNIQUE: clippy's dead_code only fires on never-constructed variants; ChatAppMsg variants are typically constructed in serialization but the gate verifies they're *handled* in chat_runner/chat_app. No clippy rule cross-references enum variants against handler-site regex matches.
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

// UNIQUE: Rust permits identically-named structs/enums in different modules; no clippy rule bans redefining ParsedNote/Wikilink/Tag/BlockHash outside their canonical home. The cross-tree regex scan is the only enforcement.
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

// UNIQUE: TS types don't see /api paths constructed at runtime; clippy/types cannot cross the Rust↔SolidJS boundary. The source-scan compares frontend string literals to backend route declarations structurally.
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

// ===========================================================================
// A2d — the CLI must not build its own knowledge-base context block.
//
// `context_enricher.rs` did this and shipped alongside the daemon's
// Precognition, so `cru chat -q` ran BOTH: the CLI prepended a block, then the
// daemon searched again using that block as its query text. Grounding is
// daemon business logic (Systems.md: "Owns all business logic that views
// consume over RPC") and there is exactly one implementation.
// ===========================================================================

/// Marker strings that only a client-side context-block builder would contain.
const CLIENT_SIDE_ENRICHMENT_MARKERS: &[&str] = &[
    "# Context from Knowledge Base",
    "Context from Knowledge Base (Reranked)",
];

// UNIQUE: no type or lint can express "this crate must not format a retrieval
// result into a prompt" — the duplicate implementation compiled cleanly and
// passed its own tests for as long as it existed. Source-scan is the seam.
#[test]
fn the_cli_does_not_build_its_own_context_block() {
    let root = workspace_root();
    let mut offenders = Vec::new();
    for entry in WalkDir::new(root.join("crates/crucible-cli/src"))
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.path().extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let contents = read(entry.path());
        for marker in CLIENT_SIDE_ENRICHMENT_MARKERS {
            if contents.contains(marker) {
                offenders.push(format!(
                    "{}: contains {marker:?}",
                    entry.path().strip_prefix(&root).unwrap().display()
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "The CLI is formatting knowledge-base context into a prompt. Grounding \
         belongs to the daemon (agent_manager/precognition/); the CLI's job is \
         to set `session.set_precognition` and render the \
         `precognition_complete` event:\n  - {}",
        offenders.join("\n  - ")
    );
}

// ===========================================================================
// A2e — every session config knob the daemon advertises is reachable from the
// web.
//
// Nine of fifteen were not, and nothing failed: the daemon grows a knob, the
// TUI wires it (`tui/oil/config/shortcuts.rs`), and the web silently falls a
// knob further behind. A1 in the daemon's companion file already gates
// client↔server field-name parity for all fifteen; daemon↔web was the only
// ungated axis, and it is a different failure (a route that does not exist at
// all, rather than a field name that disagrees).
//
// The ledger below is SHRINK-ONLY: a NEW knob is not in it and so fails
// immediately.
// ===========================================================================

fn captures(re: &str, hay: &str) -> BTreeSet<String> {
    let re = Regex::new(re).unwrap();
    re.captures_iter(hay).map(|c| c[1].to_string()).collect()
}

/// `session.set_<suffix>` → the `/api/session/{}/config/<path>` tail.
///
/// Declared rather than derived: a knob's route is not always its name in
/// kebab case, so a naive snake→kebab transform would need special-casing.
const WEB_CONFIG_ROUTES: &[(&str, &str)] = &[
    ("context_strategy", "context-strategy"),
    ("precognition", "precognition"),
    // Not a Crucible knob: the settings the external agent advertised for
    // itself. One route serves both directions — GET lists them, POST sets one
    // — because the value belongs to the agent and is read back from its list.
    ("agent_option", "agent-options"),
];

/// Knobs with no web route yet. REMOVE entries as routes land; never add.
///
/// Empty: all fifteen advertised knobs are reachable from the web. It held nine
/// when this gate landed, and the gate is what forced each row out — adding a
/// route while leaving its row here fails just as loudly as the reverse, which
/// is how the ledger stays a record of work outstanding rather than of work
/// forgotten.
const WEB_ROUTE_LEDGER: &[&str] = &[];

/// Exempt permanently, with a reason: `mode` is not a `config/` knob. It has
/// its own `POST /api/session/{id}/mode` and `GET .../modes`, because switching
/// mode changes tool policy rather than a scalar setting.
const WEB_ROUTE_EXEMPT: &[&str] = &["mode"];

/// `session.set_*` methods that mutate session SCOPE rather than configure the
/// agent. They share the prefix but are not knobs, and neither belongs under
/// `config/`.
const SCOPE_MUTATIONS: &[&str] = &["title", "workspace"];

// UNIQUE: the daemon's METHODS list and the web's axum Router are in different
// crates with no shared type; a knob present in one and absent from the other is
// not a compile error, and A1 only compares the daemon's own client to its own
// server. Route EXISTENCE, not field names — field names are already A1's job,
// which matters because `execution_timeout`'s wire field is `timeout_secs` and a
// web struct named after the knob would compile, pass review, and drop the value.
#[test]
fn every_rpc_session_knob_is_reachable_from_the_web() {
    let root = workspace_root();
    let dispatch = read(&root.join("crates/crucible-daemon/src/rpc/dispatch.rs"));

    // Knobs the daemon advertises, read from METHODS itself — the same list
    // `daemon.capabilities` returns, so this is a client's view of the surface.
    let advertised = captures(r#""session\.set_([a-z0-9_]+)""#, &dispatch);
    let scope: BTreeSet<String> = SCOPE_MUTATIONS.iter().map(|s| s.to_string()).collect();
    let advertised: BTreeSet<String> = advertised.difference(&scope).cloned().collect();

    // Sanity check on the scan, not on the knob count — the count is meant to
    // shrink. `precognition` is Crucible's own retrieval knob and is not going
    // anywhere, so its absence means the regex broke rather than that a knob
    // was deleted.
    assert!(
        advertised.contains("precognition"),
        "the scan found no `session.set_precognition`, so the regex probably \
         broke; it found: {advertised:?}"
    );

    let mapped: BTreeSet<String> = WEB_CONFIG_ROUTES
        .iter()
        .map(|(k, _)| k.to_string())
        .collect();
    let exempt: BTreeSet<String> = WEB_ROUTE_EXEMPT.iter().map(|s| s.to_string()).collect();
    let ledger: BTreeSet<&str> = WEB_ROUTE_LEDGER.iter().copied().collect();

    let mut failures = Vec::new();

    // (1) Table completeness: every advertised knob is mapped or exempt.
    let covered: BTreeSet<String> = mapped.union(&exempt).cloned().collect();
    for knob in advertised.difference(&covered) {
        failures.push(format!(
            "session.set_{knob} has no WEB_CONFIG_ROUTES row — add the route tail, \
             or add it to WEB_ROUTE_EXEMPT with a reason"
        ));
    }
    // (2) Table staleness: no row for a knob the daemon dropped.
    for stale in mapped.difference(&advertised) {
        failures.push(format!(
            "WEB_CONFIG_ROUTES row `{stale}` is not in METHODS — remove the row or \
             restore the knob"
        ));
    }

    // (3) The routes actually exist in the axum Router.
    let backend = backend_api_paths(&root);
    for (knob, tail) in WEB_CONFIG_ROUTES {
        let path = format!("/api/session/{{}}/config/{tail}");
        let present = backend.contains(&path);
        let ledgered = ledger.contains(*knob);
        if !present && !ledgered {
            failures.push(format!(
                "session.set_{knob}: no web route at {path}. Add it under \
                 crucible-web/src/routes/session_config/, register it in \
                 routes/session/mod.rs, or (temporarily) add `{knob}` to \
                 WEB_ROUTE_LEDGER"
            ));
        }
        if present && ledgered {
            failures.push(format!(
                "session.set_{knob}: route {path} now exists — REMOVE `{knob}` from \
                 WEB_ROUTE_LEDGER (the ledger only shrinks)"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "RPC↔web session-knob parity violations:\n  - {}",
        failures.join("\n  - ")
    );
}

/// `session.set_<suffix>` → the `:set` key that reaches it.
///
/// Declared rather than derived, like `WEB_CONFIG_ROUTES`: the TUI spells most
/// keys without underscores, and some carry an alias for both spellings. No
/// transform covers that, and a knob whose key differs between the two front
/// ends is worth stating once here.
const TUI_SET_KEYS: &[(&str, &str)] = &[
    ("context_strategy", "contextstrategy"),
    ("precognition", "precognition"),
];

/// Knobs the TUI cannot set at all. REMOVE entries as keys land; never add.
///
/// `agent_option` is the live gap. `session.list_agent_options` and
/// `session.set_agent_option` project the settings an external agent
/// advertises for itself, and only the web reads them, so a TUI user talking
/// to an ACP agent cannot see or change what that agent offers.
const TUI_KEY_LEDGER: &[&str] = &["agent_option"];

/// Exempt permanently, with a reason: `mode` has Shift-Tab and `:mode`, and
/// switching it changes tool policy rather than a scalar setting.
const TUI_KEY_EXEMPT: &[&str] = &["mode"];

/// A knob the daemon advertises must be reachable from the TUI as well as the
/// web.
///
/// AGENTS.md asks "Where does a user meet it? TUI *and* web", and the web half
/// already has a gate above. Without this half a knob can ship to one renderer
/// and pass review — which is what happened to `agent_option`.
///
/// Derived, not grepped: it calls the real `:set` classifier, so a row cannot
/// be satisfied by a string appearing somewhere in the file. Any error but
/// `UnknownKey` proves the key is wired; the sample value is deliberately not
/// valid for every key, because validity is the classifier's business.
#[test]
fn every_rpc_session_knob_is_reachable_from_the_tui() {
    use crucible_cli::tui::oil::commands::{classify_set_value, SetError};

    let root = workspace_root();
    let dispatch = read(&root.join("crates/crucible-daemon/src/rpc/dispatch.rs"));
    let advertised = captures(r#""session\.set_([a-z0-9_]+)""#, &dispatch);
    let scope: BTreeSet<String> = SCOPE_MUTATIONS.iter().map(|s| s.to_string()).collect();
    let advertised: BTreeSet<String> = advertised.difference(&scope).cloned().collect();

    // See the web gate: a count assertion would fail every time a knob is
    // deliberately removed. `precognition` staying is the real signal.
    assert!(
        advertised.contains("precognition"),
        "the scan found no `session.set_precognition`, so the regex probably \
         broke; it found: {advertised:?}"
    );

    let mapped: BTreeSet<String> = TUI_SET_KEYS.iter().map(|(k, _)| k.to_string()).collect();
    let exempt: BTreeSet<String> = TUI_KEY_EXEMPT.iter().map(|s| s.to_string()).collect();
    let ledger: BTreeSet<String> = TUI_KEY_LEDGER.iter().map(|s| s.to_string()).collect();
    let mut failures = Vec::new();

    // (1) Table completeness: every advertised knob is mapped, exempt or ledgered.
    let covered: BTreeSet<String> = mapped
        .union(&exempt)
        .cloned()
        .collect::<BTreeSet<_>>()
        .union(&ledger)
        .cloned()
        .collect();
    for knob in advertised.difference(&covered) {
        failures.push(format!(
            "session.set_{knob} has no TUI_SET_KEYS row — add the `:set` key, mark it \
             TUI_KEY_EXEMPT with a reason, or (temporarily) add it to TUI_KEY_LEDGER"
        ));
    }
    // (2) Table staleness: no row for a knob the daemon dropped.
    for stale in mapped
        .union(&ledger)
        .cloned()
        .collect::<BTreeSet<_>>()
        .difference(&advertised)
    {
        failures.push(format!(
            "TUI row `{stale}` is not in METHODS — remove the row or restore the knob"
        ));
    }
    // (3) The keys are real: the running classifier knows each one.
    for (knob, key) in TUI_SET_KEYS {
        if matches!(
            classify_set_value(key.to_string(), "1".to_string()),
            Err(SetError::UnknownKey(_))
        ) {
            failures.push(format!(
                "session.set_{knob}: `:set {key}=…` is an unknown key. Add an arm to \
                 `classify_set_value` in tui/oil/commands/set.rs, or fix the row"
            ));
        }
    }
    // (4) A ledgered knob really is unreachable, so the ledger only shrinks.
    for knob in &ledger {
        if !matches!(
            classify_set_value(knob.clone(), "1".to_string()),
            Err(SetError::UnknownKey(_))
        ) {
            failures.push(format!(
                "session.set_{knob}: `:set {knob}` now works — move it from \
                 TUI_KEY_LEDGER into TUI_SET_KEYS"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "RPC↔TUI session-knob parity violations:\n  - {}",
        failures.join("\n  - ")
    );
}

// ===========================================================================
// A2f — one markdown predicate.
//
// `KilnFileKind::of` (crucible-core/src/kiln.rs) is the only thing allowed to
// know which extensions are notes. Fourteen call sites used to answer it
// themselves, in four mutually-inconsistent ways, so `Reading List.markdown`
// was indexed, searchable and live-previewed by the daemon while `cru stats`,
// `cru kiln validate`, `cru workflow` and `cru process --watch` all reported
// that it did not exist. Nothing failed: each copy was locally correct.
//
// Two families deliberately do NOT match, because they answer a different
// question and unifying them would be a bug:
//   - stem-stripping (`strip_suffix(".md")`, `trim_end_matches(".md")`) is
//     wikilink *resolution*, governed by Obsidian's stem rules;
//   - `.md`-*appending* (`ensure_md_suffix`, `note_refactor.rs`, `acp/tools.rs`)
//     answers "what extension do we create", which stays `.md` even though the
//     recognizer accepts `.markdown`.
// Both spell the extension WITH a leading dot, so every pattern below matches
// only the bare `"md"` form. That is what keeps this gate's allowlist small
// instead of enumerating a dozen legitimate sites.
// ===========================================================================

/// Bare-extension comparison forms. None may contain `".md"` with a leading
/// dot — see the header: that would flag every `ends_with(".md")` path-builder.
const MARKDOWN_PREDICATE_PATTERNS: &[&str] = &[
    r#"==\s*Some\("md"\)"#,
    r#"==\s*"md""#,
    r#"eq_ignore_ascii_case\("md"\)"#,
    r#"matches!\([^)]*"md""#,
    r#"vec!\["md""#,
    r#"\["md""#,
];

/// The canonical home, skipped: it is where the answer lives.
const MARKDOWN_PREDICATE_HOME: &str = "crates/crucible-core/src/kiln.rs";

/// Files that still hold a copy. SHRINK-ONLY, like WEB_ROUTE_LEDGER: an entry
/// whose file has stopped matching fails just as loudly as a new copy, so the
/// ledger cannot outlive the work it records.
///
/// Currently empty, which is the goal state: every Rust caller asks
/// `crucible_core::is_note_file` / `is_indexable_file`. The last row was
/// `watch/handlers/parser_handler.rs`, dead code reachable only from its own
/// tests, deleted rather than migrated. Add a row only to record work you are
/// deliberately deferring, and say why.
const MARKDOWN_PREDICATE_LEDGER: &[&str] = &[];

/// Frontend files allowed to name markdown extensions.
///
/// Permanent, not a ledger: `lib/markdown-path.ts` is the frontend's canonical
/// home (the one duplicate of the Rust predicate that has to exist, because
/// TypeScript cannot call Rust and asking the daemon per keystroke is not an
/// option), and `lib/file-icons.ts` maps extensions to glyphs — cosmetic, and
/// it deliberately gives `.mdx`/`.mdc` a document icon without claiming they
/// are editable notes.
const FRONTEND_MARKDOWN_HOMES: &[&str] = &[
    "crates/crucible-web/web/src/lib/markdown-path.ts",
    "crates/crucible-web/web/src/lib/file-icons.ts",
];

/// Predicate forms only. Narrow on purpose: `=== 'md'` and the `/\.(md|
/// markdown)$/i` regex are how the copies were spelled, whereas a bare `'md'`
/// would flag `IconButton`'s `size?: 'sm' | 'md'` union and
/// `CodeMirrorEditor`'s `case 'md':` grammar switch, neither of which is a note
/// predicate.
const FRONTEND_PREDICATE_PATTERNS: &[&str] = &[
    r"\\\.\(md\|markdown\)\$/i?\.test\(",
    r"===\s*'md'",
    r#"===\s*"md""#,
];

/// `src` with `//` line comments removed, string literals respected.
///
/// Prose about the predicate is not a copy of it: `kiln_validate.rs` documents
/// itself as using "the canonical predicate rather than an inline `ext ==
/// "md"`", and flagging that sentence would teach the next author to describe
/// the fix less honestly rather than to make it.
fn without_line_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let bytes = line.as_bytes();
        let (mut in_str, mut escaped, mut i) = (false, false, 0usize);
        let mut cut = line.len();
        while i < bytes.len() {
            let c = bytes[i];
            if in_str {
                match (escaped, c) {
                    (true, _) => escaped = false,
                    (false, b'\\') => escaped = true,
                    (false, b'"') => in_str = false,
                    _ => {}
                }
            } else if c == b'"' {
                in_str = true;
            } else if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
                cut = i;
                break;
            }
            i += 1;
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// Every `crates/*/src/**/*.rs` path, as a workspace-relative `/`-joined string.
fn crate_source_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for entry in WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let rel = p
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.contains("/src/") {
            continue;
        }
        out.push((rel, p.to_path_buf()));
    }
    out
}

// UNIQUE: every copy of this predicate compiles, passes review and is locally
// correct — `extension() == Some("md")` is not a lint violation, it is just a
// different answer to a question that must have one. No type can express "ask
// KilnFileKind", and the Rust↔TypeScript half of it is beyond any compiler.
#[test]
fn nobody_hand_rolls_the_markdown_extension_check() {
    let root = workspace_root();
    let re = Regex::new(&MARKDOWN_PREDICATE_PATTERNS.join("|")).unwrap();

    // Extraction sanity: the canonical home itself must match, or the pattern
    // family has rotted into matching nothing and this gate is a no-op.
    assert!(
        re.is_match(&without_line_comments(&read(
            &root.join(MARKDOWN_PREDICATE_HOME)
        ))),
        "scan regex no longer matches {MARKDOWN_PREDICATE_HOME} — the pattern \
         family broke, fix the test"
    );

    let ledger: BTreeSet<&str> = MARKDOWN_PREDICATE_LEDGER.iter().copied().collect();
    let mut offenders = Vec::new();
    let mut matched_ledger_rows: BTreeSet<&str> = BTreeSet::new();

    for (rel, path) in crate_source_files(&root) {
        if rel == MARKDOWN_PREDICATE_HOME {
            continue;
        }
        let src = without_line_comments(&read(&path));
        let hits: Vec<String> = re
            .find_iter(&src)
            .map(|m| m.as_str().trim().to_string())
            .collect();
        if hits.is_empty() {
            continue;
        }
        match ledger.get(rel.as_str()) {
            Some(row) => {
                matched_ledger_rows.insert(row);
            }
            None => offenders.push(format!("{rel}: {}", hits.join(", "))),
        }
    }

    // Both directions, so the ledger records outstanding work rather than
    // forgotten work: a row whose file has stopped matching must be removed.
    let stale: Vec<&str> = ledger
        .difference(&matched_ledger_rows)
        .copied()
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty() && stale.is_empty(),
        "A2f: the markdown extension check has been hand-rolled again.\n\
         Call `crucible_core::is_note_file(path)` (or `is_indexable_file` when \
         canvases count too) instead — it lowercases the extension and accepts \
         `md` and `markdown`, which is what the daemon's indexer, watcher and \
         search already do. Building a `.md` path is a different question and \
         does not match these patterns.\n  \
         new copies: {offenders:?}\n  \
         stale MARKDOWN_PREDICATE_LEDGER rows (the file no longer matches — \
         remove the row): {stale:?}"
    );
}

// UNIQUE: the frontend copy is real and unavoidable (no Rust call, and asking
// the daemon would be a network round trip per keystroke), so the only thing
// that can stop it becoming six copies again is a scan. The compiler cannot
// cross the language boundary and neither can A2f's Rust half.
#[test]
fn the_frontend_has_exactly_one_markdown_predicate() {
    let root = workspace_root();
    let re = Regex::new(&FRONTEND_PREDICATE_PATTERNS.join("|")).unwrap();
    let homes: BTreeSet<&str> = FRONTEND_MARKDOWN_HOMES.iter().copied().collect();

    let canonical = root.join(FRONTEND_MARKDOWN_HOMES[0]);
    assert!(
        canonical.is_file(),
        "{} is missing — the frontend's single markdown predicate lives there; \
         if it moved, update FRONTEND_MARKDOWN_HOMES",
        FRONTEND_MARKDOWN_HOMES[0]
    );

    // Extraction sanity, against literals rather than a file. The Rust half
    // can scan its own canonical home for proof the pattern family still
    // matches; this half cannot, because `markdown-path.ts` deliberately
    // spells the predicate a third way (`NOTE_EXTENSIONS.includes(ext)`, so it
    // mirrors `Path::extension()` and agrees with `KilnFileKind::of` that a
    // bare `.md` is an Asset). With zero matches anywhere in `src/`, a typo in
    // FRONTEND_PREDICATE_PATTERNS would leave this test green forever. These
    // are the exact spellings the six deleted copies used.
    for spelling in [
        r"/\.(md|markdown)$/i.test(props.path)",
        r"/\.(md|markdown)$/.test(name)",
        r"if (ext === 'md') {",
        r#"if (ext === "md") {"#,
    ] {
        assert!(
            re.is_match(spelling),
            "scan regex no longer matches {spelling:?} — the pattern family \
             broke and the frontend half of A2f is silently a no-op; fix \
             FRONTEND_PREDICATE_PATTERNS"
        );
    }

    let mut offenders = Vec::new();
    for entry in WalkDir::new(root.join("crates/crucible-web/web/src"))
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        let ext = p.extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("ts") | Some("tsx")) {
            continue;
        }
        let rel = p
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if homes.contains(rel.as_str()) {
            continue;
        }
        for m in re.find_iter(&read(p)) {
            offenders.push(format!("{rel}: {}", m.as_str().trim()));
        }
    }

    assert!(
        offenders.is_empty(),
        "A2f (frontend): `isMarkdownPath` from `lib/markdown-path.ts` is the \
         frontend's only markdown predicate — import it instead of re-testing \
         the extension. Its counterpart `noteStem` strips the extension a \
         wikilink insert needs. Both mirror `KilnFileKind::of` in \
         crates/crucible-core/src/kiln.rs and must change with it:\n  - {}",
        offenders.join("\n  - ")
    );
}

/// A2g — the hook counts `AGENTS.md` prints match the enums the code declares.
///
/// The document told every agent that `crucible.on` takes 11 stages and 8
/// events. The code declared 13 and 10. Nothing compared the two, so the prose
/// drifted for as long as nobody counted the variants by hand.
///
/// One half of this comparison comes from `StageId::ALL` and `EventName::ALL`,
/// which `hook_name.rs` proves complete by walking `strum::EnumIter`. The
/// compiler owns that half. The test parses the other half out of the document.
///
/// **The parse must find something before the test trusts it.** A gate that
/// reads a number out of prose reports agreement when an author rewords the
/// sentence and the regex stops matching. An empty match therefore fails here,
/// and the message says the parse broke rather than naming a count.
#[test]
fn agents_md_prints_the_real_hook_counts() {
    use crucible_lua::{EventName, StageId};

    let root = workspace_root();
    let doc = read(&root.join("AGENTS.md"));

    let re = Regex::new(
        r"\*\*`StageId`\*\* \((\d+) synchronous stages\) or an \*\*`EventName`\*\* \((\d+) daemon broadcast events\)",
    )
    .unwrap();

    // Refuse an empty match. This assertion keeps the gate honest.
    let found = re.captures(&doc).unwrap_or_else(|| {
        panic!(
            "A2g: no sentence in AGENTS.md matched the hook-count pattern, so \
             this scan broke rather than the counts agreeing. The sentence \
             lives under `### Hooks and ACP` and reads \"takes a **`StageId`** \
             (N synchronous stages) or an **`EventName`** (M daemon \
             broadcast events)\". Restore that wording, or change this regex \
             together with it."
        )
    });

    let stated_stages: usize = found[1].parse().expect("stage count is a number");
    let stated_events: usize = found[2].parse().expect("event count is a number");

    assert_eq!(
        stated_stages,
        StageId::ALL.len(),
        "A2g: AGENTS.md says {stated_stages} stages; `StageId::ALL` \
         holds {}. Every agent reads that document first. Update the sentence \
         under `### Hooks and ACP`, and the module doc of \
         crates/crucible-lua/src/handlers/hook_name.rs, which repeats it.",
        StageId::ALL.len()
    );

    assert_eq!(
        stated_events,
        EventName::ALL.len(),
        "A2g: AGENTS.md says {stated_events} daemon broadcast events; \
         `EventName::ALL` holds {}. Every agent reads that document first. \
         Update the sentence under `### Hooks and ACP`, and the module doc of \
         crates/crucible-lua/src/handlers/hook_name.rs, which repeats it.",
        EventName::ALL.len()
    );
}
