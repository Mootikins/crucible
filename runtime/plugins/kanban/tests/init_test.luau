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
describe("native kanban", function()
before_each(function()
    writes, ensured, rows, groups = {}, {}, {}, {}
    plugin.setup({folder="tickets",base="tickets.base",wip={},transitions={}})
    ;(cru.kiln :: any).query = function(kiln, options)
        expect.equal(kiln,"notes")
        expect.equal(options.source.path,"tickets.base")
        return {rows=rows,groups=groups}
    end
    ;(cru.kiln :: any).set_property = function(kiln, options)
        table.insert(writes, options)
        return {ok=true,status="proposed"}
    end
    ;(cru.kiln :: any).ensure_base = function(kiln, options)
        table.insert(ensured, options)
        return {ok=true,status="exists"}
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
        local event={kiln="notes",path="tickets/a.md",key="status",old_value="todo",value="doing"}
        expect.equal(policy({},event).cancel,true)
    end)
    it("uses native groups to enforce WIP and transition rules", function()
        plugin.setup({wip={doing=1},transitions={todo={"doing"}}})
        groups={{value={type="string",value="doing"},rows={{path="tickets/other.md"}}}}
        local event={kiln="notes",path="tickets/a.md",key="status",old_value="todo",value="doing"}
        expect.equal(policy({},event).cancel,true)
        event.value="done"
        expect.equal(policy({},event).cancel,true)
        groups={}
        event.value="doing"
        expect.equal(policy({},event),nil)
        event.path="other/a.md"
        expect.equal(policy({},event),nil)
    end)
end)
