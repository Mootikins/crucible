//! Architecture invariant gates enforced as failing tests.
//!
//! These are source-scan tests (walkdir + regex over the workspace `src/`
//! trees) rather than behavioural tests: they encode structural rules that
//! CLAUDE.md states in prose but nothing enforced before. Because they only
//! read files, they are fast and have no build dependencies.
//!
//! Gates in this file:
//!   A1 — RPC field-name parity for session config get/set pairs.
//!   A3 — wire-mock seam: vendor LLM SDKs / genai stay behind `provider/`.
//!   A4 — module-size ratchet against a frozen ledger.
//!   A5 — `#[ignore]` reason strings parse, and the external test tier is
//!        derived from them rather than hand-maintained.
//!   A6 — a turn rebuilds its conversation tree before emitting the event that
//!        gets appended to the file the rebuild reads.
//!
//! **A2 is not missing.** A2a–A2e live in the *CLI* crate's companion file,
//! `crates/crucible-cli/tests/architecture_tests.rs`, because they scan the
//! CLI's message enum and the web frontend's `api.ts`. Look there for the
//! `ChatAppMsg` and frontend↔backend route gates.
//!
//! When one of these fails, the fix is almost always to change the code, not
//! the test. See the per-gate failure messages for the specific action.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::WalkDir;

/// Locate the workspace root relative to this crate's manifest dir
/// (`.../crates/crucible-daemon`), so the scans work regardless of the
/// current working directory.
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

/// Every `*.rs` file under `crates/`, including each crate's own `tests/` dir,
/// as `(relative_path, contents)`.
///
/// A5 needs this rather than [`workspace_src_files`] because 63 of the 106
/// `#[ignore]` attributes are in `tests/`. Kept as a sibling deliberately: A3
/// and A4 depend on `workspace_src_files`' `/src/`-only contract, and widening
/// it would start A4 ratcheting test files.
fn workspace_test_and_src_files() -> Vec<(String, String)> {
    let root = workspace_root();
    let mut out = Vec::new();
    for entry in WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let rel = path.strip_prefix(&root).unwrap();
        out.push((rel.to_string_lossy().replace('\\', "/"), read(path)));
    }
    out.sort();
    out
}

/// Every `*.rs` file under `crates/*/src/` as `(relative_path, contents)`.
fn workspace_src_files() -> Vec<(String, String)> {
    let root = workspace_root();
    let mut out = Vec::new();
    for entry in WalkDir::new(root.join("crates"))
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        // Only source trees, not each crate's own tests/ dir.
        let rel = path.strip_prefix(&root).unwrap();
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if !rel_str.contains("/src/") {
            continue;
        }
        out.push((rel_str, read(path)));
    }
    out
}

// ---------------------------------------------------------------------------
// Shared source-parsing helpers
// ---------------------------------------------------------------------------

/// Given the byte index of an opening delimiter (`{` or `(`), return the text
/// from that delimiter through its matching close. Skips delimiters that appear
/// inside string literals and `//` line comments so that `json!({ ... })` and
/// quoted `{}` / `()` don't unbalance the count.
fn balanced(src: &str, open: usize) -> String {
    let bytes = src.as_bytes();
    let open_c = bytes[open] as char;
    let close_c = match open_c {
        '{' => '}',
        '(' => ')',
        other => panic!("balanced: unsupported opening delimiter {other:?}"),
    };

    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    let mut i = open;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        // Skip `// ...` line comments.
        if c == '/' && i + 1 < bytes.len() && bytes[i + 1] as char == '/' {
            while i < bytes.len() && bytes[i] as char != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
        } else if c == open_c {
            depth += 1;
        } else if c == close_c {
            depth -= 1;
            if depth == 0 {
                return src[open..=i].to_string();
            }
        }
        i += 1;
    }
    panic!("unbalanced {open_c}{close_c} from offset {open}");
}

/// Extract a function body given a substring that uniquely identifies the
/// signature (the text between its opening and matching closing brace).
fn fn_body(src: &str, signature: &str) -> String {
    let start = src
        .find(signature)
        .unwrap_or_else(|| panic!("signature not found: {signature}"));
    let open = start
        + src[start..]
            .find('{')
            .unwrap_or_else(|| panic!("no opening brace after: {signature}"));
    balanced(src, open)
}

