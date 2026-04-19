pub(super) const LUA_STDLIB: &str = r#"
-- ============================================================================
-- cru.retry — Exponential backoff with jitter
-- ============================================================================

function cru.retry(fn, opts)
    opts = opts or {}
    local max = opts.max_retries or 3
    local base = opts.base_delay or 1.0
    local cap = opts.max_delay or 60.0
    local use_jitter = opts.jitter ~= false
    local is_retryable = opts.retryable or function() return true end

    for attempt = 0, max do
        local ok, result = pcall(fn)
        if ok then return result end
        if attempt == max then error(result) end
        if not is_retryable(result) then error(result) end

        local delay = math.min(base * (2 ^ attempt), cap)
        if use_jitter then
            delay = delay * (0.5 + math.random() * 0.5)
        end

        -- Honor server-specified retry-after
        if type(result) == "table" and result.after then
            delay = math.max(delay, tonumber(result.after) or delay)
        end

        cru.timer.sleep(delay)
    end
end

-- ============================================================================
-- cru.emitter — Minimal event emitter
-- ============================================================================

do
    local Emitter = {}
    Emitter.__index = Emitter

    local function get_fn(entry)
        if type(entry) == 'table' then
            return entry.fn
        end
        return entry
    end

    function Emitter.new()
        return setmetatable({ _listeners = {} }, Emitter)
    end

    function Emitter:on(event, fn, owner)
        if not self._listeners[event] then
            self._listeners[event] = {}
        end
        local list = self._listeners[event]
        local id = #list + 1
        if owner ~= nil then
            list[id] = { fn = fn, owner = owner }
        else
            list[id] = fn
        end
        return id
    end

    function Emitter:once(event, fn, owner)
        local id
        id = self:on(event, function(...)
            self:off(event, id)
            fn(...)
        end, owner)
        return id
    end

    function Emitter:off(event, id)
        if self._listeners[event] then
            self._listeners[event][id] = false
        end
    end

    function Emitter:emit(event, ...)
        local listeners = self._listeners[event]
        if not listeners then return end
        for i = 1, #listeners do
            local entry = listeners[i]
            if entry then
                local fn = get_fn(entry)
                if fn then
                    local ok, err = pcall(fn, ...)
                    if not ok then
                        cru.log("warn", "emitter handler error on '" .. event .. "': " .. tostring(err))
                        if cru.errors and cru.errors._capture then
                            local owner = (type(entry) == 'table' and entry.owner) or "unknown"
                            cru.errors._capture(owner, tostring(err), "emitter:emit('" .. tostring(event) .. "')")
                        end
                    end
                end
            end
        end
    end

    -- Fire-and-forget emit: returns immediately, errors are silently swallowed.
    -- Semantically "async" — callers must not depend on handler completion or
    -- error propagation.  In pure Lua this still executes synchronously.
    function Emitter:emit_async(event, ...)
        local listeners = self._listeners[event]
        if not listeners then return end
        for i = 1, #listeners do
            local entry = listeners[i]
            if entry then
                local fn = get_fn(entry)
                if fn then
                    local ok, err = pcall(fn, ...)
                    if not ok then
                        cru.log("warn", "emitter handler error on '" .. event .. "': " .. tostring(err))
                        if cru.errors and cru.errors._capture then
                            local owner = (type(entry) == 'table' and entry.owner) or "unknown"
                            cru.errors._capture(owner, tostring(err), "emitter:emit_async('" .. tostring(event) .. "')")
                        end
                    end
                end
            end
        end
    end

    -- Count active listeners for an event (excludes removed ones)
    function Emitter:count(event)
        local listeners = self._listeners[event]
        if not listeners then return 0 end
        local n = 0
        for i = 1, #listeners do
            if listeners[i] then n = n + 1 end
        end
        return n
    end

    function Emitter:unregister_owner(owner)
        for _, listeners in pairs(self._listeners) do
            for i = 1, #listeners do
                local entry = listeners[i]
                if type(entry) == 'table' and entry.owner == owner then
                    listeners[i] = false
                end
            end
        end
    end

    function Emitter:off_all(event)
        if event then
            self._listeners[event] = nil
        else
            self._listeners = {}
        end
    end

    -- Global shared emitter singleton (stored in closure scope)
    local _global_emitter = nil
    local function get_global()
        if not _global_emitter then
            _global_emitter = Emitter.new()
        end
        return _global_emitter
    end

    cru.emitter = { new = Emitter.new, global = get_global }
end

-- ============================================================================
-- cru.check — Argument validation
-- ============================================================================

