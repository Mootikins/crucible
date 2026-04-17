pub(super) const LUA_QOL: &str = r#"
-- ============================================================================
-- cru.inspect — Pretty-print any Lua value with cycle detection
-- ============================================================================

do
    local function inspect_impl(value, opts, seen, depth)
        opts = opts or {}
        local max_depth = opts.max_depth
        local indent_str = opts.indent or "  "
        seen = seen or {}
        depth = depth or 0

        local t = type(value)

        -- Handle nil, boolean, number
        if t == "nil" then
            return "nil"
        elseif t == "boolean" then
            return tostring(value)
        elseif t == "number" then
            return tostring(value)
        elseif t == "string" then
            return string.format("%q", value)
        elseif t == "function" then
            return "<function>"
        elseif t == "userdata" then
            return "<userdata>"
        elseif t == "thread" then
            return "<thread>"
        elseif t == "table" then
            -- Check for cycles
            if seen[value] then
                return "<cycle: table>"
            end

            -- Check depth limit
            if max_depth and depth >= max_depth then
                return "{...}"
            end

            seen[value] = true
            local indent = indent_str:rep(depth)
            local next_indent = indent_str:rep(depth + 1)
            local parts = {}

            for k, v in pairs(value) do
                local key_str
                if type(k) == "string" then
                    key_str = k
                else
                    key_str = "[" .. inspect_impl(k, opts, seen, depth + 1) .. "]"
                end
                local val_str = inspect_impl(v, opts, seen, depth + 1)
                table.insert(parts, next_indent .. key_str .. " = " .. val_str)
            end

            if #parts == 0 then
                return "{}"
            else
                return "{\n" .. table.concat(parts, ",\n") .. "\n" .. indent .. "}"
            end
        else
            return tostring(value)
        end
    end

    function cru.inspect(value, opts)
        return inspect_impl(value, opts)
    end
end

-- ============================================================================
-- cru.tbl_deep_extend — Deep merge tables with behavior semantics
-- ============================================================================

do
    local function deep_extend_impl(behavior, result, ...)
        local tables = {...}
        for _, tbl in ipairs(tables) do
            if type(tbl) == "table" then
                for k, v in pairs(tbl) do
                    if behavior == "force" then
                        -- Last wins: always override
                        if type(v) == "table" and type(result[k]) == "table" then
                            -- Recurse for nested tables
                            deep_extend_impl(behavior, result[k], v)
                        else
                            result[k] = v
                        end
                    elseif behavior == "keep" then
                        -- First wins: skip if already set
                        if result[k] == nil then
                            if type(v) == "table" then
                                -- Deep copy the table
                                result[k] = {}
                                deep_extend_impl(behavior, result[k], v)
                            else
                                result[k] = v
                            end
                        elseif type(v) == "table" and type(result[k]) == "table" then
                            -- Recurse even if key exists
                            deep_extend_impl(behavior, result[k], v)
                        end
                    end
                end
            end
        end
        return result
    end

    function cru.tbl_deep_extend(behavior, ...)
        cru.check.one_of(behavior, {"force", "keep"}, "behavior")
        local result = {}
        return deep_extend_impl(behavior, result, ...)
    end
end

-- ============================================================================
-- cru.tbl_get — Safe nested key access
-- ============================================================================

function cru.tbl_get(t, ...)
    if type(t) ~= "table" then return nil end
    local keys = {...}
    local current = t
    for _, key in ipairs(keys) do
        if type(current) ~= "table" then
            return nil
        end
        current = current[key]
        if current == nil then
            return nil
        end
    end
    return current
end

-- ============================================================================
-- cru.on_error — Overridable error handler hook
-- ============================================================================

cru.on_error = nil
"#;