fn captures(re: &str, hay: &str) -> BTreeSet<String> {
    let re = Regex::new(re).unwrap();
    re.captures_iter(hay).map(|c| c[1].to_string()).collect()
}

// ===========================================================================
// A1 — RPC field-name parity, per-knob, is GONE.
//
// Every knob now shares one write method (`session.knob.set`) and one read
// method (`session.knob.get`), and both sides — the client in
// `rpc_client/client/agent.rs` and the handler in
// `server/session/params.rs` — deserialize the same core type,
// `crucible_core::types::KnobValue` (set) or
// `crucible_core::protocol::requests::KnobRef` (get). The historical bug
// class this gate caught — a client field name that disagrees with the
// server's — is now a compile error: there is one type, so there is only one
// spelling to type out.
//
// The scope-mutation pairs below (`session.connect_kiln` etc.) are NOT
// knobs — they keep their own hand-written request/response shapes, so their
// parity gate stays.
// ===========================================================================

/// The failures for one setter whose client and server sides must both name
/// `request_type`: the client builds it, and the server deserializes it.
fn shared_request_type_failures(
    method: &str,
    request_type: &str,
    client_body: &str,
    server_body: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    let built = Regex::new(&format!(r"\b{}\s*\{{", regex::escape(request_type))).unwrap();
    if !built.is_match(client_body) {
        failures.push(format!(
            "{method}: the client does not build {request_type} — send the shared \
             request type, not a JSON literal"
        ));
    }
    // A session-scoped method reads its body inside the `Scoped<T>` envelope.
    let parsed = Regex::new(&format!(
        r"typed_params::<\s*(?:Scoped<\s*)?{}\s*>?\s*>",
        regex::escape(request_type)
    ))
    .unwrap();
    if !parsed.is_match(server_body) {
        failures.push(format!(
            "{method}: the server does not deserialize {request_type} — use \
             `typed_params::<{request_type}>(&req)`, not a field read by hand"
        ));
    }
    failures
}

// ===========================================================================
// A1b — scope-mutation parity: session.connect_kiln / session.disconnect_kiln
// / session.set_workspace. These are NOT config knobs — their handlers are
// hand-written in `server/session/scope.rs` (attach-time trust gating), there
// is no `get` direction, and all three return the shared scope response
// instead of echoing a single field — but the field-name bug class is
// identical, so they get their own parity table instead of CONFIG_METHODS
// rows.
// ===========================================================================

struct ScopeMethod {
    /// Client fn name in `rpc_client/client/agent.rs`.
    client_fn: &'static str,
    /// Handler fn name in `server/session/scope.rs`.
    server_fn: &'static str,
    /// The request type that both sides name.
    request_type: &'static str,
}

const SCOPE_METHODS: &[ScopeMethod] = &[
    ScopeMethod {
        client_fn: "session_connect_kiln",
        server_fn: "handle_session_connect_kiln",
        request_type: "NamedKiln",
    },
    ScopeMethod {
        client_fn: "session_disconnect_kiln",
        server_fn: "handle_session_disconnect_kiln",
        request_type: "NamedKiln",
    },
    ScopeMethod {
        client_fn: "session_set_workspace",
        server_fn: "handle_session_set_workspace",
        request_type: "WorkspaceChoice",
    },
];

