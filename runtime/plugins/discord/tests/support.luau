--!strict
--- The `cru.session` test double shared by `session_state_test.luau` and
--- `sessions_test.luau`. Both files built their own byte-identical `Calls`
--- type and their own near-identical `recording_api` builder; this module
--- is the one copy.

local M = {}

--- The recorder. `calls` is a record, not a list: the tests read
--- `calls.created`, `calls.configured` and `calls.ended` by name.
export type Calls = { created: { any }, configured: number, ended: { any } }

--- Options for `recording_api`.
---
--- `prefix` names each created session id, as `"<prefix>-1"`,
--- `"<prefix>-2"`, and so on. It defaults to `"chat"`, the id
--- `sessions_test.luau` used before this move; `session_state_test.luau`
--- passes its own prefix, because its state file outlives each test and two
--- tests minting the same id would make "is this the id I created?"
--- unanswerable.
---
--- `configure_fails` makes `configure_agent` answer an error instead of
--- succeeding, for the tests that exercise the refuse-rather-than-cache path.
export type RecordingOpts = { prefix: string?, configure_fails: boolean? }

--- A session API that records what it was asked to do.
function M.recording_api(opts: RecordingOpts?): (Calls, { [string]: any })
    local o = opts or {}
    local prefix = o.prefix or "chat"
    local calls: Calls = { created = {}, configured = 0, ended = {} }
    return calls, {
        create = function(session)
            table.insert(calls.created, session)
            return { id = prefix .. "-" .. #calls.created }
        end,
        -- The bridge returns `(result, err)` rather than raising — every
        -- `cru.session.*` method surfaces its error as a second return value
        -- (`DaemonSessionApi`), and `configure_agent` branches on `err`.
        configure_agent = function(): (boolean?, string?)
            calls.configured = calls.configured + 1
            if o.configure_fails then
                return nil, "no such provider"
            end
            return true, nil
        end,
        end_session = function(id): (boolean, string?)
            table.insert(calls.ended, id)
            return true, nil
        end,
    }
end

return M
