--- Tests for the review plugin.
---
--- `crucible-daemon/tests/review_plugin.rs` asserts the SHAPE of this tool
--- surface — that four tools exist and each takes `session_id`. Nothing
--- exercised what the tools do with a hunk once they have one, which is where
--- all the logic is: the truncation, the `external` derivation, the state
--- validation, and every error path.
---
--- `cru.sessions.review_*` is registered by the daemon, not by the bare
--- executor the runner builds, so each case installs its own stub and asserts
--- on what the plugin passed down as well as what it handed back.

-- Required by DIRECTORY NAME, never by `init`: the runner's package.path
-- mirrors the daemon loader's, which exposes a plugin as `<parent>/?/init.lua`.
local plugin = require("review")

local SESSION = "chat-2026-08-13T1200-abc123"

--- Records every call so a test can assert what crossed the boundary.
local calls

local function stub(overrides)
    calls = {}
    cru.sessions = cru.sessions or {}
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
    cru.sessions.review_list_hunks = record("review_list_hunks")
    cru.sessions.review_set_state = record("review_set_state")
    cru.sessions.review_comment = record("review_comment")
    cru.sessions.review_resolve_comment = record("review_resolve_comment")
end

local function hunk(over)
    local h = {
        id = "h1",
        path = "src/main.rs",
        root = "/repo",
        state = "unreviewed",
        base_range = { 1, 4 },
        current_range = { 1, 6 },
        before_content = "old",
        after_content = "new",
        tool_call_ids = { "tc-1" },
    }
    for k, v in pairs(over or {}) do
        h[k] = v
    end
    return h
end

