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
-- cru.settings — the layered configuration of one plugin
-- ============================================================================
--
-- One order for every plugin, highest priority first:
--   1. the environment, for a key that `opts.secrets` names:
--      `CRUCIBLE_<PLUGIN>_<KEY>`. Only secrets, because an environment that
--      redirects a URL is a change that a user cannot read back.
--   2. values passed to `init(cfg)`: the host calls the plugin's setup()
--      with its `plugins.<name>` section, and a user's own setup() call
--      lays over it. The last call wins for each key.
--   3. the `plugins.<name>` section, through `cru.plugin.config.get`. A key
--      that a module reads before setup() runs comes from here. A VM with no
--      daemon (the plugin test runner) has no `cru.plugin`, and the layer
--      answers nil.
--   4. `defaults`.
--   5. the caller's `fallback`.

do
    local Settings = {}

    local function env_name(plugin, key)
        local upper = function(s)
            return (s:upper():gsub("[^A-Z0-9]", "_"))
        end
        return "CRUCIBLE_" .. upper(plugin) .. "_" .. upper(key)
    end

    function Settings.new(plugin, defaults, opts)
        cru.check.string(plugin, "plugin")
        cru.check.table(defaults, "defaults", { optional = true })
        cru.check.table(opts, "opts", { optional = true })
        defaults = defaults or {}
        local secrets = (opts and opts.secrets) or {}
        local configured = {}
        local settings = {}

        local function from_env(key)
            if not secrets[key] then return nil end
            local val = os.getenv(env_name(plugin, key))
            if val == nil or val == "" then return nil end
            return val
        end

        local function from_section(key)
            local host = rawget(cru, "plugin")
            local config = type(host) == "table" and rawget(host, "config") or nil
            local get = type(config) == "table" and rawget(config, "get") or nil
            if type(get) ~= "function" then return nil end
            local ok, val = pcall(get, plugin .. "." .. key)
            if ok then return val end
            return nil
        end

        --- Lay `cfg` over the setup layer, key by key.
        function settings.init(cfg)
            if not cfg then return end
            for k, v in pairs(cfg) do
                configured[k] = v
            end
        end

        --- The value of `key`, from the first layer that has one.
        function settings.get(key, fallback)
            local env = from_env(key)
            if env ~= nil then return env end
            if configured[key] ~= nil then return configured[key] end
            local val = from_section(key)
            if val ~= nil then return val end
            if defaults[key] ~= nil then return defaults[key] end
            return fallback
        end

        --- Clear the setup layer. Tests only: the daemon never unconfigures
        --- a plugin, but a suite that cannot clear setup() has
        --- order-dependent tests.
        function settings.reset()
            configured = {}
        end

        return settings
    end

    cru.settings = Settings
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

        -- Resolve the config schema through the same layers every plugin
        -- reads: a secret from the environment, then the plugin's section,
        -- then the schema default.
        local resolved_config = nil
        if spec.config then
            resolved_config = {}
            local defaults, secrets = {}, {}
            for key, schema in pairs(spec.config) do
                defaults[key] = schema.default
                secrets[key] = schema.secret or nil
            end
            local settings = cru.settings.new(name, defaults, { secrets = secrets })
            for key in pairs(spec.config) do
                resolved_config[key] = settings.get(key)
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
