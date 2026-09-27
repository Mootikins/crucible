--!strict
-- Native Bases owns queries, grouping and writes. This plugin owns ticket policy.
local policy: any
local config: any = { folder = "tickets", base = "tickets.base", wip = {}, transitions = {} }
local function setup(cfg: any)
    if type(cfg) ~= "table" then cfg = {} end
    for _, key in { "folder", "base", "wip", "transitions" } do
        if cfg[key] ~= nil then config[key] = cfg[key] end
    end
    cru.on("base:before_write", {key="tickets"}, policy)
end
local function definition(): string
    return cru.json.encode({
        filters = 'file.inFolder(' .. cru.json.encode(config.folder) .. ') && file.ext == "md"',
        newItemFolder = config.folder,
        views = {{ type = "kanban", name = "Board", order = { "file.name", "note.status" },
            groupBy = { property = "note.status", direction = "ASC" }, groupOrder = { "todo", "doing", "done" } }},
    })
end
local function request(args: any, ctx: any): any
    assert(type(args.kiln) == "string", "kiln is required")
    return { session = ctx and ctx.session_id, source = { path = config.base }, view = "Board" }
end
local function board(args: any, ctx: any): any
    return cru.kiln.query(args.kiln, request(args, ctx))
end
local function initialize(args: any, ctx: any): any
    local options = request(args, ctx)
    assert(options.session, "Initialize tickets from a session")
    return cru.kiln.ensure_base(args.kiln, { session = options.session, path = config.base, yaml = definition() })
end
local function move(args: any, ctx: any): any
    local options = request(args, ctx)
    assert(options.session, "Move tickets from a session")
    assert(type(args.file) == "string" and not args.file:find("[/\\]"), "file must be a ticket filename")
    local path = config.folder .. "/" .. args.file
    local rows = cru.kiln.query(args.kiln, options).rows
    for _, row in rows do
        if row.path == path then
            return cru.kiln.set_property(args.kiln, { session = options.session, path = path, key = "status",
                value = args.to, ancestor_hash = args.ancestor_hash or row.ancestor_hash })
        end
    end
    error("Ticket not found")
end
policy = function(_ctx: any, event: any): any
    if type(event.path) ~= "string" or event.path:sub(1, #config.folder + 1) ~= config.folder .. "/" then return nil end
    local target = if event.key == "status" or event.key == "note.status" then event.value else event.group
    if target == nil then return nil end
    local allowed = config.transitions[event.old_value or ""]
    if allowed and event.old_value ~= target and not table.find(allowed, target) then
        return { cancel = true, reason = "Ticket transition is not allowed" }
    end
    local limit = config.wip[target]
    if limit and event.old_value ~= target then
        local result = cru.kiln.query(event.kiln, { source = { path = config.base }, view = "Board" })
        local count = 0
        for _, group in result.groups do
            if group.value.value == target then count = #group.rows end
        end
        if count >= limit then
            return { cancel = true, reason = "Ticket WIP limit reached" }
        end
    end
    return nil
end
local kiln_param = { name = "kiln", type = "string", desc = "Kiln name" }
return {
    setup = setup,
    tools = {
        kanban_board = { desc = "Query the native ticket base", params = { kiln_param }, fn = board },
        kanban_move = { desc = "Move a ticket through Bases policy and session review", params = {
            kiln_param, { name = "file", type = "string", desc = "Ticket filename" },
            { name = "to", type = "string", desc = "Target status" },
            { name = "ancestor_hash", type = "string", desc = "Expected ticket hash", optional = true },
        }, fn = move },
    },
    commands = {
        kanban = { desc = "Create tickets.base if absent", effect = "write", params = { kiln_param }, fn = initialize },
    },
}
