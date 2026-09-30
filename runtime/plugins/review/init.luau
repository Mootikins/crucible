--!strict
--- review — the diff of a delegated session, as tools an agent can call.
---
--- The daemon owns the diffsets, the comments and the proposals. This plugin
--- is the tool surface over them. It and the clients use the same `diff.*`
--- and `proposal.*` operations, so a comment from an agent and a comment
--- from a person are stored the same way.
---
--- The reason for this plugin is delegation. A delegating agent starts a
--- child session, and the child changes files. The delegator reads the
--- change and decides on it. So every tool takes `session_id`: the session
--- under review is usually not the caller's own. The id of a child comes
--- from `delegate_session` (`child_session_id`).
---
--- A tool reads one of three diffsets:
---
--- - the session record of the child (the default): each file that differs
---   between the text before the first tool call of the child and the disk;
--- - a branch (`branch_root`, and `base` or the default branch), when the
---   child works in a worktree branch;
--- - one proposal (`proposal_id`), when the child writes notes in `propose`
---   mode. Accept or reject it with `review_accept_proposal` and
---   `review_reject_proposal`.

local M = {}

--- The limit of a file text in a tool result. A reviewer that must read a
--- longer text opens the file, so a large file does not fill the context.
local MAX_CONTENT = 2000

local function truncate(s: string?): string?
    if s == nil then
        return nil
    end
    if #s <= MAX_CONTENT then
        return s
    end
    return s:sub(1, MAX_CONTENT) .. "\n… (truncated; open the file to read the rest)"
end

type Source = { kind: string, root: string?, base: string?, head: string?, session: string?, id: string? }

--- The diffset that the arguments name, or an error.
local function source_of(args: { [string]: any }): (Source?, string?)
    if not args.session_id then
        return nil, "session_id is required"
    end
    if args.branch_root and args.proposal_id then
        return nil, "give branch_root or proposal_id, not both"
    end
    if args.proposal_id then
        return { kind = "proposal", id = args.proposal_id }, nil
    end
    if args.branch_root then
        -- An empty base selects the default branch of the repository.
        return { kind = "branch", root = args.branch_root, base = args.base or "" }, nil
    end
    if args.base then
        return nil, "base needs branch_root"
    end
    return { kind = "session_record", session = args.session_id }, nil
end

local function project_file(entry: { [string]: any }): { [string]: any }
    return {
        root = entry.root,
        path = entry.path,
        status = entry.status and entry.status.kind,
        from = entry.status and entry.status.from,
        added = entry.added,
        removed = entry.removed,
        binary = entry.binary,
        too_large = entry.too_large,
    }
end