// UNIQUE: the result field names are JSON string literals on both sides of the wire, and the type system does not see them. The request side is a shared type, and this gate checks that each side names it.
#[test]
fn rpc_scope_mutation_field_names_match_across_the_wire() {
    let root = workspace_root();
    let client = read(&root.join("crates/crucible-daemon/src/rpc_client/client/agent.rs"));
    let server = read(&root.join("crates/crucible-daemon/src/server/session/scope.rs"));

    let mut failures = Vec::new();

    for m in SCOPE_METHODS {
        let client_body = fn_body(&client, &format!("fn {}(", m.client_fn));
        let server_body = fn_body(&server, &format!("fn {}(", m.server_fn));
        failures.extend(shared_request_type_failures(
            m.client_fn,
            m.request_type,
            &client_body,
            &server_body,
        ));
    }

    // All three mutations return the shared scope response, and the client
    // hands that JSON back verbatim (no per-field reads), so response parity
    // is a single assertion on the shared builder.
    let scope_response = fn_body(&server, "fn scope_response(");
    let response_fields = captures(r#""([a-z_][a-z0-9_]*)""#, &scope_response);
    let expected: BTreeSet<String> = ["session_id", "kilns", "workspace"]
        .into_iter()
        .map(str::to_string)
        .collect();
    if response_fields != expected {
        failures.push(format!(
            "scope_response: returns fields {response_fields:?}, expected {expected:?}"
        ));
    }

    assert!(
        failures.is_empty(),
        "RPC scope-mutation field-name parity violations (fix the client/server \
         field name, not this test):\n  - {}",
        failures.join("\n  - ")
    );
}

// ===========================================================================
// A3 — wire-mock seam: LLM SDK access stays behind the provider module.
//
// Keeping every vendor call behind `provider/` is what makes a fake-server
// (wire-level) LLM mock viable for the whole daemon. `genai` (the SDK genai
// wraps them all) is allowed only in `provider/` and `agent_factory.rs`;
// direct vendor SDK crates are banned everywhere.
// ===========================================================================

/// Vendor LLM SDK crate roots. `genai` is the sanctioned wrapper and is
/// checked separately (it is allowed inside the provider seam).
const BANNED_LLM_SDK_CRATES: &[&str] = &[
    "async_openai",
    "async_anthropic",
    "anthropic_sdk",
    "ollama_rs",
    "cohere_rust",
    "openai_api_rs",
    "openai_rust",
    "mistralai",
    "groq_api",
    "google_generative_ai",
    "gemini_rs",
];

fn is_provider_seam(rel_path: &str) -> bool {
    rel_path.starts_with("crates/crucible-daemon/src/provider/")
        || rel_path == "crates/crucible-daemon/src/agent_factory.rs"
}

// UNIQUE: no clippy.toml exists at the workspace root; clippy's disallowed_types would require workspace-level deny + per-file allow attributes scattered across every non-provider source file. The source-scan is the only enforceable seam keeping the wire-level LLM mock viable.
#[test]
fn genai_stays_behind_the_provider_seam() {
    let genai_use = Regex::new(r"\bgenai::").unwrap();
    let mut offenders = Vec::new();
    for (rel, contents) in workspace_src_files() {
        if is_provider_seam(&rel) {
            continue;
        }
        if genai_use.is_match(&contents) {
            offenders.push(rel);
        }
    }
    assert!(
        offenders.is_empty(),
        "`genai` may only be used behind the provider seam \
         (crucible-daemon/src/provider/ or agent_factory.rs). Route this through \
         the provider abstraction so the wire-level LLM mock stays viable. \
         Offending files:\n  - {}",
        offenders.join("\n  - ")
    );
}

// UNIQUE: clippy disallowed-types/disallowed-methods would require per-file allows in every non-provider source file (brittle, no workspace glob support). The source-scan is the only enforcement blocking direct vendor SDK imports that would break the wire-mock seam.
#[test]
fn vendor_llm_sdks_are_not_imported_directly() {
    let alt = BANNED_LLM_SDK_CRATES.join("|");
    // Match `use <crate>` or `<crate>::` path usage.
    let re = Regex::new(&format!(r"(?:\buse\s+|\b)(?:{alt})::")).unwrap();
    let mut offenders = Vec::new();
    for (rel, contents) in workspace_src_files() {
        if let Some(hit) = re.find(&contents) {
            offenders.push(format!("{rel}: {}", hit.as_str()));
        }
    }
    assert!(
        offenders.is_empty(),
        "Direct vendor LLM SDK imports are banned — go through `genai` inside the \
         provider seam so the wire mock keeps working. Offending files:\n  - {}",
        offenders.join("\n  - ")
    );
}

// ===========================================================================
// A5 — `#[ignore]` reason strings are the gating mechanism (CLAUDE.md: "Tests
// needing external prerequisites are `#[ignore]`d with the prerequisite in the
// reason string — that, not cargo features, is how slow/external tests are
// gated"), so the justfile's test tiers are derived from them. That only works
// if they parse.
//
// Before normalization there were 39 spellings across 106 sites, including a
// NEGATIVE prerequisite ("requires built binary without Ollama" — substring
// matching files it under Ollama, the exact opposite of what it needs), one
// that described a known defect rather than a prerequisite (it had since been
// fixed and the string never updated), and three that named no prerequisite at
// all.
//
// Grammar:  #[ignore = "requires: <token>[, <token>]* — <free text>"]
// The token list is closed. Adding a token means wiring it into the justfile
// tiers, which is why the gate refuses unknown ones.
// ===========================================================================

/// Prerequisites a developer machine or CI runner satisfies by building this
/// repo. Tests needing only these belong in the blocking `just test gated`
/// tier.
const IGNORE_TOKENS_HERMETIC: &[&str] = &[
    "cru binary",
    "dev kiln",
    "mock-acp-agent",
    "ripgrep",
    "wall clock",
];

/// Prerequisites nothing in the repo can provide: network, a model, a daemon
/// we do not ship, a container runtime, or a human. These drive
/// `assets/test-tiers/external.txt` and are EXCLUDED from `just test gated`.
///
/// `no Ollama` is a negative prerequisite — a test that asserts the
/// no-provider path. It is external because it needs Ollama to be *absent*,
/// which a machine with Ollama installed cannot satisfy. Never tier by
/// substring: it contains "Ollama".
const IGNORE_TOKENS_EXTERNAL: &[&str] = &[
    "ACP agent",
    "LLM provider",
    "Ollama",
    "container runtime",
    "embedding endpoint",
    "live database",
    "manual inspection",
    "model download",
    "no Ollama",
    "playwright harness",
];

/// The checked-in list of ignored tests whose prerequisites are external.
/// The `gated` tier runs every ignored test EXCEPT these (negative selection,
/// so a newly added ignored test is in the blocking gate by default).
const EXTERNAL_TIER_FILE: &str = "assets/test-tiers/external.txt";

/// Set when `just test tiers` runs this file's checker in generate mode.
/// Child-scoped env from the recipe, never `set_var` from inside a test.
const WRITE_TIERS_ENV: &str = "CRUCIBLE_WRITE_TEST_TIERS";

/// One `#[ignore]`d test: where it lives, what it is called, and the
/// prerequisite tokens its reason declares.
struct IgnoredTest {
    rel: String,
    name: String,
    reason: String,
    tokens: Vec<String>,
}

impl IgnoredTest {
    fn is_external(&self) -> bool {
        let external: BTreeSet<&str> = IGNORE_TOKENS_EXTERNAL.iter().copied().collect();
        self.tokens.iter().any(|t| external.contains(t.as_str()))
    }
}

/// Every `#[ignore = "..."]` attribute in the workspace, paired with the test
/// it decorates.
///
/// Attribute order varies (`#[ignore]` may precede or follow `#[tokio::test]`),
/// so the pairing matches FORWARD to the nearest `fn <name>(` rather than
/// assuming a fixed layout. `raw_count` is the number of `#[ignore` attribute
/// lines seen by a plain line scan: comparing it against the parsed count is
/// what catches a regex that silently stops matching a spelling, which no
/// hardcoded total can do without failing every time a test is added.
fn ignored_tests() -> (Vec<IgnoredTest>, usize) {
    let attr = Regex::new(r#"(?m)^[ \t]*#\[ignore\s*=\s*"([^"]*)"\]"#).unwrap();
    let raw = Regex::new(r"(?m)^[ \t]*#\[ignore").unwrap();
    let next_fn = Regex::new(r"\bfn\s+([A-Za-z0-9_]+)\s*\(").unwrap();

    let mut out = Vec::new();
    let mut raw_count = 0;
    for (rel, contents) in workspace_test_and_src_files() {
        raw_count += raw.find_iter(&contents).count();
        for c in attr.captures_iter(&contents) {
            let whole = c.get(0).unwrap();
            let reason = c[1].to_string();
            let name = next_fn
                .captures(&contents[whole.end()..])
                .map(|f| f[1].to_string())
                .unwrap_or_else(|| panic!("{rel}: #[ignore = {reason:?}] has no `fn` after it"));
            let tokens = reason
                .strip_prefix("requires: ")
                .map(|rest| {
                    rest.split(" — ")
                        .next()
                        .unwrap_or(rest)
                        .split(", ")
                        .map(|t| t.trim().to_string())
                        .collect()
                })
                .unwrap_or_default();
            out.push(IgnoredTest {
                rel: rel.clone(),
                name,
                reason,
                tokens,
            });
        }
    }
    (out, raw_count)
}

// UNIQUE: an attribute string is opaque to rustc — nothing checks that
// `#[ignore = "..."]` names a prerequisite at all, let alone one the justfile
// knows how to satisfy. The tiers key off these strings, so they are a wire
// contract with no type to carry it.
#[test]
fn every_ignore_reason_declares_known_prerequisites() {
    let (tests, raw_count) = ignored_tests();
    let known: BTreeSet<&str> = IGNORE_TOKENS_HERMETIC
        .iter()
        .chain(IGNORE_TOKENS_EXTERNAL)
        .copied()
        .collect();

    assert_eq!(
        tests.len(),
        raw_count,
        "The `#[ignore]` regex matched {} of {raw_count} attribute lines. The \
         unmatched ones are either bare `#[ignore]` (add a `= \"requires: …\"` \
         reason) or formatted so the scan misses them, which would silently \
         drop them from every tier.",
        tests.len()
    );

    let mut failures = Vec::new();
    for t in &tests {
        if t.tokens.is_empty() {
            failures.push(format!(
                "{}::{}: {:?} — must start with \"requires: \"",
                t.rel, t.name, t.reason
            ));
            continue;
        }
        for token in &t.tokens {
            if !known.contains(token.as_str()) {
                failures.push(format!(
                    "{}::{}: unknown prerequisite {token:?} in {:?}",
                    t.rel, t.name, t.reason
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "#[ignore] reason strings must declare machine-readable prerequisites in \
         the form `requires: <token>[, <token>]* — <free text>`.\n\
         Hermetic tokens (run in the blocking `just test gated` tier): {IGNORE_TOKENS_HERMETIC:?}\n\
         External tokens (excluded from it): {IGNORE_TOKENS_EXTERNAL:?}\n\
         To add a token you must also wire it into the justfile tiers.\n  - {}",
        failures.join("\n  - ")
    );
}

// UNIQUE: the justfile's `gated` tier consumes `assets/test-tiers/external.txt`
// to decide what to EXCLUDE. If that file drifts from the reason strings, one
// of two silent failures follows: a test needing Ollama runs in the pre-commit
// gate (red on every machine without it), or a test needing nothing gets
// skipped — the blind spot the tier exists to close. nextest cannot filter on
// ignore reasons, only on names, so the name list has to be derived and
// checked in. This test is both halves: it regenerates the set from source and
// diffs it, and with CRUCIBLE_WRITE_TEST_TIERS=1 it writes the file instead.
// One parser, so generation and checking cannot disagree.
#[test]
fn external_test_tier_file_matches_the_ignore_reasons() {
    let root = workspace_root();
    let path = root.join(EXTERNAL_TIER_FILE);

    let (tests, _) = ignored_tests();
    let derived: BTreeSet<String> = tests
        .iter()
        .filter(|t| t.is_external())
        .map(|t| t.name.clone())
        .collect();

    if std::env::var_os(WRITE_TIERS_ENV).is_some() {
        let mut body = String::from(
            "# GENERATED by `just test tiers` — do not edit by hand.\n\
             #\n\
             # Ignored tests whose #[ignore] reason names an external\n\
             # prerequisite (see IGNORE_TOKENS_EXTERNAL in\n\
             # crates/crucible-daemon/tests/architecture_tests.rs). `just test\n\
             # gated` runs every ignored test EXCEPT these; `just test external`\n\
             # runs exactly these.\n",
        );
        for name in &derived {
            body.push_str(name);
            body.push('\n');
        }
        std::fs::create_dir_all(path.parent().unwrap()).expect("create assets/test-tiers");
        std::fs::write(&path, body).expect("write external tier file");
        eprintln!("wrote {} ({} tests)", path.display(), derived.len());
        return;
    }

    let contents = read(&path);
    let checked_in: BTreeSet<String> = contents
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();

    let missing: Vec<&String> = derived.difference(&checked_in).collect();
    let stale: Vec<&String> = checked_in.difference(&derived).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "{EXTERNAL_TIER_FILE} has drifted from the #[ignore] reason strings.\n\
         Regenerate with `just test tiers`.\n\
         Missing (external prerequisite, but the gated tier would run it):\n  + {}\n\
         Stale (listed, but no longer external — the gated tier is skipping it):\n  - {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  + "),
        stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

// UNIQUE: the tier filter is `test(<name>)`, a SUBSTRING match against the
// module-qualified test name — exact match is not available to us, because this
// gate derives names from source without a build and so cannot know the module
// path (`chat::chat_ctrl_c_exits`). Substring matching is only safe while every
// external name names exactly one ignored test and is not a substring of
// another ignored test's name (or of a module segment): either would silently
// pull a hermetic test out of the blocking gate. Rust scopes test names per
// module, so nothing else notices a collision.
#[test]
fn external_tier_test_names_are_unambiguous() {
    let (tests, _) = ignored_tests();

    // Only IGNORED tests can be affected. Both tiers run with `--run-ignored
    // ignored-only`, so `-E` never has a non-ignored test to include or
    // exclude: a collision with a normally-running test is inert. Scoping the
    // check here rather than over every `#[test]` fn in the workspace is what
    // keeps it from demanding renames that would change nothing (the first
    // draft flagged `test_fastembed_batch` against the un-ignored
    // `test_fastembed_batch_embedding`).
    let mut failures = Vec::new();
    for t in tests.iter().filter(|t| t.is_external()) {
        let same_name: Vec<&IgnoredTest> = tests.iter().filter(|o| o.name == t.name).collect();
        if same_name.len() > 1 {
            failures.push(format!(
                "{} is the name of {} ignored tests ({}) — rename so the \
                 external tier can name it unambiguously",
                t.name,
                same_name.len(),
                same_name
                    .iter()
                    .map(|o| o.rel.as_str())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        // `test(<name>)` is a substring match, so a longer ignored test name
        // containing this one is dragged out of the blocking gate with it.
        let shadowed: Vec<&str> = tests
            .iter()
            .filter(|o| o.name != t.name && o.name.contains(&t.name))
            .map(|o| o.name.as_str())
            .collect();
        if !shadowed.is_empty() {
            failures.push(format!(
                "{} is a substring of ignored test(s) {} — `test({})` would \
                 exclude those from the blocking gate too; rename one",
                t.name,
                shadowed.join(", "),
                t.name
            ));
        }

        // The match is against the module-qualified name, so a name equal to a
        // module segment would match every test in that module. Module segments
        // come from file stems, so comparing against the stems of files holding
        // ignored tests covers it.
        let as_module: Vec<&str> = tests
            .iter()
            .filter(|o| {
                o.rel.rsplit('/').next().and_then(|f| f.strip_suffix(".rs"))
                    == Some(t.name.as_str())
            })
            .map(|o| o.rel.as_str())
            .collect();
        if !as_module.is_empty() {
            failures.push(format!(
                "{} is also a module name ({}) — `test({})` would match every \
                 ignored test in that module; rename the test",
                t.name,
                as_module.join(", "),
                t.name
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "External tier entries must name exactly one ignored test:\n  - {}",
        failures.join("\n  - ")
    );
}

// ===========================================================================
// A6 — the wire-request-type table lives in `architecture_tests/wire_types.rs`.
// It is a wire contract, while this file is about module shape; see that
// module's header.
// ===========================================================================

// `#[path]` because this file is the test binary's crate root, so a bare `mod`
// would resolve to `tests/wire_types.rs` — and anything directly under `tests/`
// is compiled as its OWN integration-test target, which is not what this is.
#[path = "architecture_tests/wire_types.rs"]
mod wire_types;

// ===========================================================================
// A6 — a turn reads history before it writes to history.
// ===========================================================================

// UNIQUE: both statements are independently correct and neither the compiler
// nor a behavioural test can see the ordering — the loser of the race is a
// writer task in another module, and forcing the losing interleaving needs a
// `sleep` in production code. A source scan is the only standing enforcement.
#[test]
fn the_conversation_tree_is_rebuilt_before_the_turns_user_message_is_emitted() {
    let send =
        read(&workspace_root().join("crates/crucible-daemon/src/agent_manager/messaging/send.rs"));
    let body = fn_body(&send, "async fn send_message_inner(");

    let rebuild = body
        .find("get_or_rebuild_session_tree(")
        .expect("send_message_inner must fetch the session's conversation tree");
    let emit = body
        .find("SessionEventMessage::user_message(")
        .expect("send_message_inner must emit the turn's user_message event");

    assert!(
        rebuild < emit,
        "`get_or_rebuild_session_tree` must run BEFORE the `user_message` event \
         is emitted. The rebuild reads `session.jsonl`; a separate writer task \
         appends the emitted event to that same file. Emitting first races the \
         append, and when the append wins the rebuilt tree already holds this \
         turn's User node — `undo_depth() == 1` then reads false and the turn \
         runs with Precognition silently skipped."
    );
}
