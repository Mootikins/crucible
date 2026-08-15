//! Which tools bypass the permission gate.
//!
//! Split from `mod.rs` for the file-size ceiling, but the pair belongs
//! together on its own: this is the one decision that can skip the session
//! mode stance, its rules, every Lua `on_request` hook, the saved patterns
//! and the prompt. Read `is_safe`'s doc comment before widening either.

use std::collections::HashSet;

/// Check if a tool is safe to execute without requiring explicit permission.
///
/// Safe tools are read-only operations that only query data without modifying state.
/// Unsafe tools can modify files, execute commands, or change state and require permission.
///
/// # Safe Tools (is_safe=true)
///
/// **MCP Tools (10 read-only):**
/// - `semantic_search` — Search notes using semantic similarity
/// - `grep_notes` — Grep-style text search across notes
/// - `property_search` — Search notes by frontmatter properties (includes tags)
/// - `list_notes` — List notes in a directory
/// - `read_note` — Read note content with optional line range
/// - `read_metadata` — Read note metadata without loading full content
/// - `get_kiln_info` — Get comprehensive kiln information including root path and statistics
/// - `list_jobs` — List all background jobs (running and completed) for the current session
///
/// **Workspace Tools (Rig-native, 3 read-only):**
/// - `read_file` — Read file content
/// - `glob` — Find files matching patterns
/// - `grep` — Search file contents
///
/// # Unsafe Tools (is_safe=false)
///
/// **MCP Tools (6 mutating):**
/// - `create_note` — Create a new note in the kiln
/// - `update_note` — Update an existing note
/// - `delete_note` — Delete a note from the kiln
/// - `delegate_session` — Delegate a task to another AI agent
/// - `get_job_result` — Get the result of a background job
/// - `cancel_job` — Cancel a running background job by ID
///
/// **Workspace Tools (Rig-native, 3 mutating):**
/// - `write` — Write file content
/// - `edit` — Edit file content
/// - `bash` — Execute shell commands
///
/// # Default-Deny Policy
///
/// Only explicitly safe tools skip the permission prompt.
/// Everything unknown requires permission.
///
/// # Why this takes no MCP annotations
///
/// This answer decides whether the permission gate runs at all
/// (`messaging::tool_call`), and skipping the gate skips the session's mode
/// stance, its mode rules, every Lua `on_request` hook, the saved patterns and
/// the prompt. So it may only ever widen on something the daemon itself knows.
/// A tool name and a `readOnlyHint` both come from a third-party MCP server;
/// letting them widen this would let any upstream annotate its way past a
/// mode's `default = "deny"`. For what a server *claims*, see
/// [`believed_read_only`].
pub fn is_safe(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file"
            | "glob"
            | "grep"
            | "semantic_search"
            | "grep_notes"
            | "property_search"
            | "list_notes"
            | "read_note"
            | "read_metadata"
            | "get_kiln_info"
            | "list_jobs"
            // Progressive-disclosure bridge lookups are read-only.
            | "discover_tools"
            | "get_tool_schema"
    )
}

/// What we *believe* about a tool, including what its MCP server claims via
/// `readOnlyHint` (see
/// [`crate::tools::mcp_gateway::McpGatewayManager::read_only_tool_names`]).
///
/// Advisory, and deliberately separate from [`is_safe`]: this is surfaced to
/// Lua as `request.is_safe` so a policy hook can distinguish a read-only
/// upstream tool from one that writes — which is the whole point of reading
/// the annotation. It must never be used to skip the gate; a hook that trusts
/// it is making that call knowingly, on one tool, in its own policy.
///
/// A server that annotates nothing is not a server saying "writes": the
/// annotation is optional in MCP, so silence falls through to [`is_safe`].
pub fn believed_read_only(tool_name: &str, mcp_read_only: &HashSet<String>) -> bool {
    is_safe(tool_name) || mcp_read_only.contains(tool_name)
}
