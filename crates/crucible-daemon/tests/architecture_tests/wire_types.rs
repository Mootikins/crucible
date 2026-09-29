//! A6 — a request type that derives `Deserialize` must be the type the server
//! deserializes.
//!
//! Split out of `architecture_tests.rs` on 2026-08-19: the S2 conversion added
//! 26 `WIRE_REQUEST_TYPES` rows, and this table is a *wire contract* while the
//! rest of `architecture_tests.rs` is about module shape, so it is a real seam
//! rather than an exemption.
//!
//! `mod` of the parent test binary, so it shares `workspace_root`, `read` and
//! `captures` rather than growing a second copy.

use super::{captures, read, workspace_root};
use regex::Regex;
use std::collections::BTreeSet;

const WIRE_REQUEST_TYPES: &[(&str, &str)] = &[
    (
        "SessionCreateRequest",
        "crates/crucible-daemon/src/server/session/create.rs",
    ),
    (
        "Scoped<LuaSessionInit>",
        "crates/crucible-daemon/src/server/lua.rs",
    ),
    ("Scoped<()>", "crates/crucible-daemon/src/server/lua.rs"),
    (
        "LuaDiscoverPluginsRequest",
        "crates/crucible-daemon/src/server/lua.rs",
    ),
    (
        "LuaPluginHealthRequest",
        "crates/crucible-daemon/src/server/lua.rs",
    ),
    (
        "LuaGenerateStubsRequest",
        "crates/crucible-daemon/src/server/lua.rs",
    ),
    (
        "Scoped<LuaCommands>",
        "crates/crucible-daemon/src/server/lua.rs",
    ),
    (
        "McpStartRequest",
        "crates/crucible-daemon/src/server/platform.rs",
    ),
    (
        "SkillsListRequest",
        "crates/crucible-daemon/src/server/platform.rs",
    ),
    (
        "SkillsGetRequest",
        "crates/crucible-daemon/src/server/platform.rs",
    ),
    (
        "SkillsSearchRequest",
        "crates/crucible-daemon/src/server/platform.rs",
    ),
    // `NameRequest` carries a single `name` and serves two methods:
    // `agents.resolve_profile` (platform.rs) and `plugin.reload` (plugins.rs).
    // One row per server file, because the row IS "this file deserializes it".
    (
        "NameRequest",
        "crates/crucible-daemon/src/server/platform.rs",
    ),
    (
        "KilnOpenRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "SearchVectorsRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "ProcessFileRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "GrepSearchRequest",
        "crates/crucible-daemon/src/server/grep.rs",
    ),
    (
        "NoteRenameRequest",
        "crates/crucible-daemon/src/server/note_refactor.rs",
    ),
    (
        "FsListDirRequest",
        "crates/crucible-daemon/src/server/fs/mod.rs",
    ),
    (
        "FsMoveRequest",
        "crates/crucible-daemon/src/server/fs/mod.rs",
    ),
    (
        "FsPathRequest",
        "crates/crucible-daemon/src/server/fs/mod.rs",
    ),
    (
        "NameRequest",
        "crates/crucible-daemon/src/server/plugins.rs",
    ),
    (
        "PluginPublicationsRequest",
        "crates/crucible-daemon/src/server/plugins.rs",
    ),
    (
        "PluginOptionsRequest",
        "crates/crucible-daemon/src/server/plugins.rs",
    ),
    (
        "PluginOptionCallRequest",
        "crates/crucible-daemon/src/server/plugins.rs",
    ),
    (
        "PluginRunCommandRequest",
        "crates/crucible-daemon/src/server/plugins.rs",
    ),
    (
        "PathRequest",
        "crates/crucible-daemon/src/server/plugins.rs",
    ),
    (
        "ScmCloneRequest",
        "crates/crucible-daemon/src/server/plugins.rs",
    ),
    ("Scoped<()>", "crates/crucible-daemon/src/server/plugins.rs"),
    (
        "PluginInstallRequest",
        "crates/crucible-daemon/src/server/plugin_install.rs",
    ),
    (
        "PluginRemoveRequest",
        "crates/crucible-daemon/src/server/plugin_install.rs",
    ),
    (
        "SessionReplayRequest",
        "crates/crucible-daemon/src/server/session/lifecycle.rs",
    ),
    // `Scoped<()>` is the params of each method that names only a session. A
    // dozen methods share it, so it has a row for each server file that
    // deserializes it: the row says "this file reads the wire contract, and
    // does not spell `session_id` again". A session-scoped method names its
    // row as `Scoped<Body>`, the turbofish that the file writes.
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/lifecycle.rs",
    ),
    (
        "Scoped<Page>",
        "crates/crucible-daemon/src/server/session/lifecycle.rs",
    ),
    (
        "Scoped<MarkdownOptions>",
        "crates/crucible-daemon/src/server/observe.rs",
    ),
    (
        "Scoped<ExportOptions>",
        "crates/crucible-daemon/src/server/observe.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/list.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/modes.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/models.rs",
    ),
    (
        "Scoped<ForkPoint>",
        "crates/crucible-daemon/src/server/session/models.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/notifications.rs",
    ),
    (
        "Scoped<NotificationKey>",
        "crates/crucible-daemon/src/server/session/notifications.rs",
    ),
    (
        "NotificationListRequest",
        "crates/crucible-daemon/src/server/notifications.rs",
    ),
    (
        "NotificationDismissRequest",
        "crates/crucible-daemon/src/server/notifications.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/messaging.rs",
    ),
    (
        "Scoped<AgentConfig>",
        "crates/crucible-daemon/src/server/session/messaging.rs",
    ),
    (
        "Scoped<ContextInjection>",
        "crates/crucible-daemon/src/server/session/messaging.rs",
    ),
    (
        "Scoped<InteractionAnswer>",
        "crates/crucible-daemon/src/server/session/messaging.rs",
    ),
    (
        "Scoped<TestInteraction>",
        "crates/crucible-daemon/src/server/session/messaging.rs",
    ),
    (
        "ListAllModelsRequest",
        "crates/crucible-daemon/src/server/session/models.rs",
    ),
    (
        "ListProvidersRequest",
        "crates/crucible-daemon/src/server/session/models.rs",
    ),
    (
        "LuaRunPluginTestsRequest",
        "crates/crucible-daemon/src/server/lua_plugin_suite.rs",
    ),
    // `session.set_title` and `session.generate_title` answer on the
    // `Result<Value, RpcError>` path, so they deserialize with
    // `parse_params::<T>` rather than `typed_params::<T>`. The gate reads the
    // turbofish, not the helper's name.
    (
        "Scoped<Title>",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    ("Scoped<()>", "crates/crucible-daemon/src/rpc/dispatch.rs"),
    // Step 6, part 4 of the simplification plan: the handlers that read
    // their fields by hand with `require_param!` now deserialize these.
    ("PathRequest", "crates/crucible-daemon/src/server/kiln.rs"),
    (
        "SearchTextRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "EmbedQueryRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "ListNotesRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    ("NoteRef", "crates/crucible-daemon/src/server/kiln.rs"),
    ("KilnRef", "crates/crucible-daemon/src/server/kiln.rs"),
    (
        "NoteUpsertRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "NotePathRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "ProcessBatchRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "SuggestLinksRequest",
        "crates/crucible-daemon/src/server/kiln.rs",
    ),
    (
        "SessionListPersistedRequest",
        "crates/crucible-daemon/src/server/observe.rs",
    ),
    (
        "SessionCleanupRequest",
        "crates/crucible-daemon/src/server/observe.rs",
    ),
    (
        "Scoped<PluginApprovalChange>",
        "crates/crucible-daemon/src/server/session/approval.rs",
    ),
    (
        "Scoped<PluginRef>",
        "crates/crucible-daemon/src/server/session/approval.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/approval.rs",
    ),
    (
        "SessionListRequest",
        "crates/crucible-daemon/src/server/session/list.rs",
    ),
    (
        "SessionSearchRequest",
        "crates/crucible-daemon/src/server/session/list.rs",
    ),
    (
        "Scoped<MessageInput>",
        "crates/crucible-daemon/src/server/session/messaging.rs",
    ),
    (
        "Scoped<NewNotification>",
        "crates/crucible-daemon/src/server/session/notifications.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/server/session/params.rs",
    ),
    (
        "Scoped<KnobValue>",
        "crates/crucible-daemon/src/server/session/params.rs",
    ),
    (
        "Scoped<KnobRef>",
        "crates/crucible-daemon/src/server/session/params.rs",
    ),
    (
        "Scoped<UndoCount>",
        "crates/crucible-daemon/src/server/session/params.rs",
    ),
    (
        "Scoped<NamedKiln>",
        "crates/crucible-daemon/src/server/session/scope.rs",
    ),
    (
        "Scoped<WorkspaceChoice>",
        "crates/crucible-daemon/src/server/session/scope.rs",
    ),
    // Step 12 of the simplification plan: the handlers that read their params
    // with a struct local to the handler now deserialize these core types.
    (
        "SessionSubscribeRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    (
        "LuaEvalRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    (
        "ConfigLookupRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    (
        "ConfigKeyRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    (
        "ConfigValuesRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    (
        "SubagentCollectRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    (
        "WebhookReceiveRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
    (
        "Scoped<WorkflowSource>",
        "crates/crucible-daemon/src/rpc/workflow_handlers.rs",
    ),
    (
        "Scoped<GateRef>",
        "crates/crucible-daemon/src/rpc/workflow_handlers.rs",
    ),
    (
        "Scoped<()>",
        "crates/crucible-daemon/src/rpc/workflow_handlers.rs",
    ),
    // `resolve_workspace_target` reads `workspace_target` before the create
    // handler runs, with `parse_params::<T>` on the `RpcResult` path.
    (
        "SessionCreateRequest",
        "crates/crucible-daemon/src/rpc/dispatch.rs",
    ),
];

/// Handlers that still hand-pluck their fields. REMOVE rows; never add.
///
/// Empty: every table row above deserializes its request type. Keep the
/// constant — a new `*Request` whose handler hand-plucks gets one row here with
/// a reason, and the gate's other direction then forces the row out again the
/// moment it is converted.
const HAND_PLUCKED_LEDGER: &[&str] = &[];

// UNIQUE: rustc is perfectly happy for a `Deserialize` struct to describe a
// payload nobody deserializes — the derive compiles, the client serializes, and
// the server reads whatever field names it chose to type out. Nothing but a
// source scan can assert that the two are the same act.
#[test]
fn wire_request_types_are_deserialized_not_hand_plucked() {
    let root = workspace_root();
    let ledger: BTreeSet<&str> = HAND_PLUCKED_LEDGER.iter().copied().collect();
    let declared: BTreeSet<&str> = WIRE_REQUEST_TYPES.iter().map(|(s, _)| *s).collect();

    let mut failures = Vec::new();

    for stale in ledger.difference(&declared) {
        failures.push(format!(
            "HAND_PLUCKED_LEDGER row `{stale}` is not in WIRE_REQUEST_TYPES — \
             remove it"
        ));
    }

    for (struct_name, server_file) in WIRE_REQUEST_TYPES {
        let contents = read(&root.join(server_file));
        // The TURBOFISH is the marker, not any particular function: it names the
        // type at the call site, so `typed_params::<T>(&req)` and
        // `serde_json::from_value::<T>(…)` both count and a future wrapper needs
        // no change here. An annotated binding would let the type drift out of
        // view of a source scan, which is why every call site turbofishes.
        let turbofish = Regex::new(&format!(
            r"::<\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*{}\s*,?\s*>",
            regex::escape(struct_name)
        ))
        .unwrap();
        let deserializes = turbofish.is_match(&contents);
        let ledgered = ledger.contains(*struct_name);

        if !deserializes && !ledgered {
            failures.push(format!(
                "{server_file} does not deserialize {struct_name} — replace the \
                 field reads by hand with `typed_params::<{struct_name}>(&req)`, \
                 or (temporarily) add `{struct_name}` to HAND_PLUCKED_LEDGER \
                 with a reason"
            ));
        }
        if deserializes && ledgered {
            failures.push(format!(
                "{server_file} now deserializes {struct_name} — REMOVE \
                 `{struct_name}` from HAND_PLUCKED_LEDGER (the ledger only shrinks)"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Wire request types must be deserialized, not re-derived field by field:\n  - {}",
        failures.join("\n  - ")
    );
}

// UNIQUE: a `Deserialize` request type whose fields no handler reads is
// invisible to every other check — the derive compiles and the client happily
// serializes it. §P0's `LuaInitSessionRequest.config` lived like that. This
// asserts the table above stays honest about which structs exist, so a NEW
// `*Request` type cannot quietly skip the gate.
#[test]
fn every_lua_request_type_is_in_the_wire_table() {
    let root = workspace_root();
    let lua_client = read(&root.join("crates/crucible-daemon/src/rpc_client/client/lua.rs"));
    let defined = captures(r"pub struct (Lua[A-Za-z0-9]+Request)\b", &lua_client);
    let tabled: BTreeSet<String> = WIRE_REQUEST_TYPES
        .iter()
        .map(|(s, _)| s.to_string())
        .collect();

    // No exceptions: every `Lua*Request` the client defines has a row naming
    // the file that deserializes it, `LuaRunPluginTestsRequest` included (its
    // handler lives in `server/lua_plugin_suite.rs`, which is what its row
    // says).
    let missing: Vec<&String> = defined.difference(&tabled).collect();
    assert!(
        missing.is_empty(),
        "Lua request types with no WIRE_REQUEST_TYPES row — add one naming the \
         server file that deserializes it: {missing:?}"
    );
}
