//! Architecture invariant gates for the CLI/TUI crate (source-scan tests).
//!
//! Companion to `crucible-daemon/tests/architecture_tests.rs`. These encode
//! TUI-side invariants CLAUDE.md states in prose:
//!   A2a — every `ChatAppMsg` variant is handled somewhere (no dead messages).
//!   A2b — canonical parser types are defined only in crucible-core/parser.
//!   A2c — moved to `crucible-web/tests/openapi_contract.rs`, which holds
//!         the router and the client to the generated OpenAPI document.
//!   A2d — the CLI does not build its own knowledge-base context block.
//!   A2e — every session knob the daemon advertises has a web route and a
//!         TUI `:set` key, both derived from `SessionKnob`.
//!   A2f — nobody hand-rolls the "is this file markdown" predicate.
//!
//! A2* live here rather than in the daemon's companion file because they scan
//! CLI and web source; the daemon's header lists A1/A3/A4/A5 for the same
//! reason.
//!
//! Source-scan style: read files and match, so they are fast and build-free.
//! When one fails, fix the code, not the test — see each failure message.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crucible_core::types::SessionKnob;
use crucible_daemon::rpc::{rpc_set_method, RpcMethod, METHODS};
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
// A2c — MOVED. `crates/crucible-web/tests/openapi_contract.rs` now holds this
// gate, as `every_api_path_the_client_calls_reaches_a_route` (green today),
// plus `every_route_the_router_serves_is_in_the_document` and
// `every_api_path_the_client_calls_is_in_the_document` (both `#[ignore]`d
// until task A10 finishes converting the route groups).
//
// The scan that stood here compared `/api` string literals in one TypeScript
// file against a second regex scan of the Rust routes. It caught a frontend
// path that named no route, and missed everything else. The two tests in
// `openapi_contract.rs` compare both sides against the generated OpenAPI
// document instead, which adds:
//   - the method: `GET /api/layout` and `DELETE /api/layout` are two entries,
//     not one string;
//   - the fields: the document carries each reply's shape, so the drifts a
//     path comparison cannot see fail `just lint types`;
//   - `lib/review-api.ts`: its seven review paths were never read here;
//   - the reverse direction: a route the router serves and nothing describes
//     now fails, where this scan only ran frontend to backend.
// The nest prefixes resolve by router-function name there, so the scan no
// longer invents paths by joining every prefix onto every relative route.
// ===========================================================================

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
// A2e — every session knob the daemon advertises is reachable from the web,
// and from the TUI.
//
// Nine of fifteen knobs had no web route and nothing failed: the daemon grows
// a knob, the TUI wires it, and the web falls a knob further behind. A1 in the
// daemon's companion file gates client↔server field-name parity; daemon↔front
// end was the ungated axis, and it is a different failure (a route or a `:set`
// key that does not exist at all, rather than a field name that disagrees).
//
// DERIVED, NOT GREPPED. Both gates once read `"session\.set_([a-z0-9_]+)"`
// over the whole text of `dispatch.rs`. That file names each method three
// times — in the `rpc_methods!` table, in the setter router, and in
// `#[cfg(test)] mod tests` — and a regex cannot tell the three apart. A stale
// literal in a unit-test sample value therefore advertised a knob the daemon
// had deleted, and the gates demanded a route and a key for it. That cost two
// red CI runs. The prefix scan also missed `session.switch_model` entirely, so
// the `model` knob was never gated on either front end.
//
// The source is now `SessionKnob::ALL` joined to `RpcMethod` through
// `rpc::rpc_set_method`. Both arrays are proved complete by an `EnumIter` walk
// in their own crate, the mapping is an exhaustive match under two clippy
// denies, and `RpcMethod` cannot name a method `METHODS` does not advertise.
// A literal in a test body reaches none of that.
//
// The ledgers below are SHRINK-ONLY: a NEW knob is not in one and so fails
// immediately.
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