--- List the files of the diffset, with the counts of added and removed lines.
function M.changes(args: { [string]: any }): ToolResult
    local source, bad = source_of(args)
    if not source then
        return { error = bad }
    end
    local diffset, err = cru.diff.get({ source = source })
    if err then
        return { error = err }
    end
    local out: { { [string]: any } } = {}
    for _, entry in ipairs(diffset.files or {}) do
        out[#out + 1] = project_file(entry)
    end
    return { diffset = diffset.id, files = out, count = #out }
end

--- Read the two texts of one file of the diffset.
function M.file(args: { [string]: any }): ToolResult
    local source, bad = source_of(args)
    if not source then
        return { error = bad }
    end
    if not args.path then
        return { error = "path is required" }
    end
    local texts, err = cru.diff.file({
        source = source,
        path = args.path,
        root = args.root,
        from = args.from,
    })
    if err then
        return { error = err }
    end
    return {
        path = args.path,
        -- An absent text is an added file (no base) or a deleted file (no
        -- current text), or a binary or too-large file.
        base_text = truncate(texts.base_text),
        current_text = truncate(texts.current_text),
    }
end

--- Comment on a line range of one file of the diffset.
function M.comment(args: { [string]: any }): ToolResult
    local source, bad = source_of(args)
    if not source then
        return { error = bad }
    end
    for _, key in ipairs({ "path", "line_start", "body" }) do
        if args[key] == nil then
            return { error = key .. " is required" }
        end
    end
    local reply, err = cru.diff.comment({
        source = source,
        root = args.root,
        path = args.path,
        side = args.side,
        line_start = args.line_start,
        line_end = args.line_end,
        body = args.body,
        author = "agent",
    })
    if err then
        return { error = err }
    end
    return {
        comment_id = reply.comment.id,
        diffset = reply.diffset,
        path = reply.comment.path,
        resolved = false,
    }
end

--- List the open comments of the diffset.
function M.comments(args: { [string]: any }): ToolResult
    local source, bad = source_of(args)
    if not source then
        return { error = bad }
    end
    local reply, err = cru.diff.comments({ source = source })
    if err then
        return { error = err }
    end
    local out: { { [string]: any } } = {}
    for _, listed in ipairs(reply.comments or {}) do
        local c = listed.comment
        if not c.resolved then
            out[#out + 1] = {
                comment_id = c.id,
                path = c.path,
                line_start = c.line_range.start,
                line_end = c.line_range["end"],
                side = c.side,
                body = c.body,
                author = c.author,
                outdated = listed.outdated,
            }
        end
    end
    return { diffset = reply.diffset, comments = out, count = #out }
end

--- Mark a comment of the diffset resolved.
function M.resolve_comment(args: { [string]: any }): ToolResult
    local source, bad = source_of(args)
    if not source then
        return { error = bad }
    end
    if not args.comment_id then
        return { error = "comment_id is required" }
    end
    local _, err = cru.diff.resolve_comment({ source = source, comment_id = args.comment_id })
    if err then
        return { error = err }
    end
    return { comment_id = args.comment_id, resolved = true }
end

local function project_proposal(p: { [string]: any }): { [string]: any }
    local paths: { string } = {}
    for _, w in ipairs(p.writes or {}) do
        paths[#paths + 1] = w.path
    end
    return {
        proposal_id = p.id,
        title = p.title,
        rationale = p.rationale,
        state = p.state and p.state.kind,
        paths = paths,
    }
end

--- List the proposals of the session that wait for a decision.
function M.proposals(args: { [string]: any }): ToolResult
    if not args.session_id then
        return { error = "session_id is required" }
    end
    local rows, err = cru.proposals.list({ session = args.session_id })
    if err then
        return { error = err }
    end
    local out: { { [string]: any } } = {}
    for _, p in ipairs(rows or {}) do
        out[#out + 1] = project_proposal(p)
    end
    return { proposals = out, count = #out }
end

--- The decision params, after a check that the proposal is an open
--- proposal of the session. A reviewer decides only on the work of the
--- session that it reviews.
local function decision_params(args: { [string]: any }): ({ [string]: any }?, string?)
    if not args.session_id then
        return nil, "session_id is required"
    end
    if not args.proposal_id then
        return nil, "proposal_id is required"
    end
    local rows, err = cru.proposals.list({ session = args.session_id })
    if err then
        return nil, err
    end
    local found = false
    for _, p in ipairs(rows or {}) do
        if p.id == args.proposal_id then
            found = true
        end
    end
    if not found then
        return nil,
            "proposal " .. tostring(args.proposal_id) .. " is not an open proposal of session " .. tostring(
                args.session_id
            )
    end
    local params: { [string]: any } = { id = args.proposal_id }
    -- An empty list is a map on the wire, and the daemon reads a list.
    -- No paths means every file.
    if args.paths and #args.paths > 0 then
        params.paths = args.paths
    end
    return params, nil
end

--- Accept a proposal of the session: the daemon writes its files.
function M.accept_proposal(args: { [string]: any }): ToolResult
    local params, bad = decision_params(args)
    if not params then
        return { error = bad }
    end
    local proposal, err = cru.proposals.accept({ id = params.id, paths = params.paths })
    if err then
        return { error = err }
    end
    return project_proposal(proposal)
end

--- Reject a proposal of the session: the daemon writes nothing.
function M.reject_proposal(args: { [string]: any }): ToolResult
    local params, bad = decision_params(args)
    if not params then
        return { error = bad }
    end
    local proposal, err =
        cru.proposals.reject({ id = params.id, paths = params.paths, reason = args.reason })
    if err then
        return { error = err }
    end
    return project_proposal(proposal)
end

local SESSION = { name = "session_id", type = "string", desc = "The session under review" }
local BRANCH_ROOT = {
    name = "branch_root",
    type = "string",
    desc = "The worktree of the child, to read its branch instead of its session record",
    optional = true,
}
local BASE = {
    name = "base",
    type = "string",
    desc = "The base branch of branch_root; absent means the default branch",
    optional = true,
}
local PROPOSAL = {
    name = "proposal_id",
    type = "string",
    desc = "A proposal of the session, to read it instead of the session record",
    optional = true,
}
local ROOT = {
    name = "root",
    type = "string",
    desc = "The root of the file, as review_changes lists it",
    optional = true,
}

return {
    tools = {
        review_changes = {
            desc = "List the files that a session changed, with added and removed line counts. "
                .. "Use the child_session_id from delegate_session to review delegated work.",
            params = { SESSION, BRANCH_ROOT, BASE, PROPOSAL },
            fn = M.changes,
        },
        review_file = {
            desc = "Read the base text and the current text of one changed file.",
            params = {
                SESSION,
                { name = "path", type = "string", desc = "The path, relative to its root" },
                ROOT,
                {
                    name = "from",
                    type = "string",
                    desc = "The old path of a renamed file",
                    optional = true,
                },
                BRANCH_ROOT,
                BASE,
                PROPOSAL,
            },
            fn = M.file,
        },
        review_comment = {
            desc = "Comment on a line range of a changed file. Say what should change.",
            params = {
                SESSION,
                { name = "path", type = "string", desc = "The path, relative to its root" },
                ROOT,
                { name = "line_start", type = "number", desc = "First line, 1-based" },
                {
                    name = "line_end",
                    type = "number",
                    desc = "One past the last line; absent means one line",
                    optional = true,
                },
                {
                    name = "side",
                    type = "string",
                    desc = "current (the default) or base: the text that the lines count on",
                    optional = true,
                },
                { name = "body", type = "string", desc = "Comment text" },
                BRANCH_ROOT,
                BASE,
                PROPOSAL,
            },
            fn = M.comment,
        },
        review_comments = {
            desc = "List the open comments on the changes of a session.",
            params = { SESSION, BRANCH_ROOT, BASE, PROPOSAL },
            fn = M.comments,
        },
        review_resolve_comment = {
            desc = "Mark a review comment answered.",
            params = {
                SESSION,
                { name = "comment_id", type = "string", desc = "Comment id" },
                BRANCH_ROOT,
                BASE,
                PROPOSAL,
            },
            fn = M.resolve_comment,
        },
        review_proposals = {
            desc = "List the proposals of a session that wait for a decision.",
            params = { SESSION },
            fn = M.proposals,
        },
        review_accept_proposal = {
            desc = "Accept a proposal of the session. The daemon writes its files.",
            params = {
                SESSION,
                { name = "proposal_id", type = "string", desc = "The proposal to accept" },
                {
                    name = "paths",
                    type = "string[]",
                    desc = "Accept only these files; absent means every file",
                    optional = true,
                },
            },
            fn = M.accept_proposal,
        },
        review_reject_proposal = {
            desc = "Reject a proposal of the session. The daemon writes nothing.",
            params = {
                SESSION,
                { name = "proposal_id", type = "string", desc = "The proposal to reject" },
                {
                    name = "reason",
                    type = "string",
                    desc = "Why; later passes read it",
                    optional = true,
                },
                {
                    name = "paths",
                    type = "string[]",
                    desc = "Reject only these files; absent means every file",
                    optional = true,
                },
            },
            fn = M.reject_proposal,
        },
    },
}
