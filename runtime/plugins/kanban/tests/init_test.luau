--!strict
-- Policy and delegation contracts; real Lua -> daemon writes are tested in bases/plugin_tests.rs.
local policy: any
;(cru :: any).on = function(name, _opts, fn)
    expect.equal(name, "base:before_write")
    policy = fn
end
local plugin = require("kanban")
local writes: { any } = {}
local ensured: { any } = {}
local rows: { any } = {}
local groups: { any } = {}
local pending: { any } = {}
local queries = 0
local answer: any = {status="proposed"}
describe("native kanban", function()
before_each(function()
    writes, ensured, rows, groups, pending, queries = {}, {}, {}, {}, {}, 0
    answer = {status="proposed"}
    plugin.setup({folder="tickets",base="tickets.base",wip={},transitions={}})
    ;(cru.kiln :: any).query = function(kiln, options)
        expect.equal(kiln,"notes")
        expect.equal(options.source.path,"tickets.base")
        queries += 1
        return {rows=rows,groups=groups}
    end
    ;(cru.kiln :: any).pending_writes = function(kiln, _options)
        expect.equal(kiln,"notes")
        return pending
    end
    ;(cru.kiln :: any).set_property = function(kiln, options)
        table.insert(writes, options)
        return answer
    end
    ;(cru.kiln :: any).ensure_base = function(kiln, options)
        table.insert(ensured, options)
        return {status="unchanged"}
    end
end)
    it("initializes through the absent-only daemon operation", function()
        plugin.commands.kanban.fn({kiln="notes"},{session_id="s1"})
        expect.equal(#ensured,1)
        expect.equal(ensured[1].session,"s1")
        expect.equal(ensured[1].path,"tickets.base")
        local base = cru.json.decode(ensured[1].yaml)
        expect.equal(base.views[1].type,"kanban")
    end)
    it("passes the explicit session and row ancestor to property writes", function()
        rows={{path="tickets/a.md",ancestor_hash="h1"}}
        local result = plugin.tools.kanban_move.fn({kiln="notes",file="a.md",to="doing"},{session_id="s1"})
        expect.equal(result.status,"proposed")
        expect.equal(writes[1].session,"s1")
        expect.equal(writes[1].ancestor_hash,"h1")
        expect.equal(writes[1].value,"doing")
    end)
    it("refuses sessionless commands and path-shaped ticket names", function()
        expect.equal(pcall(plugin.commands.kanban.fn,{kiln="notes"},{}),false)
        expect.equal(pcall(plugin.tools.kanban_move.fn,{kiln="notes",file="../a.md",to="doing"},{session_id="s1"}),false)
        expect.equal(#writes,0)
    end)
    it("enforces a zero WIP limit even before a group has entries", function()
        plugin.setup({wip={doing=0}})
        local event={kiln="notes",path="tickets/a.md",old_properties={status="todo"},properties={status="doing"}}
        expect.equal(policy({},event).cancel,true)
    end)
    it("uses native groups to enforce WIP and transition rules", function()
        plugin.setup({wip={doing=1},transitions={todo={"doing"}}})
        groups={{value={type="string",value="doing"},rows={{path="tickets/other.md"}}}}
        local event={kiln="notes",path="tickets/a.md",old_properties={status="todo"},properties={status="doing"}}
        expect.equal(policy({},event).cancel,true)
        event.properties.status="done"
        expect.equal(policy({},event).cancel,true)
        groups={}
        event.properties.status="doing"
        expect.equal(policy({},event),nil)
        event.path="other/a.md"
        expect.equal(policy({},event),nil)
    end)
    it("counts pending proposed moves against the WIP limit", function()
        plugin.setup({wip={doing=1}})
        local event={kiln="notes",path="tickets/b.md",old_properties={status="todo"},properties={status="doing"}}
        pending={{path="tickets/a.md",proposal="p1",properties={status="doing"}}}
        expect.equal(policy({},event).cancel,true)
        -- A proposal for the ticket being moved, or outside the folder, takes no place.
        pending={{path="tickets/b.md",proposal="p1",properties={status="doing"}},
            {path="other/c.md",proposal="p2",properties={status="doing"}}}
        expect.equal(policy({},event),nil)
        -- A ticket that is in the column on disk and also proposed counts once.
        plugin.setup({wip={doing=2}})
        groups={{value={type="string",value="doing"},rows={{path="tickets/a.md"}}}}
        pending={{path="tickets/a.md",proposal="p1",properties={status="doing"}}}
        expect.equal(policy({},event),nil)
    end)
    it("uses a given ancestor without querying the board", function()
        plugin.tools.kanban_move.fn({kiln="notes",file="a.md",to="doing",ancestor_hash="h9"},{session_id="s1"})
        expect.equal(queries,0)
        expect.equal(writes[1].ancestor_hash,"h9")
    end)
    it("fails the tool when the write is refused or stale", function()
        answer={status="refused",reason="Bases policy refused: Ticket WIP limit reached"}
        local ok, err = pcall(plugin.tools.kanban_move.fn,{kiln="notes",file="a.md",to="doing",ancestor_hash="h"},{session_id="s1"})
        expect.equal(ok,false)
        expect.equal(string.find(tostring(err),"WIP",1,true) ~= nil,true)
        answer={status="stale",current_hash="h2"}
        expect.equal(pcall(plugin.tools.kanban_move.fn,{kiln="notes",file="a.md",to="doing",ancestor_hash="h"},{session_id="s1"}),false)
    end)
end)