do
    local check = {}

    local function fail(name, expected, got)
        error(string.format("%s: expected %s, got %s", name, expected, type(got)), 3)
    end

    function check.string(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "string" then fail(name, "string", val) end
    end

    function check.number(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "number" then fail(name, "number", val) end
        if opts then
            if opts.min and val < opts.min then
                error(string.format("%s: must be >= %s, got %s", name, opts.min, val), 2)
            end
            if opts.max and val > opts.max then
                error(string.format("%s: must be <= %s, got %s", name, opts.max, val), 2)
            end
        end
    end

    function check.boolean(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "boolean" then fail(name, "boolean", val) end
    end

    function check.table(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "table" then fail(name, "table", val) end
    end

    function check.one_of(val, choices, name, opts)
        if opts and opts.optional and val == nil then return end
        for _, v in ipairs(choices) do
            if val == v then return end
        end
        error(string.format("%s: must be one of [%s], got %s",
            name, table.concat(choices, ", "), tostring(val)), 2)
    end

    function check.func(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "function" then fail(name, "function", val) end
    end

    cru.check = check
end

-- ============================================================================
-- cru.service — Supervised service lifecycle
-- ============================================================================

do
    local Service = {}
    Service._services = {}

    function Service.define(spec)
        cru.check.table(spec, "spec")
        cru.check.string(spec.name, "spec.name")
        cru.check.string(spec.desc, "spec.desc")
        cru.check.func(spec.start, "spec.start")
        cru.check.func(spec.stop, "spec.stop", { optional = true })
        cru.check.func(spec.health, "spec.health", { optional = true })

        local name = spec.name
        local restart = spec.restart or {}
        local max_retries  = restart.max_retries or 10
        local base_delay   = restart.base_delay  or 1.0
        local max_delay    = restart.max_delay    or 60.0

        -- Resolve config schema defaults and secrets
        local resolved_config = nil
        if spec.config then
            resolved_config = {}
            local plugin_upper = name:upper():gsub("[^A-Z0-9]", "_")
            for key, schema in pairs(spec.config) do
                local val = nil
                -- Secret resolution: env var first
                if schema.secret then
                    local env_key = "CRUCIBLE_" .. plugin_upper .. "_" .. key:upper():gsub("[^A-Z0-9]", "_")
                    val = os.getenv(env_key)
                end
                -- Fall back to plugin config
                if val == nil then
                    local ok, cfg_val = pcall(function()
                        return crucible.config.get(name .. "." .. key)
                    end)
                    if ok and cfg_val ~= nil then val = cfg_val end
                end
                -- Fall back to schema default
                if val == nil and schema.default ~= nil then
                    val = schema.default
                end
                resolved_config[key] = val
            end
        end

        local entry = {
            name      = name,
            desc      = spec.desc,
            running   = false,
            healthy   = nil,
            start_fn  = spec.start,
            stop_fn   = spec.stop,
            health_fn = spec.health,
            config    = resolved_config,
        }
        Service._services[name] = entry

        -- Build the wrapper function the daemon spawns
        local function wrapped()
            entry.running = true
            cru.log("info", "service '" .. name .. "' starting")

            local ok, err = pcall(function()
                cru.retry(function()
                    entry.running = true
                    spec.start()
                end, {
                    max_retries = max_retries,
                    base_delay  = base_delay,
                    max_delay   = max_delay,
                    retryable   = function(e)
                        return type(e) ~= "table" or e.retryable ~= false
                    end,
                })
            end)

            entry.running = false
            if not ok then
                cru.log("warn", "service '" .. name .. "' stopped: " .. tostring(err))
            else
                cru.log("info", "service '" .. name .. "' completed")
            end
        end

        return { desc = spec.desc, fn = wrapped }
    end

    function Service.status(name)
        local entry = Service._services[name]
        if not entry then return nil end
        local healthy = nil
        if entry.health_fn then
            local ok, h = pcall(entry.health_fn)
            healthy = ok and h or false
        end
        return { running = entry.running, healthy = healthy, name = entry.name, desc = entry.desc }
    end

    function Service.list()
        local out = {}
        for _, entry in pairs(Service._services) do
            local healthy = nil
            if entry.health_fn then
                local ok, h = pcall(entry.health_fn)
                healthy = ok and h or false
            end
            out[#out + 1] = { name = entry.name, desc = entry.desc, running = entry.running, healthy = healthy }
        end
        return out
    end

    function Service.stop(name)
        local entry = Service._services[name]
        if not entry then return false end
        if entry.stop_fn then
            local ok, err = pcall(entry.stop_fn)
            if not ok then
                cru.log("warn", "service '" .. name .. "' stop error: " .. tostring(err))
            end
        end
        entry.running = false
        return true
    end

    cru.service = Service
end
"#;
