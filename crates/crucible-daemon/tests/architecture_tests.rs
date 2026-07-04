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

/// The argument list of the `session_config_*!(...)` macro invocation whose
/// first argument is `handler` — from the opening paren through its match.
fn macro_invocation(src: &str, handler: &str) -> String {
    let at = src
        .find(handler)
        .unwrap_or_else(|| panic!("handler not found: {handler}"));
    let open = src[..at]
        .rfind('(')
        .unwrap_or_else(|| panic!("no '(' before handler: {handler}"));
    balanced(src, open)
}

/// The source region that declares a server config handler: either the body of
/// a hand-written `fn <handler>(...)` or the argument list of the
/// `session_config_*!` macro invocation that generates it. The request/response
/// wire field-name literals appear verbatim in both forms, so the same regex
/// scans work regardless of which form a given knob uses.
fn server_decl(params: &str, handler: &str) -> String {
    if params.contains(&format!("fn {handler}(")) {
        fn_body(params, &format!("fn {handler}("))
    } else {
        macro_invocation(params, handler)
    }
}

fn captures(re: &str, hay: &str) -> BTreeSet<String> {
    let re = Regex::new(re).unwrap();
    re.captures_iter(hay).map(|c| c[1].to_string()).collect()
}

// ===========================================================================
// A1 — RPC field-name parity for session config get/set pairs.
//
// The historical bug class: the client serializes a request field under one
// JSON name (e.g. `thinking_budget`) while the daemon handler reads a
// different name (e.g. `budget`), so the value is silently dropped. These
// gates diff the field-name *sets* used on each side of the wire per method.
//
// The server handlers are generated by the `session_config_setter!` /
// `session_config_getter!` macros in `server/session/params.rs` (a knob that
// deviates from the uniform shape stays hand-written). The gate reads the wire
// field name from whichever form a knob uses: `server_decl` returns either the
// hand-written fn body or the macro invocation's argument list, and the same
// literal-scanning regexes apply to both. All get/set knobs are covered.
//
// Adding a knob is one row in CONFIG_METHODS.
// ===========================================================================

struct ConfigMethod {
    /// The `session.{set,get}_<suffix>` method-name stem, which is also the
    /// stem of both the client (`session_{set,get}_<suffix>`) and server
    /// (`handle_session_{set,get}_<suffix>`) function names.
    suffix: &'static str,
    /// JSON field the *request* carries (besides `session_id`): the client's
    /// request-struct field name must equal the server's param-read name.
    request_field: &'static str,
    /// JSON field the *response* carries (besides `session_id`): the server's
    /// result-key name must equal the name the client reads back.
    result_field: &'static str,
}

const CONFIG_METHODS: &[ConfigMethod] = &[
    ConfigMethod {
        suffix: "thinking_budget",
        request_field: "thinking_budget",
        result_field: "thinking_budget",
    },
    ConfigMethod {
        suffix: "system_prompt",
        request_field: "system_prompt",
        result_field: "system_prompt",
    },
    ConfigMethod {
        suffix: "precognition",
        request_field: "precognition_enabled",
        result_field: "precognition_enabled",
    },
    ConfigMethod {
        suffix: "precognition_results",
        request_field: "precognition_results",
        result_field: "precognition_results",
    },
    ConfigMethod {
        suffix: "temperature",
        request_field: "temperature",
        result_field: "temperature",
    },
    ConfigMethod {
        suffix: "max_tokens",
        request_field: "max_tokens",
        result_field: "max_tokens",
    },
    ConfigMethod {
        suffix: "max_iterations",
        request_field: "max_iterations",
        result_field: "max_iterations",
    },
    ConfigMethod {
        suffix: "execution_timeout",
        request_field: "timeout_secs",
        result_field: "timeout_secs",
    },
    ConfigMethod {
        suffix: "context_budget",
        request_field: "context_budget",
        result_field: "context_budget",
    },
    ConfigMethod {
        suffix: "context_strategy",
        request_field: "context_strategy",
        result_field: "context_strategy",
    },
    ConfigMethod {
        suffix: "context_window",
        request_field: "context_window",
        result_field: "context_window",
    },
    ConfigMethod {
        suffix: "output_validation",
        request_field: "output_validation",
        result_field: "output_validation",
    },
    ConfigMethod {
        suffix: "validation_retries",
        request_field: "validation_retries",
        result_field: "validation_retries",
    },
    ConfigMethod {
        suffix: "autocompact_threshold",
        request_field: "autocompact_threshold",
        result_field: "autocompact_threshold",
    },
];

