--!strict
--- Tests for session-board.
---
--- Driven through `setup` and read back off the surface, never by reaching for
--- the module's internals: that is the path a daemon actually takes, so it covers
--- `declare` and `set_rows` as well as the row mapping.
---
--- Three things about that mapping are easy to get wrong:
---
--- 1. A failed `cru.session.list()` must not look like "no sessions". The call
---    returns a value AND an error, and a plugin reading only the value shows an
---    empty board when the daemon is unreachable.
--- 2. A row carries a declared mark, never a glyph, so each client chooses its
---    own. A state the table does not know must leave the mark unset.
--- 3. `declare` runs again on every reload, and the rows a user is reading have
---    to survive it.

local plugin = require("session-board")

--- Stage a session list, load the plugin against it, and hand back the surface.
local function board(fixture)
    test_mocks.setup({ sessions = fixture })
    plugin.setup({})
    return test_mocks.surface("session-board", "sessions")
end

describe("session-board", function()
    after_each(function()
        test_mocks.reset()
    end)

    describe("the row mapping", function()
        it("maps a session state to a declared mark, never a glyph", function()
            local surface = board({
                list = {
                    { id = "s1", title = "crucible", state = "Active" },
                    { id = "s2", title = "web-fix", state = "Waiting" },
                    { id = "s3", title = "docs", state = "Ended" },
                },
            })

            expect.equal(3, #surface.rows)
            expect.equal("busy", surface.rows[1].mark)
            expect.equal("blocked", surface.rows[2].mark)
            expect.equal("ok", surface.rows[3].mark)
        end)

        it("leaves the mark unset for a state it does not know", function()
            local surface = board({ list = { { id = "s1", state = "Hibernating" } } })

            expect.equal(1, #surface.rows)
            expect.is_nil(surface.rows[1].mark)
        end)

        it("falls back to the id when a session has no title", function()
            local surface = board({ list = { { id = "sess-abc", state = "Active" } } })

            expect.equal("sess-abc", surface.rows[1].text)
        end)

        it("skips a record with no id, which nothing could act on later", function()
            local surface = board({
                list = { { state = "Active" }, { id = "s2", state = "Active" } },
            })

            expect.equal(1, #surface.rows)
            expect.equal("s2", surface.rows[1].id)
        end)
    end)

    describe("failure and emptiness", function()
        -- An empty list and a failed call must not produce the same board. This
        -- is the whole reason the plugin reads the error half.
        it("reports nothing when the list call fails", function()
            local surface = board({ list_error = "daemon unreachable" })

            expect.equal(0, #surface.rows)
        end)

        it("reports nothing when there are no sessions", function()
            local surface = board({ list = {} })

            expect.equal(0, #surface.rows)
        end)
    end)

    describe("the surface itself", function()
        it("declares a list surface with a title", function()
            local surface = board({ list = {} })

            expect.is_not_nil(surface)
            expect.equal("Sessions", surface.opts.title)
            expect.equal("list", surface.opts.shape)
        end)

        -- Both hooks: a board that only learned about new sessions would keep
        -- showing ended ones.
        it("subscribes to both session lifecycle events", function()
            board({ list = {} })

            expect.equal(1, #test_mocks.get_calls("on", "session:created"))
            expect.equal(1, #test_mocks.get_calls("on", "session:ended"))
        end)

        it("redraws when a session event fires", function()
            local surface = board({ list = {} })
            expect.equal(0, #surface.rows)

            -- A session appeared since the board was drawn. Patch ONLY the list:
            -- a second `test_mocks.setup` would rebuild `cru.surface` and discard
            -- the surface the plugin just declared, so the test would assert
            -- against a registry the plugin never wrote to.
            cru.session = mock({
                list = function()
                    return { { id = "s9", state = "Active" } }, nil
                end,
            })
            test_mocks.fire("session:created", {})

            expect.equal(1, #test_mocks.surface("session-board", "sessions").rows)
        end)

        -- A reload re-runs init.luau, so declare lands on a live surface.
        it("keeps its rows across a reload", function()
            board({ list = { { id = "s1", title = "kept", state = "Active" } } })
            expect.equal(1, #test_mocks.surface("session-board", "sessions").rows)

            plugin.setup({})

            local surface = test_mocks.surface("session-board", "sessions")
            expect.equal(1, #surface.rows)
            expect.equal("kept", surface.rows[1].text)
        end)
    end)
end)
