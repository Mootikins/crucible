--- Note writers for digest + entity outputs.
---
--- Path conventions (locked in plan):
---   Digests:  Sessions/<YYYY-MM-DD>-<session_id_prefix>.md  (prefix = first 8 chars)
---   Entities: Entities/<canonical_name>.md
---
--- Both use `cru.kiln.create_note(...)`. Digests never deduplicate;
--- collisions (same prefix + same date) get a -<n> suffix bumped until
--- create succeeds. Entity merges happen in dedupe.lua, which calls
--- create_note with `overwrite = true` after reconstructing the body.

local M = {}

local function today()
    return os.date("%Y-%m-%d")
end

--- Sanitize a string for use inside a filesystem path. Strict: only
--- A-Z, a-z, 0-9, space, hyphen, underscore, period. Everything else
--- becomes "_". Leading/trailing junk (dots, slashes, underscores) is
--- trimmed; a name that becomes empty after cleanup falls back to
--- "untitled" so we never write `Entities/.md`.
function M.slugify(name)
    if type(name) ~= "string" or name == "" then return "untitled" end
    local cleaned = name:gsub("[^%w%s%-%._]", "_")
    -- Strip leading/trailing dots, slashes, AND the underscores that
    -- the previous gsub may have introduced from slashes.
    cleaned = cleaned:gsub("^[%./_]+", ""):gsub("[%./_]+$", "")
    if cleaned == "" then return "untitled" end
    return cleaned
end

--- Build a wikilink target that points at a digest file by absolute
--- kiln path. The kiln stores notes by path, so wikilinks like
--- `[[Sessions/2026-05-11-abc123]]` resolve cleanly even though we
--- write `.md` on disk.
function M.digest_wikilink(rel_path)
    -- rel_path like "Sessions/2026-05-11-abc12345.md"
    local stem = rel_path:gsub("%.md$", "")
    return string.format("[[%s]]", stem)
end

local function build_digest_body(extraction, opts)
    local d = extraction.digest or {}
    opts = opts or {}
    local lines = {}
    lines[#lines + 1] = string.format("# Session %s", opts.date or today())
    lines[#lines + 1] = ""
    if d.summary and d.summary ~= "" then
        lines[#lines + 1] = d.summary
        lines[#lines + 1] = ""
    end

    if d.decisions and #d.decisions > 0 then
        lines[#lines + 1] = "## Decisions"
        for _, dec in ipairs(d.decisions) do
            lines[#lines + 1] = string.format("- %s", dec)
        end
        lines[#lines + 1] = ""
    end

    if d.action_items and #d.action_items > 0 then
        lines[#lines + 1] = "## Action Items"
        for _, item in ipairs(d.action_items) do
            lines[#lines + 1] = string.format("- %s", item)
        end
        lines[#lines + 1] = ""
    end

    local entities = extraction.entities or {}
    if #entities > 0 then
        lines[#lines + 1] = "## Entities Mentioned"
        for _, ent in ipairs(entities) do
            local name = ent.name or "Unknown"
            lines[#lines + 1] = string.format("- [[Entities/%s]]", M.slugify(name))
        end
        lines[#lines + 1] = ""
    end

    return table.concat(lines, "\n")
end

local function digest_frontmatter(extraction, opts)
    local d = extraction.digest or {}
    return {
        type = "session-digest",
        session_id = opts.session_id,
        date = opts.date or today(),
        topics = d.topics or {},
        scope = "workspace",
    }
end

local function digest_path(opts, suffix)
    local prefix = (opts.session_id or "unknown"):sub(1, 8)
    local date = opts.date or today()
    local suffix_str = suffix and string.format("-%d", suffix) or ""
    return string.format("Sessions/%s-%s%s.md", date, prefix, suffix_str)
end

--- Write a session digest note. Never overwrites — appends `-<n>` on
--- collision until a free path is found, up to a safety bound of 100
--- attempts.
---
--- Returns absolute_path, nil or nil, error_string.
function M.write_digest(extraction, opts, deps)
    deps = deps or {}
    local kiln = deps.kiln or (cru and cru.kiln)
    if not kiln or not kiln.create_note then
        return nil, "cru.kiln.create_note not available"
    end

    opts = opts or {}
    local body = build_digest_body(extraction, opts)
    local fm = digest_frontmatter(extraction, opts)

    -- Try without suffix; on collision, bump suffix. Distinguishing
    -- "already exists" from "different create failure" requires string
    -- matching since create_note errors return strings.
    local attempt = 0
    while attempt < 100 do
        -- Note: `attempt == 0 and nil or attempt` is the Lua trap —
        -- `nil` is falsy so the conditional returns `attempt` (0) instead
        -- of nil. Use an explicit branch.
        local suffix
        if attempt > 0 then suffix = attempt end
        local path = digest_path(opts, suffix)
        local abs, err = kiln.create_note({
            path = path,
            body = body,
            frontmatter = fm,
            overwrite = false,
        })
        if abs then
            return abs, nil, path
        end
        -- Collision heuristics: T0a's stub raises with "file exists" /
        -- "path exists". We accept either phrase. Anything else is a
        -- real failure and we surface it immediately.
        local err_str = tostring(err or "")
        if not (err_str:find("exists", 1, true) or err_str:find("file_exists", 1, true)) then
            return nil, err_str
        end
        attempt = attempt + 1
    end
    return nil, "exhausted digest path suffixes"
end

local function entity_path(entity)
    local name = entity.canonical_name or entity.name or "Untitled"
    return string.format("Entities/%s.md", M.slugify(name))
end

local function entity_frontmatter(entity)
    return {
        type = "entity",
        entity_type = entity.type or entity.entity_type or "concept",
        canonical_name = entity.canonical_name or entity.name or "Untitled",
        aliases = entity.aliases or {},
        scope = "workspace",
    }
end

--- Format a fact line with a source wikilink. The trailing source
--- citation is the seam dedupe.lua uses to deduplicate verbatim repeats
--- across sessions — keep it stable.
function M.format_fact(text, source_wikilink)
    if source_wikilink and source_wikilink ~= "" then
        return string.format("- %s (source: %s)", text, source_wikilink)
    end
    return string.format("- %s", text)
end

local function build_entity_body(entity, facts_lines)
    local lines = {}
    local name = entity.canonical_name or entity.name or "Untitled"
    lines[#lines + 1] = string.format("# %s", name)
    lines[#lines + 1] = ""
    lines[#lines + 1] = "## Facts"
    for _, line in ipairs(facts_lines) do
        lines[#lines + 1] = line
    end
    lines[#lines + 1] = ""
    return table.concat(lines, "\n")
end

--- Write a new entity note. Caller is responsible for deduplication;
--- this overwrites unconditionally so dedupe.lua can rebuild the body
--- with merged facts.
function M.write_entity(entity, facts_lines, deps)
    deps = deps or {}
    local kiln = deps.kiln or (cru and cru.kiln)
    if not kiln or not kiln.create_note then
        return nil, "cru.kiln.create_note not available"
    end

    local path = entity_path(entity)
    local body = build_entity_body(entity, facts_lines)
    local fm = entity_frontmatter(entity)

    local abs, err = kiln.create_note({
        path = path,
        body = body,
        frontmatter = fm,
        overwrite = true,
    })
    if not abs then
        return nil, tostring(err or "create_note failed")
    end
    return abs, nil, path
end

-- Test surface
M._build_digest_body = build_digest_body
M._digest_path = digest_path
M._entity_path = entity_path
M._build_entity_body = build_entity_body

return M
