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
local function in_folder(path: any): boolean
    return type(path) == "string" and path:sub(1, #config.folder + 1) == config.folder .. "/"
end
local function move(args: any, ctx: any): any
    local options = request(args, ctx)
    assert(options.session, "Move tickets from a session")
    assert(type(args.file) == "string" and not args.file:find("[/\\]"), "file must be a ticket filename")
    local path = config.folder .. "/" .. args.file
    local ancestor = args.ancestor_hash
    if ancestor == nil then
        for _, row in cru.kiln.query(args.kiln, options).rows do
            if row.path == path then ancestor = row.ancestor_hash end
        end
    end
    if ancestor == nil then error("Ticket not found") end
    local result = cru.kiln.set_property(args.kiln, { session = options.session, path = path, key = "status",
        value = args.to, ancestor_hash = ancestor })
    if result.status == "refused" then error(result.reason) end
    if result.status == "stale" then error("Ticket changed since it was read; query the board again") end
    return result
end
-- Tickets in `target`: on disk, plus pending proposals that would move one there.
local function occupancy(kiln: string, target: any, moving: string): number
    local counted: { [string]: boolean } = {}
    local count = 0
    local result = cru.kiln.query(kiln, { source = { path = config.base }, view = "Board" })
    for _, group in result.groups do
        if group.value.value == target then
            for _, row in group.rows do
                counted[row.path] = true
                count += 1
            end
        end
    end
    for _, write in cru.kiln.pending_writes(kiln, {}) do
        local status = write.properties and write.properties.status
        if in_folder(write.path) and write.path ~= moving and not counted[write.path] and status == target then
            counted[write.path] = true
            count += 1
        end
    end
    return count
end
policy = function(_ctx: any, event: any): any
    if not in_folder(event.path) then return nil end
    local target = event.properties and event.properties.status
    if target == nil then return nil end
    local old = event.old_properties and event.old_properties.status
    if event.previous_path and not in_folder(event.previous_path) then old = nil end
    local allowed = config.transitions[old or ""]
    if allowed and old ~= target and not table.find(allowed, target) then
        return { cancel = true, reason = "Ticket transition is not allowed" }
    end
    local limit = config.wip[target]
    if limit and old ~= target and occupancy(event.kiln, target, event.path) >= limit then
        return { cancel = true, reason = "Ticket WIP limit reached" }
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
