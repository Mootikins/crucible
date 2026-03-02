//! Tool mode definitions for Crucible MCP tools.
//!
//! This module defines which tools are available in different operational modes,
//! independent of any LLM provider implementation.

/// Read-only tools available in "plan" mode.
///
/// These tools provide safe, non-mutating access to the knowledge base
/// and are suitable for planning and analysis workflows.
pub const PLAN_TOOL_NAMES: &[&str] = &[
    "semantic_search",
    "text_search",
    "property_search",
    "list_notes",
    "read_note",
    "read_metadata",
    "get_kiln_info",
    "list_jobs",
];
