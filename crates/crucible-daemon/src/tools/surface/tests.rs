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
use std::collections::BTreeSet;
use std::sync::Arc;
use strum::IntoEnumIterator;
use tempfile::TempDir;

/// Every tool name the daemon's own executors put in front of a model.
fn advertised_builtin_names() -> BTreeSet<String> {
    let temp = TempDir::new().expect("tempdir");
    let server = CrucibleMcpServer::new(
        temp.path().display().to_string(),
        Arc::new(EmptyKnowledgeRepository),
        Arc::new(EmptyEmbeddingProvider),
    );

    let mut names: BTreeSet<String> = server
        .list_tools()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();

    names.extend(
        WorkspaceTools::tool_definitions()
            .into_iter()
            .map(|t| t.name.to_string()),
    );

    // `list_tools` filters `delegate_session` out unless a `DelegationContext`
    // is wired, which needs spawner mocks this module has no business owning.
    // Naming it here keeps it inside the coverage claim rather than outside it.
    names.insert("delegate_session".to_string());

    // The progressive-disclosure bridge belongs to no executor —
    // `DaemonToolDispatcher` answers it out of its own catalog — so nothing
    // above lists it.
    names.extend(
        crate::tool_dispatch::DISCOVERY_TOOL_NAMES
            .iter()
            .map(|n| (*n).to_string()),
    );

    names
}

fn classified_names() -> BTreeSet<String> {
    BuiltinTool::iter().map(|t| t.name().to_string()).collect()
}

/// The gap rustc cannot close: a tool added to `CrucibleMcpServer` or
/// `WorkspaceTools` with no [`BuiltinTool`] variant.
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