describe("review", function()
    before_each(function()
        test_mocks.setup()
        stub()
    end)

    after_each(function()
        test_mocks.reset()
    end)

    describe("review_list_hunks", function()
        it("requires a session_id", function()
            local result = plugin.tools.review_list_hunks.fn({})
            assert.equal(result.error, "session_id is required")
            assert.equal(#calls, 0)
        end)

        it("passes the session id straight through", function()
            stub({ review_list_hunks = function() return {} end })
            plugin.tools.review_list_hunks.fn({ session_id = SESSION })
            assert.equal(calls[1].name, "review_list_hunks")
            assert.equal(calls[1].args[1], SESSION)
        end)

        it("surfaces an error from the daemon", function()
            stub({ review_list_hunks = function() return nil, "no such session" end })
            local result = plugin.tools.review_list_hunks.fn({ session_id = SESSION })
            assert.equal(result.error, "no such session")
        end)

        it("projects the fields a reviewer needs", function()
            stub({ review_list_hunks = function() return { hunk() } end })
            local row = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks[1]
            assert.equal(row.id, "h1")
            assert.equal(row.path, "src/main.rs")
            assert.equal(row.root, "/repo")
            assert.equal(row.state, "unreviewed")
            assert.equal(row.before, "old")
            assert.equal(row.after, "new")
            assert.deep_equal(row.tool_call_ids, { "tc-1" })
        end)

        it("truncates a hunk body rather than blowing the context window", function()
            local huge = string.rep("x", 5000)
            stub({
                review_list_hunks = function()
                    return { hunk({ before_content = huge, after_content = huge }) }
                end,
            })
            local row = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks[1]
            assert.truthy(#row.before < 5000)
            assert.truthy(row.before:find("truncated", 1, true))
            assert.truthy(row.after:find("truncated", 1, true))
        end)

        it("leaves a body at the limit untouched", function()
            local exact = string.rep("x", 2000)
            stub({ review_list_hunks = function() return { hunk({ before_content = exact }) } end })
            local row = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks[1]
            assert.equal(row.before, exact)
        end)

        it("renders a nil body as an empty string", function()
            -- Built literally, not through `hunk`: a Lua table cannot hold a
            -- nil, so an override of `{before_content = nil}` is no override.
            stub({
                review_list_hunks = function()
                    return { { id = "h1", path = "a.rs", state = "unreviewed", tool_call_ids = {} } }
                end,
            })
            local row = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks[1]
            assert.equal(row.before, "")
            assert.equal(row.after, "")
        end)

        it("marks a hunk with no tool calls as external", function()
            stub({ review_list_hunks = function() return { hunk({ tool_call_ids = {} }) } end })
            local row = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks[1]
            assert.equal(row.external, true)
        end)

        it("marks a hunk with nil tool calls as external", function()
            -- Likewise literal: the absence of the key is the case under test.
            stub({
                review_list_hunks = function()
                    return { { id = "h1", path = "a.rs", state = "unreviewed" } }
                end,
            })
            local row = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks[1]
            assert.equal(row.external, true)
        end)

        it("does not mark an agent-authored hunk external", function()
            stub({ review_list_hunks = function() return { hunk() } end })
            local row = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks[1]
            assert.equal(row.external, false)
        end)

        it("surfaces reapplied, and defaults it to false", function()
            stub({
                review_list_hunks = function()
                    return { hunk({ id = "a", reapplied = true }), hunk({ id = "b" }) }
                end,
            })
            local rows = plugin.tools.review_list_hunks.fn({ session_id = SESSION }).hunks
            assert.equal(rows[1].reapplied, true)
            assert.equal(rows[2].reapplied, false)
        end)

        it("counts unreviewed hunks, excluding external ones", function()
            stub({
                review_list_hunks = function()
                    return {
                        hunk({ id = "a", state = "unreviewed" }),
                        hunk({ id = "b", state = "accepted" }),
                        -- External and unreviewed: cannot be rejected, so it is
                        -- not work the reviewer can action.
                        hunk({ id = "c", state = "unreviewed", tool_call_ids = {} }),
                    }
                end,
            })
            local result = plugin.tools.review_list_hunks.fn({ session_id = SESSION })
            assert.equal(result.count, 3)
            assert.equal(result.unreviewed, 1)
        end)

        it("reports an empty diff as zero, not as an error", function()
            stub({ review_list_hunks = function() return {} end })
            local result = plugin.tools.review_list_hunks.fn({ session_id = SESSION })
            assert.equal(result.count, 0)
            assert.equal(result.unreviewed, 0)
            assert.deep_equal(result.hunks, {})
        end)
    end)

    describe("review_set_state", function()
        it("requires a session_id", function()
            local result = plugin.tools.review_set_state.fn({ hunk_id = "h1", state = "accepted" })
            assert.equal(result.error, "session_id is required")
        end)

        it("requires a hunk_id", function()
            local result = plugin.tools.review_set_state.fn({ session_id = SESSION, state = "accepted" })
            assert.equal(result.error, "hunk_id is required")
        end)

        it("accepts each of the three valid states", function()
            for _, state in ipairs({ "accepted", "rejected", "unreviewed" }) do
                stub({ review_set_state = function() return true end })
                local result = plugin.tools.review_set_state.fn({
                    session_id = SESSION,
                    hunk_id = "h1",
                    state = state,
                })
                assert.equal(result.state, state)
            end
        end)

        it("rejects any other state before calling the daemon", function()
            for _, state in ipairs({ "approved", "ACCEPTED", "", "maybe" }) do
                stub({ review_set_state = function() return true end })
                local result = plugin.tools.review_set_state.fn({
                    session_id = SESSION,
                    hunk_id = "h1",
                    state = state,
                })
                assert.truthy(result.error)
                assert.equal(#calls, 0)
            end
        end)

        it("rejects a missing state", function()
            local result = plugin.tools.review_set_state.fn({ session_id = SESSION, hunk_id = "h1" })
            assert.truthy(result.error)
            assert.equal(#calls, 0)
        end)

        it("passes session, hunk and state down in order", function()
            stub({ review_set_state = function() return true end })
            plugin.tools.review_set_state.fn({
                session_id = SESSION,
                hunk_id = "h9",
                state = "rejected",
            })
            assert.equal(calls[1].args[1], SESSION)
            assert.equal(calls[1].args[2], "h9")
            assert.equal(calls[1].args[3], "rejected")
        end)

        it("surfaces a refusal from the daemon", function()
            stub({
                review_set_state = function()
                    return false, "external hunks cannot be rejected"
                end,
            })
            local result = plugin.tools.review_set_state.fn({
                session_id = SESSION,
                hunk_id = "h1",
                state = "rejected",
            })
            assert.equal(result.error, "external hunks cannot be rejected")
        end)
    end)

    describe("review_comment", function()
        local function args(over)
            local a = {
                session_id = SESSION,
                path = "src/main.rs",
                line_start = 10,
                body = "this leaks",
            }
            for k, v in pairs(over or {}) do
                a[k] = v
            end
            return a
        end

        it("requires session_id, path, body and line_start", function()
            for _, missing in ipairs({ "session_id", "path", "body", "line_start" }) do
                local a = args()
                a[missing] = nil
                local result = plugin.tools.review_comment.fn(a)
                assert.truthy(result.error, missing .. " should be required")
                assert.truthy(result.error:find(missing, 1, true))
            end
        end)

        it("returns the new comment id, unresolved", function()
            stub({ review_comment = function() return { id = "c7", path = "src/main.rs" } end })
            local result = plugin.tools.review_comment.fn(args())
            assert.equal(result.comment_id, "c7")
            assert.equal(result.path, "src/main.rs")
            assert.equal(result.resolved, false)
        end)

        it("attributes the comment to the agent", function()
            stub({ review_comment = function() return { id = "c7" } end })
            plugin.tools.review_comment.fn(args())
            assert.equal(calls[1].args[2].author, "agent")
        end)

        it("passes the line range through, including an absent end", function()
            stub({ review_comment = function() return { id = "c7" } end })
            plugin.tools.review_comment.fn(args({ line_start = 3, line_end = 9 }))
            assert.equal(calls[1].args[2].line_start, 3)
            assert.equal(calls[1].args[2].line_end, 9)

            stub({ review_comment = function() return { id = "c8" } end })
            plugin.tools.review_comment.fn(args({ line_start = 3 }))
            assert.equal(calls[1].args[2].line_end, nil)
        end)

        it("surfaces an error from the daemon", function()
            stub({ review_comment = function() return nil, "no such path" end })
            assert.equal(plugin.tools.review_comment.fn(args()).error, "no such path")
        end)
    end)

    describe("review_resolve_comment", function()
        it("requires a session_id", function()
            local result = plugin.tools.review_resolve_comment.fn({ comment_id = "c1" })
            assert.equal(result.error, "session_id is required")
        end)

        it("requires a comment_id", function()
            local result = plugin.tools.review_resolve_comment.fn({ session_id = SESSION })
            assert.equal(result.error, "comment_id is required")
        end)

        it("reports the comment resolved", function()
            stub({ review_resolve_comment = function() return true end })
            local result = plugin.tools.review_resolve_comment.fn({
                session_id = SESSION,
                comment_id = "c1",
            })
            assert.equal(result.comment_id, "c1")
            assert.equal(result.resolved, true)
        end)

        it("surfaces a refusal from the daemon", function()
            stub({ review_resolve_comment = function() return false, "already resolved" end })
            local result = plugin.tools.review_resolve_comment.fn({
                session_id = SESSION,
                comment_id = "c1",
            })
            assert.equal(result.error, "already resolved")
        end)
    end)

    describe("plugin metadata", function()
        it("exports the correct name", function()
            assert.equal(plugin.name, "review")
        end)

        it("exports every review operation as a tool", function()
            assert.truthy(plugin.tools.review_list_hunks)
            assert.truthy(plugin.tools.review_set_state)
            assert.truthy(plugin.tools.review_comment)
            assert.truthy(plugin.tools.review_resolve_comment)
        end)

        it("takes the reviewed session explicitly on every tool", function()
            for name, tool in pairs(plugin.tools) do
                local has_session = false
                for _, param in ipairs(tool.params) do
                    if param.name == "session_id" then
                        has_session = true
                    end
                end
                assert.truthy(has_session, name .. " must take session_id")
            end
        end)
    end)
end)