/// Every `/api` path the axum router declares, as a shape.
///
/// This helper and `normalize_api_path` stayed behind when A2c moved to
/// `crucible-web/tests/openapi_contract.rs`. The gate below asks only whether
/// a knob's route exists, and a membership test tolerates the
/// over-approximation the comment at the end admits; A2c did not.
fn backend_api_paths(root: &Path) -> BTreeSet<String> {
    let route_re = Regex::new(r#"\.route\(\s*"([^"]+)""#).unwrap();
    let nest_re = Regex::new(r#"\.nest\(\s*"([^"]+)""#).unwrap();
    // `utoipa_axum::routes!(handler)` takes the path from the handler's
    // `#[utoipa::path]` attribute, so a converted route has no `.route("...")`
    // line to find. The lazy match takes the first `path = "..."` after the
    // attribute opens.
    let utoipa_re = Regex::new(r#"(?s)#\[utoipa::path\(.*?path\s*=\s*"([^"]+)""#).unwrap();

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
        for c in utoipa_re.captures_iter(src) {
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

/// `session.set_agent_option` is guarded, and it is NOT a [`SessionKnob`].
///
/// The gate exists because of this name: `agent_option` shipped to the web and
/// not to the TUI and passed review. A gate driven by `SessionKnob` alone
/// would drop it, which is why it is named here rather than omitted.
///
/// It stays out of the enum because it is not one setting. It projects the
/// settings an EXTERNAL agent advertised for itself, so its value space
/// changes with the agent, an internal session has none at all, and the report
/// of what it holds is `session.list_agent_options` rather than a getter. A
/// `SessionKnob` variant would put a `{ id, supported }` row in
/// `session.list_knobs` for something no session has until an agent says so,
/// and would demand an `on_acp` answer for a setting that exists only over
/// ACP. `AgentConfigOption`'s own docs state the same rule: "Crucible has no
/// knob for these."
///
/// The pair is compiler-checked all the same — the method is an `RpcMethod`
/// variant, so a deleted method breaks this file rather than silencing it.
const NON_KNOB_GUARDED: &[(&str, RpcMethod)] =
    &[("agent_option", RpcMethod::SessionSetAgentOption)];

/// `session.set_*` methods that mutate session SCOPE rather than configure the
/// agent. They share the prefix but are not knobs, and neither belongs under
/// `config/`.
const SCOPE_MUTATIONS: &[&str] = &["title", "workspace"];

/// Every id the two parity gates guard, with the method that writes it.
///
/// `SessionKnob::ALL` plus [`NON_KNOB_GUARDED`]. A knob's method comes from
/// `rpc_set_method`, which is why `model` is guarded at last: `session.switch_model`
/// writes it, and a `set_` prefix scan never saw that name.
fn guarded_ids() -> BTreeMap<String, RpcMethod> {
    let mut out: BTreeMap<String, RpcMethod> = SessionKnob::ALL
        .iter()
        .map(|k| (k.id().to_string(), rpc_set_method(*k)))
        .collect();
    for (id, method) in NON_KNOB_GUARDED {
        out.insert((*id).to_string(), *method);
    }
    out
}

/// Nothing guarded names a method the daemon does not answer, and no
/// `session.set_*` method escapes the gates.
///
/// The second half keeps the enumerated source honest. A new setter in
/// `rpc_methods!` is neither a `SessionKnob` nor a scope mutation, so it fails
/// here until somebody says which it is. Without it, the swap from a grep to
/// the enum would have narrowed what the gates cover.
#[test]
fn every_session_setter_is_either_guarded_or_declared_out_of_scope() {
    let guarded = guarded_ids();
    let mut failures = Vec::new();

    for (id, method) in &guarded {
        assert!(
            METHODS.contains(&method.as_str()),
            "guarded id `{id}` maps to `{}`, which METHODS does not advertise",
            method.as_str()
        );
    }

    let guarded_methods: BTreeSet<&str> = guarded.values().map(|m| m.as_str()).collect();
    let scope: BTreeSet<String> = SCOPE_MUTATIONS
        .iter()
        .map(|s| format!("session.set_{s}"))
        .collect();

    for method in RpcMethod::ALL {
        let name = method.as_str();
        if !name.starts_with("session.set_") {
            continue;
        }
        if guarded_methods.contains(name) || scope.contains(name) {
            continue;
        }
        failures.push(format!(
            "`{name}` is a session setter that no front-end gate covers. Add a \
             `SessionKnob` variant for it, add it to NON_KNOB_GUARDED with a \
             reason, or add its suffix to SCOPE_MUTATIONS"
        ));
    }

    assert!(
        failures.is_empty(),
        "ungated session setters:\n  - {}",
        failures.join("\n  - ")
    );
}

/// knob id → the web route that reaches it.
///
/// Declared rather than derived: a knob's route is not always its id in kebab
/// case, and two of them sit outside `config/` altogether. Full paths, not
/// tails, so `mode` and `model` get the same existence check as the rest
/// instead of a blanket exemption.
const WEB_KNOB_ROUTES: &[(&str, &str)] = &[
    // Switching model and switching mode each have their own route: mode
    // changes tool policy and model re-resolves the provider, so neither is a
    // scalar under `config/`.
    ("model", "/api/session/{}/model"),
    ("mode", "/api/session/{}/mode"),
    (
        "context_strategy",
        "/api/session/{}/config/context-strategy",
    ),
    ("precognition", "/api/session/{}/config/precognition"),
    // One path serves both directions — GET lists them, POST sets one —
    // because the value belongs to the agent and is read back from its list.
    ("agent_option", "/api/session/{}/config/agent-options"),
];

/// Knobs with no web route yet. REMOVE entries as routes land; never add.
///
/// Empty: every guarded id is reachable from the web. It held nine when this
/// gate landed, and the gate is what forced each row out — a route that lands
/// while its row stays here fails just as loudly as the reverse, which is how
/// the ledger stays a record of work outstanding rather than of work
/// forgotten.
const WEB_ROUTE_LEDGER: &[&str] = &[];

// UNIQUE: the daemon's method table and the web's axum Router are in different
// crates with no shared type; a knob present in one and absent from the other
// is not a compile error, and A1 only compares the daemon's own client to its
// own server. Route EXISTENCE, not field names — field names are already A1's
// job, which matters because a web struct named after the knob would compile,
// pass review, and drop the value.
#[test]
fn every_rpc_session_knob_is_reachable_from_the_web() {
    let root = workspace_root();
    let guarded = guarded_ids();

    let mapped: BTreeSet<String> = WEB_KNOB_ROUTES.iter().map(|(k, _)| k.to_string()).collect();
    let ledger: BTreeSet<&str> = WEB_ROUTE_LEDGER.iter().copied().collect();
    let mut failures = Vec::new();

    // (1) Table completeness: every guarded id has a route row.
    for id in guarded.keys() {
        if !mapped.contains(id) {
            failures.push(format!(
                "`{id}` has no WEB_KNOB_ROUTES row — add the route path, or \
                 (temporarily) add `{id}` to WEB_ROUTE_LEDGER"
            ));
        }
    }
    // (2) Table staleness: no row for an id the daemon dropped.
    for stale in mapped.iter().filter(|id| !guarded.contains_key(*id)) {
        failures.push(format!(
            "WEB_KNOB_ROUTES row `{stale}` is no longer guarded — remove the row \
             or restore the knob"
        ));
    }

    // (3) The routes actually exist in the axum Router.
    let backend = backend_api_paths(&root);
    for (id, path) in WEB_KNOB_ROUTES {
        let present = backend.contains(*path);
        let ledgered = ledger.contains(*id);
        if !present && !ledgered {
            failures.push(format!(
                "`{id}`: no web route at {path}. Add it under \
                 crucible-web/src/routes/session_config/, register it in \
                 routes/session/mod.rs, or (temporarily) add `{id}` to \
                 WEB_ROUTE_LEDGER"
            ));
        }
        if present && ledgered {
            failures.push(format!(
                "`{id}`: route {path} now exists — REMOVE `{id}` from \
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

/// knob id → the `:set` key that reaches it, and a value that key accepts.
///
/// Declared rather than derived, like [`WEB_KNOB_ROUTES`]: the TUI spells most
/// keys without underscores, and some carry an alias for both spellings. No
/// transform covers that.
///
/// The sample value must be VALID for the key, because the gate demands
/// `Ok(SetEffect::DaemonRpc(_))`. An earlier version accepted any error but
/// `UnknownKey`, and a knob demoted to `SetEffect::TuiLocal` passed it. That
/// regression is on record in `tui/oil/commands/set.rs`: `precognition` looked
/// like its TUI-local neighbours, became one, and the toggle then changed only
/// the `:set` readout.
const TUI_SET_KEYS: &[(&str, &str, &str)] = &[
    ("model", "model", "claude-opus-4"),
    ("context_strategy", "contextstrategy", "truncate"),
    ("precognition", "precognition", "true"),
];

/// Ids the TUI cannot set at all. REMOVE entries as keys land; never add.
///
/// `agent_option` is the live gap. `session.list_agent_options` and
/// `session.set_agent_option` project the settings an external agent
/// advertises for itself, and only the web reads them, so a TUI user who talks
/// to an ACP agent cannot see or change what that agent offers.
const TUI_KEY_LEDGER: &[&str] = &["agent_option"];

/// Exempt permanently, with a reason: `mode` has Shift-Tab and `:mode`, and a
/// mode switch changes tool policy rather than a scalar setting.
const TUI_KEY_EXEMPT: &[&str] = &["mode"];

/// A knob the daemon advertises must be reachable from the TUI as well as the
/// web.
///
/// AGENTS.md asks "Where does a user meet it? TUI *and* web", and the web half
/// is the gate above. Without this half a knob can ship to one renderer and
/// pass review — which is what happened to `agent_option`.
///
/// Derived, not grepped: it calls the real `:set` classifier, so a row cannot
/// be satisfied by a string that appears somewhere in the file.
#[test]
fn every_rpc_session_knob_is_reachable_from_the_tui() {
    use crucible_cli::tui::oil::commands::{classify_set_value, SetEffect, SetError};

    let guarded = guarded_ids();
    let mapped: BTreeSet<String> = TUI_SET_KEYS.iter().map(|(k, _, _)| k.to_string()).collect();
    let exempt: BTreeSet<String> = TUI_KEY_EXEMPT.iter().map(|s| s.to_string()).collect();
    let ledger: BTreeSet<String> = TUI_KEY_LEDGER.iter().map(|s| s.to_string()).collect();
    let mut failures = Vec::new();

    // (1) Table completeness: every guarded id is mapped, exempt or ledgered.
    for id in guarded.keys() {
        if !mapped.contains(id) && !exempt.contains(id) && !ledger.contains(id) {
            failures.push(format!(
                "`{id}` has no TUI_SET_KEYS row — add the `:set` key, mark it \
                 TUI_KEY_EXEMPT with a reason, or (temporarily) add it to \
                 TUI_KEY_LEDGER"
            ));
        }
    }
    // (2) Table staleness: no row for an id the daemon dropped.
    let declared: BTreeSet<String> = mapped
        .union(&ledger)
        .cloned()
        .collect::<BTreeSet<_>>()
        .union(&exempt)
        .cloned()
        .collect();
    for stale in declared.iter().filter(|id| !guarded.contains_key(*id)) {
        failures.push(format!(
            "TUI row `{stale}` is no longer guarded — remove the row or restore \
             the knob"
        ));
    }
    // (3) The keys are real AND they still reach the daemon. `Ok(DaemonRpc)`,
    // not "any error but UnknownKey": a key that validates and then writes a
    // TUI-local value is the regression this gate is here to catch.
    for (id, key, sample) in TUI_SET_KEYS {
        match classify_set_value((*key).to_string(), (*sample).to_string()) {
            Ok(SetEffect::DaemonRpc(_)) => {}
            Err(SetError::UnknownKey(_)) => failures.push(format!(
                "`{id}`: `:set {key}=…` is an unknown key. Add an arm to \
                 `classify_set_value` in tui/oil/commands/set.rs, or fix the row"
            )),
            Ok(SetEffect::TuiLocal { .. }) => failures.push(format!(
                "`{id}`: `:set {key}={sample}` writes a TUI-local value, so the \
                 daemon never hears it. Return SetEffect::DaemonRpc from \
                 `classify_set_value`"
            )),
            Err(other) => failures.push(format!(
                "`{id}`: `:set {key}={sample}` was refused ({other:?}). The sample \
                 value must be one the key accepts"
            )),
        }
    }
    // (4) A ledgered or exempt id really is unreachable, so both lists shrink.
    for id in ledger.union(&exempt) {
        if !matches!(
            classify_set_value(id.clone(), "1".to_string()),
            Err(SetError::UnknownKey(_))
        ) {
            failures.push(format!(
                "`{id}`: `:set {id}` now classifies — move it into TUI_SET_KEYS \
                 with a valid sample value"
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

// ─────────────────────────────────────────────────────────────────────────
// A2h — a side-channel wire name is written once
// ─────────────────────────────────────────────────────────────────────────

/// The only Rust files allowed to spell a `SystemPayload` wire name.
///
/// `lifecycle.rs` declares it twice by necessity — `#[serde(rename = ...)]`
/// takes a literal and cannot read a const — and
/// `a_system_events_const_matches_its_serde_name` proves the two agree by
/// serializing the variant. `session_events/mod.rs` holds `Group::of`, whose
/// coverage `group_of_knows_every_declared_event` derives from the enums.
///
/// Every other crate reads `SystemPayload::SURFACE_CHANGED` or
/// `SystemPayload::PUBLICATION_CHANGED`.
const WIRE_NAME_HOMES: &[&str] = &[
    "crates/crucible-core/src/protocol/session_events/lifecycle.rs",
    "crates/crucible-core/src/protocol/session_events/mod.rs",
];

/// A2h: neither side-channel event name appears as a literal outside its home.
///
/// A merge deleted `crucible_daemon::event_map::PUBLICATION_CHANGED_EVENT` and
/// replaced its one cross-crate use with a fresh literal in `crucible-web`, so
/// the name was written in two crates with nothing comparing them. Nothing
/// noticed: the cross-language gate that exists
/// (`crucible-web`'s `sse_event_names_match_the_frontend_listener_list`)
/// compares `ChatEvent::event_name()` to `SSE_EVENT_TYPES`, and these two
/// events travel their own SSE streams, so neither side of it names either one.
///
/// **This gate FORBIDS rather than REQUIRES.** A gate that requires an entry
/// is satisfiable by not adding the entry — the failure `AGENTS.md` records
/// four times. A gate that forbids an extra copy fails the moment somebody
/// writes one, which is the event to catch. The names come from the compiled
/// consts, and a run that scans no file fails.
#[test]
fn a_side_channel_wire_name_is_written_once() {
    use crucible_core::protocol::SystemPayload;

    let root = workspace_root();
    let files = crate_source_files(&root);
    assert!(
        !files.is_empty(),
        "A2h: scanned no crate sources — the walk root moved, fix this test"
    );

    let names = [
        SystemPayload::SURFACE_CHANGED,
        SystemPayload::PUBLICATION_CHANGED,
    ];

    // The homes must still hold the names, or the scan below proves nothing.
    for home in WIRE_NAME_HOMES {
        let (_, path) = files
            .iter()
            .find(|(rel, _)| rel == home)
            .unwrap_or_else(|| panic!("A2h: `{home}` is gone — fix this test"));
        let body = read(path);
        for name in names {
            assert!(
                body.contains(&format!("\"{name}\"")),
                "A2h: `{home}` no longer spells `{name}` — the declaration \
                 moved, so update WIRE_NAME_HOMES"
            );
        }
    }

    let mut offenders = Vec::new();
    for (rel, path) in &files {
        if WIRE_NAME_HOMES.contains(&rel.as_str()) {
            continue;
        }
        let body = without_line_comments(&read(path));
        for name in names {
            if body.contains(&format!("\"{name}\"")) {
                offenders.push(format!("{rel}: \"{name}\""));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "A2h: a side-channel wire name is written outside its home. Read \
         `crucible_core::protocol::SystemPayload::SURFACE_CHANGED` or \
         `::PUBLICATION_CHANGED` instead of spelling the string:\n  {}",
        offenders.join("\n  ")
    );
}
