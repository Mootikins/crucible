//! What the compiler cannot check about [`BuiltinTool`].
//!
//! rustc guarantees every *variant* is classified. It cannot guarantee the set
//! of variants matches the set of tools the daemon actually advertises, nor
//! that a classification is the honest one. These tests cover both gaps, and
//! `strum::EnumIter` is what makes them total — an iteration written by hand
//! would go stale in exactly the way the enum exists to prevent.

use super::*;
use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
use crate::tools::mcp_server::{CrucibleMcpServer, KILN_BACKED_TOOLS};
use crate::tools::workspace::WorkspaceTools;
use crucible_core::traits::tools::ToolExecutor;
use std::collections::BTreeSet;
use std::sync::Arc;
use strum::IntoEnumIterator;
use tempfile::TempDir;

fn classified_names() -> BTreeSet<String> {
    BuiltinTool::iter().map(|t| t.name().to_string()).collect()
}

/// The gap rustc cannot close: a tool added to `CrucibleMcpServer` or
/// `WorkspaceTools` with no [`BuiltinTool`] variant.
///
/// Asserted over [`advertised_builtin_names`], which is the *whole* catalog:
/// `CrucibleMcpServer::all_tool_names`, never the session-filtered
/// `list_tools`. The filtered set is what a particular session may call — it
/// drops `delegate_session` without a delegation context and every
/// `KILN_BACKED_TOOLS` name on a kiln-less server — so a coverage claim built
/// from it silently excuses exactly the tools some session shape hides, and
/// has to hand-patch names back in to stay honest.
///
/// It fails *closed* at runtime — [`classify`] answers `Unknown` and the
/// isolation gate refuses it — which is the safe direction but a silent one,
/// discovered by a user whose sandboxed session lost a tool. This turns it into
/// a build failure that names the tool.
///
/// The reverse direction matters too: a variant nothing advertises is a
/// classification for a tool that no longer exists, and the next reader trusts
/// it.
#[test]
fn builtin_surfaces_cover_every_advertised_tool() {
    let advertised = advertised_builtin_names();
    let classified = classified_names();

    let unclassified: Vec<_> = advertised.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "these tools are advertised to models but have no BuiltinTool variant, so \
         classify() answers Unknown and an isolated session silently refuses them: \
         {unclassified:?}"
    );

    let stale: Vec<_> = classified.difference(&advertised).collect();
    assert!(
        stale.is_empty(),
        "these BuiltinTool variants classify tools nothing advertises: {stale:?}"
    );
}

/// The semantic relationship behind the whole `Host` classification: the
/// executor that takes an arbitrary host path or runs a host process must not
/// have a single tool classified as reaching daemon state only.
///
/// This is the test that would have caught the original defect had the surfaces
/// been per tool from the start, and it is what stops a seventh workspace tool
/// arriving as `Daemon`.
#[test]
fn every_workspace_tool_is_host_surface() {
    for tool in WorkspaceTools::tool_definitions() {
        assert_eq!(
            classify(&tool.name),
            ToolSurface::Host,
            "'{}' is served by WorkspaceTools, which resolves caller-supplied host \
             paths and spawns host processes; anything but Host lets it run inside a \
             session the user containerized",
            tool.name
        );
    }
}

/// Kiln tools surviving a sandbox is the property `Daemon` exists for —
/// default-deny by name is what made "turning on the sandbox turns off
/// Crucible" true. Asserted over the same constant the kiln-less filter uses,
/// so a kiln tool added later is covered without editing this test.
#[test]
fn every_kiln_backed_tool_is_daemon_surface() {
    for name in KILN_BACKED_TOOLS {
        assert_eq!(
            classify(name),
            ToolSurface::Daemon,
            "'{name}' is a kiln tool; refusing it inside an isolated session turns \
             off Crucible rather than containing it"
        );
    }
}

/// `Daemon` is the only classification that *passes* an isolation claim, so
/// every member is a decision to let a tool run while the user believes the
/// session is contained. Listing the set here means widening it takes two
/// edits: the `match`, and this literal. Narrowing it — the safe direction —
/// takes one.
#[test]
fn the_daemon_classified_set_is_exactly_the_reviewed_one() {
    let reviewed: BTreeSet<&str> = [
        "cancel_job",
        "create_note",
        "delegate_session",
        "delete_note",
        "discover_tools",
        "get_job_result",
        "get_kiln_info",
        "get_tool_schema",
        "grep_notes",
        "list_jobs",
        "list_notes",
        "property_search",
        "read_metadata",
        "read_note",
        "semantic_search",
        "skill_view",
        "update_note",
    ]
    .into_iter()
    .collect();

    let actual: BTreeSet<&str> = BuiltinTool::iter()
        .filter(|t| t.surface() == ToolSurface::Daemon)
        .map(BuiltinTool::name)
        .collect();

    assert_eq!(
        actual, reviewed,
        "a tool moved into or out of the set that passes an isolation claim. \
         Adding one means a sandboxed session can now run it — say so here \
         deliberately, or classify it Host."
    );
}

/// `from_name` is the only way a wire name reaches a classification, so a
/// variant missing from it is a tool that silently drops to `Unknown`.
#[test]
fn every_variant_round_trips_through_its_wire_name() {
    for tool in BuiltinTool::iter() {
        assert_eq!(
            BuiltinTool::from_name(tool.name()),
            Some(tool),
            "'{}' does not resolve back to its own variant",
            tool.name()
        );
    }
}

