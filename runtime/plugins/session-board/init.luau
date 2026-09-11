--!strict
--- session-board — every live session as a surface, in the TUI and the browser.
---
--- The smallest plugin that owns a UI surface, and the reference for the two
--- things a surface author has to get right:
---
--- 1. **Rows are data, never presentation.** A row states a `mark` from the
---    declared set and the client decides what that looks like — the TUI draws
---    `●`, the browser draws a coloured dot. A plugin that shipped the glyph
---    would bind one client's medium into a contract both must honour.
---
--- 2. **`declare` is idempotent, `set_rows` is the update.** A reload re-runs
---    this file, so `declare` runs again on a live surface. It keeps the rows and
---    the version, which is what lets a client's open panel survive a reload.
---
--- `cru.session.list()` returns a value AND an error, so the error half must be
--- read. Calling `ipairs` straight on the result raises on failure, which is a
--- background refresh turning into a broken panel.
---
--- Note the singular: the typed declarations carry `cru.session.*` only.
--- `cru.sessions` still answers at runtime as a deprecated alias, so a plugin
--- that writes the plural works and then fails the Luau checker.

local M = {}

local SURFACE = "sessions"

--- Map a session's state to a declared mark.
---
--- The vocabulary is closed (`busy`, `blocked`, `ok`, `failed`) and an unknown
--- name is dropped by the host rather than refused, so a state this table does
--- not know renders without a mark instead of failing the push.
local MARKS: { [string]: string } = {
    Active = "busy",
    Running = "busy",
    Waiting = "blocked",
    Ended = "ok",
    Archived = "ok",
    Failed = "failed",
    Error = "failed",
}

--- Build one row per session.
function M.rows(): { { [string]: any } }
    local sessions, err = cru.session.list()
    if err ~= nil or type(sessions) ~= "table" then
        -- Report nothing rather than an empty board: an empty list means "no
        -- sessions", and a failed call must not be shown as the same thing.
        cru.log("warn", "session-board: could not list sessions: " .. tostring(err))
        return {}
    end

    local rows = {}
    for _, s in ipairs(sessions) do
        local id = tostring(s.id or s.session_id or "")
        if id ~= "" then
            table.insert(rows, {
                id = id,
                -- A title when the session has earned one, else its id. A board
                -- of identical ids is unreadable.
                text = s.title or id,
                detail = s.agent_model or s.type,
                mark = MARKS[tostring(s.state)],
            })
        end
    end
    return rows
end

--- Push the current sessions onto the surface.
function M.refresh()
    cru.surface.set_rows({
        plugin = "session-board",
        name = SURFACE,
        rows = M.rows(),
    })
end

function M.setup(_cfg)
    cru.surface.declare({
        plugin = "session-board",
        name = SURFACE,
        title = "Sessions",
        shape = "list",
    })

    -- Both halves matter: `session:created` and `session:ended` are daemon-wide,
    -- so this plugin sees every session rather than only the one it runs in.
    cru.on("session:created", function()
        M.refresh()
    end)
    cru.on("session:ended", function()
        M.refresh()
    end)

    -- Draw once at load, so the board is populated before anything happens.
    M.refresh()
end

return {
    author = "Crucible Contributors",
    license = "MIT",
    name = "session-board",
    version = "1.0.0",
    description = "Every live session as a surface, in the TUI and the browser",

    setup = M.setup,

    commands = {
        ["session-board"] = {
            desc = "Refresh the session board",
            fn = function()
                M.refresh()
                return { ok = true, rows = #M.rows() }
            end,
        },
    },
}