const SESSION_ID: &str = "session_id";

#[test]
fn rpc_config_field_names_match_across_the_wire() {
    let root = workspace_root();
    let client = read(&root.join("crates/crucible-daemon/src/rpc_client/client/agent.rs"));
    let server = read(&root.join("crates/crucible-daemon/src/server/session/params.rs"));

    let mut failures = Vec::new();

    for m in CONFIG_METHODS {
        // --- request parity (the `set` direction) ---------------------------
        let client_set = fn_body(&client, &format!("fn session_set_{}(", m.suffix));
        let server_set = server_decl(&server, &format!("handle_session_set_{}", m.suffix));

        // Client struct-init fields: line-anchored `field:` / `field,`.
        let mut client_req = captures(r"(?m)^\s*([a-z_][a-z0-9_]*)\s*[:,]", &client_set);
        client_req.remove(SESSION_ID);

        // Server param reads: `require_param!`/`optional_param!(req, "field", ..)`.
        // For a macro-generated handler this scans the extractor argument of the
        // `session_config_setter!` invocation; for a hand-written one, its body.
        let mut server_req = captures(
            r#"(?:require_param|optional_param)!\s*\(\s*req\s*,\s*"([^"]+)""#,
            &server_set,
        );
        server_req.remove(SESSION_ID);

        let expected: BTreeSet<String> = [m.request_field.to_string()].into_iter().collect();
        if client_req != expected {
            failures.push(format!(
                "session.set_{}: client sends request fields {client_req:?}, expected {expected:?}",
                m.suffix
            ));
        }
        if server_req != expected {
            failures.push(format!(
                "session.set_{}: server reads request fields {server_req:?}, expected {expected:?} \
                 (client sends {client_req:?})",
                m.suffix
            ));
        }

        // --- result parity (the `get` direction) ----------------------------
        let client_get = fn_body(&client, &format!("fn session_get_{}(", m.suffix));
        let server_get = server_decl(&server, &format!("handle_session_get_{}", m.suffix));

        // Client result reads: bare `"field"` literals (method names contain
        // `.` and so never match this identifier-only pattern).
        let mut client_res = captures(r#""([a-z_][a-z0-9_]*)""#, &client_get);
        client_res.remove(SESSION_ID);

        // Server result field: bare `"field"` literals. This matches both the
        // macro invocation's field argument (`session_config_getter!(.., "field")`)
        // and a hand-written handler's `"field":` response-json key. Method-name
        // strings contain `.` and so never match this identifier-only pattern.
        let mut server_res = captures(r#""([a-z_][a-z0-9_]*)""#, &server_get);
        server_res.remove(SESSION_ID);

        let expected: BTreeSet<String> = [m.result_field.to_string()].into_iter().collect();
        if client_res != expected {
            failures.push(format!(
                "session.get_{}: client reads result fields {client_res:?}, expected {expected:?}",
                m.suffix
            ));
        }
        if server_res != expected {
            failures.push(format!(
                "session.get_{}: server returns result fields {server_res:?}, expected {expected:?} \
                 (client reads {client_res:?})",
                m.suffix
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "RPC field-name parity violations (fix the client/server field name, \
         not this test):\n  - {}",
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
// A4 — module-size ratchet.
//
// No source file may exceed 1000 lines except those in the frozen ledger
// below. The ledger was generated from the tree's current state; entries may
// only be REMOVED (by splitting the file), never added. A brand-new oversized
// file fails this gate.
// ===========================================================================

const LINE_BUDGET: usize = 1000;

/// Files already over the line budget when this gate was introduced.
/// Sorted; entries may only be removed (split the file), never added.
const SIZE_LEDGER: &[&str] = &[
    "crates/crucible-cli/src/commands/tasks.rs",
    "crates/crucible-cli/src/tui/oil/chat_app/command_handling.rs",
    "crates/crucible-cli/src/tui/oil/components/diff_view.rs",
    "crates/crucible-cli/src/tui/oil/components/tool_render.rs",
    "crates/crucible-cli/src/tui/oil/containers.rs",
    "crates/crucible-cli/src/tui/oil/tests/component_isolation_tests.rs",
    "crates/crucible-cli/src/tui/oil/tests/e2e_debug_test.rs",
    "crates/crucible-cli/src/tui/oil/tests/vt100_runtime.rs",
    "crates/crucible-cli/src/web/routes/session.rs",
    "crates/crucible-cli/src/web/services/daemon.rs",
    "crates/crucible-core/src/config/components/backend.rs",
    "crates/crucible-core/src/config/components/llm.rs",
    "crates/crucible-core/src/config/config/cli_app.rs",
    "crates/crucible-core/src/config/enrichment.rs",
    "crates/crucible-core/src/events/session_event/internal.rs",
    "crates/crucible-core/src/parser/block_extractor.rs",
    "crates/crucible-core/src/parser/types/task.rs",
    "crates/crucible-core/src/workflow/engine.rs",
    "crates/crucible-daemon/src/agent_factory.rs",
    "crates/crucible-daemon/src/agent_manager/messaging/permission.rs",
    "crates/crucible-daemon/src/agent_manager/messaging/stream.rs",
    "crates/crucible-daemon/src/agent_manager/models.rs",
    "crates/crucible-daemon/src/agent_manager/mod.rs",
    "crates/crucible-daemon/src/agent_manager/tests/messaging.rs",
    "crates/crucible-daemon/src/agent_manager/tests/models/list.rs",
    "crates/crucible-daemon/src/agent_manager/tests/models_discovery.rs",
    "crates/crucible-daemon/src/daemon_plugins.rs",
    "crates/crucible-daemon/src/kiln_manager.rs",
    "crates/crucible-daemon/src/provider/genai_handle.rs",
    "crates/crucible-daemon/src/rpc/dispatch.rs",
    "crates/crucible-daemon/src/rpc_client/client/mod.rs",
    "crates/crucible-daemon/src/session_bridge.rs",
    "crates/crucible-daemon/src/session_manager.rs",
    "crates/crucible-daemon/src/storage/lance/note_store.rs",
    "crates/crucible-daemon/src/storage/sqlite/note_store.rs",
    "crates/crucible-daemon/src/tools/mcp_server.rs",
    "crates/crucible-daemon/src/tools/search.rs",
    "crates/crucible-daemon/src/watch/types.rs",
    "crates/crucible-lua/src/annotations.rs",
    "crates/crucible-lua/src/graph.rs",
    "crates/crucible-lua/src/oil.rs",
    "crates/crucible-lua/src/theme.rs",
    "crates/crucible-oil/src/template/node_spec.rs",
];

#[test]
fn no_new_oversized_modules() {
    let ledger: BTreeSet<&str> = SIZE_LEDGER.iter().copied().collect();
    let mut offenders = Vec::new();
    for (rel, contents) in workspace_src_files() {
        let lines = contents.lines().count();
        if lines > LINE_BUDGET && !ledger.contains(rel.as_str()) {
            offenders.push(format!("{rel} ({lines} lines)"));
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "New file(s) exceed the {LINE_BUDGET}-line budget. SPLIT the file into \
         focused modules — do NOT add it to SIZE_LEDGER (the ledger only \
         shrinks). Offending files:\n  - {}",
        offenders.join("\n  - ")
    );
}
