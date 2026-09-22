--!strict
--- Tests for the review plugin.
---
--- `crucible-daemon/tests/review_plugin.rs` asserts the SHAPE of this tool
--- surface: the tools exist, and each takes `session_id`. These tests assert
--- what the tools do: the choice of the diffset, the truncation, the check
--- that a decided proposal belongs to the session, and every error path.
---
--- `cru.diff` and `cru.proposals` are registered by the daemon, not by the
--- bare executor that the runner builds, so each case installs its own stub
--- and asserts on what the plugin passed down as well as what it handed back.

-- Required by DIRECTORY NAME, never by `init`: the runner's package.path
-- mirrors the daemon loader's, which exposes a plugin as `<parent>/?/init.lua`.
local plugin = require("review")

local SESSION = "chat-2026-08-13T1200-abc123"

--- Records every call so a test can assert what crossed the boundary.
local calls: { { name: string, args: { any } } }

local function stub(overrides: { [string]: any }?)
    calls = {}
    local function record(name)
        return function(...)
            table.insert(calls, { name = name, args = { ... } })
            local handler = overrides and overrides[name]
            if handler then
                return handler(...)
            end
            return nil, nil
        end
    end
    local diff: any = {}
    for _, name in ipairs({ "get", "file", "comment", "resolve_comment", "comments" }) do
        diff[name] = record("diff." .. name)
    end
    cru.diff = diff
    local proposals: any = cru.proposals or {}
    for _, name in ipairs({ "list", "accept", "reject" }) do
        proposals[name] = record("proposals." .. name)
    end
    cru.proposals = proposals
end

local function file_entry(over: { [string]: any }?)
    local e = {
        root = "/repo",
        path = "src/main.rs",
        status = { kind = "modified" },
        added = 2,
        removed = 1,
        binary = false,
        too_large = false,
    }
    for k, v in pairs(over or {}) do
        e[k] = v
    end
    return e
end

local function open_proposal(id: string)
    return {
        id = id,
        title = "Change a.md",
        state = { kind = "open" },
        writes = { { path = "a.md" }, { path = "b.md" } },
    }
end

