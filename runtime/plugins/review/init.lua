--- review — the attributed-diff review queue, as tools an agent can call.
---
--- The daemon owns review; this plugin is the *tool* surface over it. Both it
--- and the web panel go through the same `review.*` operations, so a hunk
--- rejected by an agent and a hunk rejected by a human are reverted the same
--- way and reported the same way.
---
--- The reason this exists at all is delegation. A delegating agent spawns a
--- sub-session, the sub-session edits the workspace, and the delegator is the
--- one who should read that diff before accepting it — that is a review, and
--- it cannot happen over an RPC surface only the browser speaks. So every tool
--- here takes `session_id` explicitly: the session under review is usually
--- *not* the caller's own.
---
--- Get the id of a session you delegated to from `delegate_session`'s result
--- (`child_session_id`).
---
--- The action unit is the composed hunk, never a tool call: every hunk is
--- independently revertible against the session's base tree. `external` hunks
--- (`tool_call_ids` empty) are somebody's own edits — they are listed so the
--- diff stays honest, and they cannot be rejected.

local M = {}

--- Everything the reviewer needs to decide, and nothing that would blow up a
--- context window. Full hunk bodies are deliberately truncated: an agent
--- deciding on a 400-line hunk should open the file, not read it inline.
local MAX_CONTENT = 2000

local function truncate(s)
    if not s then
        return ""
    end
    if #s <= MAX_CONTENT then
        return s
    end
    return s:sub(1, MAX_CONTENT) .. "\n… (truncated; open the file to read the rest)"
end

local function project(hunk)
    return {
        id = hunk.id,
        path = hunk.path,
        root = hunk.root,
        state = hunk.state,
        base_range = hunk.base_range,
        current_range = hunk.current_range,
        before = truncate(hunk.before_content),
        after = truncate(hunk.after_content),
        tool_call_ids = hunk.tool_call_ids,
        -- Surfaced as its own field because it is the one property that
        -- changes what the reviewer is allowed to do.
        external = (hunk.tool_call_ids == nil) or (#hunk.tool_call_ids == 0),
        -- A change the reviewer already rejected and the agent applied again.
        -- It adds no decision — the state is still "unreviewed" — but a
        -- reviewing agent that cannot see the repeat will keep accepting it.
        reapplied = hunk.reapplied == true,
    }
end

--- List the composed diff for a session.
function M.list_hunks(args)
    if not args.session_id then
        return { error = "session_id is required" }
    end

    local hunks, err = cru.sessions.review_list_hunks(args.session_id)
    if err then
        return { error = err }
    end

    local out = {}
    local unreviewed = 0
    for _, hunk in ipairs(hunks) do
        local row = project(hunk)
        if row.state == "unreviewed" and not row.external then
            unreviewed = unreviewed + 1
        end
        out[#out + 1] = row
    end
    return { hunks = out, count = #out, unreviewed = unreviewed }
end

--- Accept or reject one hunk.
function M.set_state(args)
    if not args.session_id then
        return { error = "session_id is required" }
    end
    if not args.hunk_id then
        return { error = "hunk_id is required" }
    end
    local state = args.state
    if state ~= "accepted" and state ~= "rejected" and state ~= "unreviewed" then
        return { error = "state must be 'accepted', 'rejected' or 'unreviewed'" }
    end

    local ok, err = cru.sessions.review_set_state(args.session_id, args.hunk_id, state)
    if not ok then
        return { error = err }
    end
    return { hunk_id = args.hunk_id, state = state }
end

--- Comment on a line range.
function M.comment(args)
    if not args.session_id then
        return { error = "session_id is required" }
    end
    if not args.path then
        return { error = "path is required" }
    end
    if not args.body then
        return { error = "body is required" }
    end
    if not args.line_start then
        return { error = "line_start is required" }
    end

    local comment, err = cru.sessions.review_comment(args.session_id, {
        path = args.path,
        line_start = args.line_start,
        line_end = args.line_end,
        body = args.body,
        author = "agent",
    })
    if err then
        return { error = err }
    end
    return { comment_id = comment.id, path = comment.path, resolved = false }
end

--- Mark a comment answered.
function M.resolve_comment(args)
    if not args.session_id then
        return { error = "session_id is required" }
    end
    if not args.comment_id then
        return { error = "comment_id is required" }
    end

    local ok, err = cru.sessions.review_resolve_comment(args.session_id, args.comment_id)
    if not ok then
        return { error = err }
    end
    return { comment_id = args.comment_id, resolved = true }
end

return {
    name = "review",
    version = "0.1.0",
    description = "Review the attributed diff of a session — including one you delegated to",
    capabilities = { "agent" },

    tools = {
        review_list_hunks = {
            desc = "List a session's composed diff: every change between the session's base "
                .. "tree and the worktree now, with which tool calls produced it and whether "
                .. "it has been reviewed. Use the child_session_id from delegate_session to "
                .. "review work you delegated.",
            params = {
                { name = "session_id", type = "string", desc = "Session whose diff to list" },
            },
            fn = M.list_hunks,
        },
        review_set_state = {
            desc = "Accept or reject one hunk. Rejecting reverts it on disk immediately and "
                .. "tells that session's agent, so it does not re-apply the same edit. "
                .. "External hunks (external = true) cannot be rejected.",
            params = {
                { name = "session_id", type = "string", desc = "Session being reviewed" },
                { name = "hunk_id", type = "string", desc = "Hunk id from review_list_hunks" },
                {
                    name = "state",
                    type = "string",
                    desc = "accepted | rejected | unreviewed",
                },
            },
            fn = M.set_state,
        },
        review_comment = {
            desc = "Comment on a line range — changed or unchanged. Say what should change; "
                .. "use review_set_state to undo something instead.",
            params = {
                { name = "session_id", type = "string", desc = "Session being reviewed" },
                {
                    name = "path",
                    type = "string",
                    desc = "File path, absolute or relative to the session's root",
                },
                { name = "line_start", type = "number", desc = "First line, 1-based" },
                {
                    name = "line_end",
                    type = "number",
                    desc = "One past the last line; defaults to a single line",
                    optional = true,
                },
                { name = "body", type = "string", desc = "Comment text" },
            },
            fn = M.comment,
        },
        review_resolve_comment = {
            desc = "Mark a review comment answered.",
            params = {
                { name = "session_id", type = "string", desc = "Session being reviewed" },
                { name = "comment_id", type = "string", desc = "Comment id" },
            },
            fn = M.resolve_comment,
        },
    },
}