/// An executor under test, labelled by the type that provides it.
type LabelledExecutor = (&'static str, Arc<dyn ToolExecutor>);

/// Every executor a production dispatcher registers, in the order
/// `AgentManager::tool_dispatcher` builds them.
///
/// The `TempDir` comes back with them because `WorkspaceTools` anchors on a
/// directory; nothing here executes a tool, but the root must outlive the
/// executor.
fn production_executors() -> (TempDir, Vec<LabelledExecutor>) {
    let temp = TempDir::new().expect("tempdir");

    let mcp = Arc::new(CrucibleMcpServer::new(
        temp.path().display().to_string(),
        Arc::new(EmptyKnowledgeRepository),
        Arc::new(EmptyEmbeddingProvider),
    ));
    let gateway = Arc::new(tokio::sync::RwLock::new(
        crate::tools::mcp_gateway::McpGatewayManager::new(),
    ));

    let executors: Vec<LabelledExecutor> = vec![
        (
            "WorkspaceTools",
            Arc::new(WorkspaceTools::new(temp.path())) as Arc<dyn ToolExecutor>,
        ),
        (
            "McpToolExecutor",
            Arc::new(crate::tool_dispatch::McpToolExecutor::new(mcp)),
        ),
        (
            "GatewayToolExecutor",
            Arc::new(crate::tools::gateway_executor::GatewayToolExecutor::new(
                gateway,
                vec!["any".to_string()],
            )),
        ),
        (
            "PluginToolExecutor",
            Arc::new(crate::plugin_tools::PluginToolExecutor::new(Arc::new(
                crate::plugin_tools::PluginRegistry::new(),
            ))),
        ),
    ];

    (temp, executors)
}

/// The hole the exhaustiveness check does not cover: `ToolExecutor::surface`
/// is free-form.
///
/// rustc binds every name routed through [`classify`], but nothing forces an
/// executor to route. `surface(&self, tool: &str)` may ignore its argument
/// entirely — a provider advertising `dump_env` and `run_migration` while
/// answering `ToolSurface::Daemon` for every name compiles, passes the whole
/// suite, and hands itself an exemption from the isolation gate for two tools
/// nobody classified.
///
/// So: an executor may always answer `Unknown` — that is the floor, and it is
/// what the executors serving foreign code answer for everything they run —
/// but a claim of anything else has to be one the table gives that same name.
/// No executor invents a classification.
#[tokio::test]
async fn no_executor_claims_a_surface_the_table_does_not_give_that_name() {
    let (_temp, executors) = production_executors();

    for (label, executor) in executors {
        for def in executor.list_tools().await.expect("list_tools") {
            let claimed = executor.surface(&def.name);
            if claimed == ToolSurface::Unknown {
                continue;
            }
            assert_eq!(
                claimed,
                classify(&def.name),
                "{label} advertises '{}' and answers {claimed:?} for it, which is not \
                 what the built-in table says. An executor may answer Unknown about \
                 anything, but a classification it invents is a sandbox exemption \
                 nobody reviewed",
                def.name
            );
            assert_ne!(
                classify(&def.name),
                ToolSurface::Unknown,
                "{label} advertises '{}' with a classification, but the table does \
                 not know the name — add a BuiltinTool variant for it",
                def.name
            );
        }
    }
}

/// The other half of D1's rule, stated as a property of the executors that
/// serve foreign code: they answer `Unknown` even for a name the table knows,
/// so a plugin tool or an upstream MCP tool called `read_note` cannot borrow
/// the built-in's `Daemon`.
#[test]
fn a_foreign_executor_answers_unknown_even_for_a_builtin_name() {
    let gateway = Arc::new(tokio::sync::RwLock::new(
        crate::tools::mcp_gateway::McpGatewayManager::new(),
    ));
    let gw = crate::tools::gateway_executor::GatewayToolExecutor::new(gateway, vec![]);
    let plugins = crate::plugin_tools::PluginToolExecutor::new(Arc::new(
        crate::plugin_tools::PluginRegistry::new(),
    ));

    for name in ["read_note", "semantic_search", "bash"] {
        assert_eq!(gw.surface(name), ToolSurface::Unknown, "gateway '{name}'");
        assert_eq!(
            plugins.surface(name),
            ToolSurface::Unknown,
            "plugin '{name}'"
        );
    }
}

/// An executor answers only for what it runs.
///
/// `WorkspaceTools` handed back the whole built-in table, so asking it about
/// `create_note` produced `Daemon` — a classification describing a different
/// executor — and `DaemonToolsBridge::isolation_refusal` asks exactly this
/// question of exactly this executor.
#[test]
fn workspace_tools_does_not_answer_for_tools_it_does_not_serve() {
    let temp = TempDir::new().expect("tempdir");
    let tools = WorkspaceTools::new(temp.path());

    assert_eq!(tools.surface("bash"), ToolSurface::Host);
    for name in ["create_note", "semantic_search", "delegate_session"] {
        assert_eq!(
            tools.surface(name),
            ToolSurface::Unknown,
            "'{name}' is not served by WorkspaceTools; answering for it makes this \
             executor an authority on a tool it cannot run"
        );
    }
}

/// Absence denies.
///
/// This is the behaviour the per-executor answer got wrong: `McpToolExecutor`
/// replied `Daemon` for any name at all, including ones it had never heard of,
/// because the answer described the executor rather than the tool.
#[test]
fn an_unclassified_name_is_unknown_not_daemon() {
    assert_eq!(classify("a_tool_added_next_year"), ToolSurface::Unknown);
    assert_eq!(classify(""), ToolSurface::Unknown);
    assert_eq!(classify("read_note "), ToolSurface::Unknown);
}