describe("review", function()
    before_each(function()
        test_mocks.setup()
        stub()
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("review_changes", function()
        it("requires a session_id", function()
            local result = plugin.tools.review_changes.fn({})
            expect.equal(result.error, "session_id is required")
            expect.equal(#calls, 0)
        end)

        it("reads the child session record", function()
            stub({
                ["diff.get"] = function()
                    return { id = "session-" .. SESSION, files = { file_entry() } }
                end,
            })
            local result = plugin.tools.review_changes.fn({ session_id = SESSION })
            expect.equal(calls[1].name, "diff.get")
            expect.deep_equal(
                calls[1].args[1],
                { source = { kind = "session_record", session = SESSION } }
            )
            expect.equal(result.diffset, "session-" .. SESSION)
            expect.equal(result.count, 1)
            local row = result.files[1]
            expect.equal(row.path, "src/main.rs")
            expect.equal(row.root, "/repo")
            expect.equal(row.status, "modified")
            expect.equal(row.added, 2)
            expect.equal(row.removed, 1)
        end)

        it("reads the branch of a child worktree", function()
            stub({ ["diff.get"] = function() return { id = "branch-x", files = {} } end })
            plugin.tools.review_changes.fn({ session_id = SESSION, branch_root = "/wt/child" })
            expect.deep_equal(
                calls[1].args[1],
                { source = { kind = "branch", root = "/wt/child", base = "" } }
            )

            stub({ ["diff.get"] = function() return { id = "branch-y", files = {} } end })
            plugin.tools.review_changes.fn({
                session_id = SESSION,
                branch_root = "/wt/child",
                base = "main",
            })
            expect.equal(calls[1].args[1].source.base, "main")
        end)

        it("reads one proposal", function()
            stub({ ["diff.get"] = function() return { id = "proposal-p1", files = {} } end })
            plugin.tools.review_changes.fn({ session_id = SESSION, proposal_id = "p1" })
            expect.deep_equal(calls[1].args[1], { source = { kind = "proposal", id = "p1" } })
        end)

        it("refuses two diffsets at once, and a base with no branch", function()
            local both = plugin.tools.review_changes.fn({
                session_id = SESSION,
                branch_root = "/wt",
                proposal_id = "p1",
            })
            expect.truthy(both.error:find("not both", 1, true))
            local base = plugin.tools.review_changes.fn({ session_id = SESSION, base = "main" })
            expect.equal(base.error, "base needs branch_root")
            expect.equal(#calls, 0)
        end)

        it("names the old path of a renamed file", function()
            stub({
                ["diff.get"] = function()
                    return {
                        id = "branch-x",
                        files = { file_entry({ status = { kind = "renamed", from = "old.rs" } }) },
                    }
                end,
            })
            local row = plugin.tools.review_changes.fn({ session_id = SESSION }).files[1]
            expect.equal(row.status, "renamed")
            expect.equal(row.from, "old.rs")
        end)

        it("surfaces an error from the daemon", function()
            stub({ ["diff.get"] = function() return nil, "root is not admitted" end })
            local result = plugin.tools.review_changes.fn({ session_id = SESSION })
            expect.equal(result.error, "root is not admitted")
        end)

        it("reports an empty record as zero, not as an error", function()
            stub({ ["diff.get"] = function() return { id = "s", files = {} } end })
            local result = plugin.tools.review_changes.fn({ session_id = SESSION })
            expect.equal(result.count, 0)
            expect.deep_equal(result.files, {})
        end)
    end)

    describe("review_file", function()
        it("requires a path", function()
            local result = plugin.tools.review_file.fn({ session_id = SESSION })
            expect.equal(result.error, "path is required")
        end)

        it("passes the root and the old path through", function()
            stub({ ["diff.file"] = function() return { base_text = "a", current_text = "b" } end })
            local result = plugin.tools.review_file.fn({
                session_id = SESSION,
                path = "new.rs",
                root = "/repo",
                from = "old.rs",
            })
            local sent = calls[1].args[1]
            expect.equal(sent.path, "new.rs")
            expect.equal(sent.root, "/repo")
            expect.equal(sent.from, "old.rs")
            expect.equal(sent.source.kind, "session_record")
            expect.equal(result.base_text, "a")
            expect.equal(result.current_text, "b")
        end)

        it("truncates a long text rather than filling the context window", function()
            local huge = string.rep("x", 5000)
            stub({ ["diff.file"] = function() return { base_text = huge, current_text = huge } end })
            local result = plugin.tools.review_file.fn({ session_id = SESSION, path = "a.rs" })
            expect.truthy(#result.base_text < 5000)
            expect.truthy(result.base_text:find("truncated", 1, true))
            expect.truthy(result.current_text:find("truncated", 1, true))
        end)

        it("leaves a text at the limit untouched", function()
            local exact = string.rep("x", 2000)
            stub({ ["diff.file"] = function() return { base_text = exact } end })
            local result = plugin.tools.review_file.fn({ session_id = SESSION, path = "a.rs" })
            expect.equal(result.base_text, exact)
        end)

        it("keeps the absent base of an added file absent", function()
            stub({ ["diff.file"] = function() return { current_text = "new" } end })
            local result = plugin.tools.review_file.fn({ session_id = SESSION, path = "a.rs" })
            expect.equal(result.base_text, nil)
            expect.equal(result.current_text, "new")
        end)
    end)

    describe("review_comment", function()
        local function args(over: { [string]: any }?)
            local a = {
                session_id = SESSION,
                path = "src/main.rs",
                root = "/repo",
                line_start = 10,
                body = "this leaks",
            }
            for k, v in pairs(over or {}) do
                a[k] = v
            end
            return a
        end

        local function stored()
            return { diffset = "session-x", comment = { id = "c7", path = "src/main.rs" } }
        end

        it("requires session_id, path, body and line_start", function()
            for _, missing in ipairs({ "session_id", "path", "body", "line_start" }) do
                local a = args()
                a[missing] = nil
                local result = plugin.tools.review_comment.fn(a)
                expect.truthy(result.error, missing .. " should be required")
                expect.truthy(result.error:find(missing, 1, true))
            end
            expect.equal(#calls, 0)
        end)

        it("comments through diff.comment on the session record", function()
            stub({ ["diff.comment"] = stored })
            local result = plugin.tools.review_comment.fn(args())
            expect.equal(calls[1].name, "diff.comment")
            local sent = calls[1].args[1]
            expect.deep_equal(sent.source, { kind = "session_record", session = SESSION })
            expect.equal(sent.root, "/repo")
            expect.equal(sent.path, "src/main.rs")
            expect.equal(sent.author, "agent")
            expect.equal(result.comment_id, "c7")
            expect.equal(result.diffset, "session-x")
            expect.equal(result.resolved, false)
        end)

        it("passes the line range and the side through, including an absent end", function()
            stub({ ["diff.comment"] = stored })
            plugin.tools.review_comment.fn(args({ line_start = 3, line_end = 9, side = "base" }))
            expect.equal(calls[1].args[1].line_start, 3)
            expect.equal(calls[1].args[1].line_end, 9)
            expect.equal(calls[1].args[1].side, "base")

            stub({ ["diff.comment"] = stored })
            plugin.tools.review_comment.fn(args({ line_start = 3 }))
            expect.equal(calls[1].args[1].line_end, nil)
            expect.equal(calls[1].args[1].side, nil)
        end)

        it("comments on a branch", function()
            stub({ ["diff.comment"] = stored })
            plugin.tools.review_comment.fn(args({ branch_root = "/wt/child" }))
            expect.equal(calls[1].args[1].source.kind, "branch")
            expect.equal(calls[1].args[1].source.root, "/wt/child")
        end)

        it("surfaces an error from the daemon", function()
            stub({ ["diff.comment"] = function() return nil, "no such path" end })
            expect.equal(plugin.tools.review_comment.fn(args()).error, "no such path")
        end)
    end)

    describe("review_comments", function()
        it("lists only the open comments", function()
            local function listed(id: string, resolved: boolean)
                return {
                    outdated = false,
                    comment = {
                        id = id,
                        path = "a.rs",
                        line_range = { start = 3, ["end"] = 5 },
                        side = "current",
                        body = "why",
                        author = "human",
                        resolved = resolved,
                    },
                }
            end
            stub({
                ["diff.comments"] = function()
                    return { diffset = "s", comments = { listed("c1", false), listed("c2", true) } }
                end,
            })
            local result = plugin.tools.review_comments.fn({ session_id = SESSION })
            expect.equal(result.count, 1)
            expect.equal(result.comments[1].comment_id, "c1")
            expect.equal(result.comments[1].line_start, 3)
            expect.equal(result.comments[1].line_end, 5)
        end)
    end)

    describe("review_resolve_comment", function()
        it("requires a session_id and a comment_id", function()
            expect.equal(
                plugin.tools.review_resolve_comment.fn({ comment_id = "c1" }).error,
                "session_id is required"
            )
            expect.equal(
                plugin.tools.review_resolve_comment.fn({ session_id = SESSION }).error,
                "comment_id is required"
            )
        end)

        it("resolves through diff.resolve_comment on the same diffset", function()
            stub({ ["diff.resolve_comment"] = function() return { resolved = true } end })
            local result = plugin.tools.review_resolve_comment.fn({
                session_id = SESSION,
                comment_id = "c1",
            })
            expect.deep_equal(calls[1].args[1], {
                source = { kind = "session_record", session = SESSION },
                comment_id = "c1",
            })
            expect.equal(result.comment_id, "c1")
            expect.equal(result.resolved, true)
        end)

        it("surfaces a refusal from the daemon", function()
            stub({ ["diff.resolve_comment"] = function() return nil, "unknown comment c1" end })
            local result = plugin.tools.review_resolve_comment.fn({
                session_id = SESSION,
                comment_id = "c1",
            })
            expect.equal(result.error, "unknown comment c1")
        end)
    end)

    describe("proposals", function()
        it("lists the proposals of the child", function()
            stub({ ["proposals.list"] = function() return { open_proposal("p1") } end })
            local result = plugin.tools.review_proposals.fn({ session_id = SESSION })
            expect.deep_equal(calls[1].args[1], { session = SESSION })
            expect.equal(result.count, 1)
            expect.equal(result.proposals[1].proposal_id, "p1")
            expect.equal(result.proposals[1].state, "open")
            expect.deep_equal(result.proposals[1].paths, { "a.md", "b.md" })
        end)

        it("accepts a child proposal", function()
            stub({
                ["proposals.list"] = function() return { open_proposal("p1") } end,
                ["proposals.accept"] = function(params)
                    local p = open_proposal(params.id)
                    p.state = { kind = "accepted" }
                    return p
                end,
            })
            local result = plugin.tools.review_accept_proposal.fn({
                session_id = SESSION,
                proposal_id = "p1",
            })
            expect.equal(calls[1].name, "proposals.list")
            expect.equal(calls[2].name, "proposals.accept")
            expect.deep_equal(calls[2].args[1], { id = "p1" })
            expect.equal(result.state, "accepted")
        end)

        it("accepts only the named files", function()
            stub({
                ["proposals.list"] = function() return { open_proposal("p1") } end,
                ["proposals.accept"] = function() return open_proposal("p2") end,
            })
            plugin.tools.review_accept_proposal.fn({
                session_id = SESSION,
                proposal_id = "p1",
                paths = { "a.md" },
            })
            expect.deep_equal(calls[2].args[1], { id = "p1", paths = { "a.md" } })
        end)

        it("sends no paths for an empty list", function()
            stub({
                ["proposals.list"] = function() return { open_proposal("p1") } end,
                ["proposals.accept"] = function() return open_proposal("p1") end,
            })
            plugin.tools.review_accept_proposal.fn({
                session_id = SESSION,
                proposal_id = "p1",
                paths = {},
            })
            expect.deep_equal(calls[2].args[1], { id = "p1" })
        end)

        it("rejects a child proposal with a reason", function()
            stub({
                ["proposals.list"] = function() return { open_proposal("p1") } end,
                ["proposals.reject"] = function(params)
                    local p = open_proposal(params.id)
                    p.state = { kind = "rejected", reason = params.reason }
                    return p
                end,
            })
            local result = plugin.tools.review_reject_proposal.fn({
                session_id = SESSION,
                proposal_id = "p1",
                reason = "a duplicate",
            })
            expect.deep_equal(calls[2].args[1], { id = "p1", reason = "a duplicate" })
            expect.equal(result.state, "rejected")
        end)

        it("refuses a proposal of another session", function()
            stub({ ["proposals.list"] = function() return { open_proposal("p1") } end })
            local result = plugin.tools.review_accept_proposal.fn({
                session_id = SESSION,
                proposal_id = "p9",
            })
            expect.truthy(result.error:find("not an open proposal", 1, true))
            expect.equal(#calls, 1)
        end)

        it("requires a proposal_id", function()
            local result = plugin.tools.review_reject_proposal.fn({ session_id = SESSION })
            expect.equal(result.error, "proposal_id is required")
            expect.equal(#calls, 0)
        end)

        it("surfaces a refusal from the daemon", function()
            stub({
                ["proposals.list"] = function() return { open_proposal("p1") } end,
                ["proposals.accept"] = function() return nil, "proposal p1 is settled" end,
            })
            local result = plugin.tools.review_accept_proposal.fn({
                session_id = SESSION,
                proposal_id = "p1",
            })
            expect.equal(result.error, "proposal p1 is settled")
        end)
    end)

    describe("plugin metadata", function()
        -- The name and the version are in `spec.luau`, which discovery
        -- reads. The module table carries no metadata, and the Rust gate
        -- `no_shipped_module_table_carries_fragment_metadata` says so.
        it("exports every review operation as a tool", function()
            for _, name in ipairs({
                "review_changes",
                "review_file",
                "review_comment",
                "review_comments",
                "review_resolve_comment",
                "review_proposals",
                "review_accept_proposal",
                "review_reject_proposal",
            }) do
                expect.truthy(plugin.tools[name], name)
            end
            expect.equal(plugin.tools.review_list_hunks, nil)
        end)

        it("takes the reviewed session explicitly on every tool", function()
            for name, tool in pairs(plugin.tools) do
                local has_session = false
                for _, param in ipairs(tool.params) do
                    if param.name == "session_id" then
                        has_session = true
                    end
                end
                expect.truthy(has_session, name .. " must take session_id")
            end
        end)
    end)
end)
