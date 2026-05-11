--- Entity-note dedupe + merge.
---
--- For each freshly-extracted entity:
---   1. cru.kiln.search(name, { threshold }) → top-k existing notes
---   2. If a result has frontmatter `type = "entity"` AND canonical_name
---      matches (or alias overlap), MERGE: append non-duplicate facts to
---      its `## Facts` section, then rewrite via cru.kiln.create_note(
---      overwrite = true ).
---   3. Otherwise, CREATE a new entity note.
---
--- ⚠️ Race: two `on_session_end` hooks firing simultaneously for the
--- same entity could both clobber each other's body since we
--- read-merge-overwrite without a lock. `cru.kiln` has no update-note
--- API yet so we accept this tradeoff for Wave 2; documented in the
--- plan's "Conflict resolution" section. Concurrent-extraction locking
--- is explicitly listed as out-of-scope.

local write = require("lua.write")

local M = {}

-- ─────────────────────────────────────────────────────────────────────────────
-- Match scoring
-- ─────────────────────────────────────────────────────────────────────────────

local function lower(s)
    if type(s) ~= "string" then return "" end
    return s:lower()
end

--- Does an existing note's frontmatter look like the entity we extracted?
---
--- We accept any of:
---   - canonical_name (case-insensitive) matches entity name
---   - entity name appears in existing aliases (case-insensitive)
---   - any alias of the extracted entity matches existing canonical_name
local function frontmatter_matches(props, entity)
    if not props or type(props) ~= "table" then return false end
    if props.type ~= "entity" and props.entity_type == nil then
        -- T0a's NoteRecord puts frontmatter under `properties`. We
        -- require either `type: entity` or at least an `entity_type`
        -- field; otherwise the note is some unrelated topical note
        -- that happened to match the name keyword.
        if props.type ~= "entity" then return false end
    end

    local name = lower(entity.canonical_name or entity.name)
    if name == "" then return false end

    if lower(props.canonical_name) == name then return true end

    local existing_aliases = props.aliases
    if type(existing_aliases) == "table" then
        for _, a in ipairs(existing_aliases) do
            if lower(a) == name then return true end
        end
    end

    local new_aliases = entity.aliases
    if type(new_aliases) == "table" then
        local existing_canon = lower(props.canonical_name)
        for _, a in ipairs(new_aliases) do
            if lower(a) == existing_canon then return true end
        end
    end

    return false
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Existing-fact extraction
-- ─────────────────────────────────────────────────────────────────────────────

--- Pull the `## Facts` bullet lines verbatim out of an existing
--- entity note. We re-emit them as-is on rewrite so we preserve any
--- human edits the user made to existing facts (e.g. typo fixes).
---
--- Returns array of fact LINES (with leading `- ` already attached).
function M.parse_existing_facts(body)
    if type(body) ~= "string" or body == "" then return {} end
    local lines = {}
    local in_facts = false
    for line in (body .. "\n"):gmatch("([^\n]*)\n") do
        if line:match("^##%s+Facts%s*$") then
            in_facts = true
        elseif in_facts and line:match("^##%s+") then
            in_facts = false
        elseif in_facts then
            local trimmed = line:match("^%s*(.-)%s*$") or line
            if trimmed:sub(1, 1) == "-" then
                lines[#lines + 1] = trimmed
            end
        end
    end
    return lines
end

--- Compare two fact lines for verbatim equality after normalising
--- whitespace + the trailing source citation. We want
---     "- works on X (source: [[A]])"
---     "- works on X (source: [[B]])"
--- to dedupe to ONE entry — the SAME fact from two sessions. Strip
--- the parenthetical before comparing.
function M.fact_core(line)
    if type(line) ~= "string" then return "" end
    local without_source = line:gsub("%s*%(source:[^%)]*%)%s*$", "")
    return (without_source:match("^%s*(.-)%s*$") or without_source):lower()
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Decision: merge vs create
-- ─────────────────────────────────────────────────────────────────────────────

--- Find a matching existing note for `entity`, returning the kiln record
--- or nil. Pure (no writes).
function M.find_existing(entity, deps)
    deps = deps or {}
    local kiln = deps.kiln or (cru and cru.kiln)
    if not kiln or not kiln.search or not kiln.get then return nil end

    local threshold = deps.threshold or 0.80
    local query = entity.name or entity.canonical_name
    if not query or query == "" then return nil end

    local results = kiln.search(query, { limit = 5, threshold = threshold }) or {}
    for _, hit in ipairs(results) do
        local record = kiln.get(hit.path)
        if record and frontmatter_matches(record.properties, entity) then
            return record
        end
    end
    return nil
end

--- Process all entities for a single session.
---
--- Returns a summary table for tests / logging:
---   { created = {paths…}, merged = {paths…}, skipped = {names…} }
function M.process(entities, opts, deps)
    opts = opts or {}
    deps = deps or {}
    local kiln = deps.kiln or (cru and cru.kiln)
    local summary = { created = {}, merged = {}, skipped = {} }
    if not entities or #entities == 0 then return summary end

    local source = opts.source_wikilink
    local merge_deps = { kiln = kiln, threshold = opts.dedupe_threshold }

    for _, entity in ipairs(entities) do
        local name = entity.name or entity.canonical_name
        local fact_list = entity.facts or {}
        if not name or name == "" or #fact_list == 0 then
            summary.skipped[#summary.skipped + 1] = name or "<unnamed>"
        else
            local existing = M.find_existing(entity, merge_deps)
            if existing then
                local existing_lines = M.parse_existing_facts(existing.body or existing.content or "")
                local seen = {}
                for _, line in ipairs(existing_lines) do
                    seen[M.fact_core(line)] = true
                end
                local merged_lines = {}
                for _, line in ipairs(existing_lines) do
                    merged_lines[#merged_lines + 1] = line
                end
                for _, fact in ipairs(fact_list) do
                    local core = M.fact_core("- " .. fact)
                    if not seen[core] then
                        seen[core] = true
                        merged_lines[#merged_lines + 1] = write.format_fact(fact, source)
                    end
                end

                -- Carry over the existing canonical_name + aliases so
                -- we don't lose user edits. Aliases get unioned with
                -- any new ones the extractor proposed.
                local merged_entity = {
                    canonical_name = (existing.properties and existing.properties.canonical_name)
                        or entity.canonical_name or entity.name,
                    name = entity.name,
                    type = (existing.properties and existing.properties.entity_type)
                        or entity.type or "concept",
                    aliases = M.merge_aliases(
                        existing.properties and existing.properties.aliases,
                        entity.aliases
                    ),
                }

                local abs, err, rel = write.write_entity(merged_entity, merged_lines, { kiln = kiln })
                if abs then
                    summary.merged[#summary.merged + 1] = rel or abs
                else
                    summary.skipped[#summary.skipped + 1] = name .. " (merge failed: " .. tostring(err) .. ")"
                end
            else
                -- Brand new entity: synthesize a fresh facts list with source citation.
                local fact_lines = {}
                for _, fact in ipairs(fact_list) do
                    fact_lines[#fact_lines + 1] = write.format_fact(fact, source)
                end
                local fresh_entity = {
                    canonical_name = entity.canonical_name or entity.name,
                    name = entity.name,
                    type = entity.type,
                    aliases = entity.aliases or {},
                }
                local abs, err, rel = write.write_entity(fresh_entity, fact_lines, { kiln = kiln })
                if abs then
                    summary.created[#summary.created + 1] = rel or abs
                else
                    summary.skipped[#summary.skipped + 1] = name .. " (create failed: " .. tostring(err) .. ")"
                end
            end
        end
    end

    return summary
end

--- Union two alias arrays, preserving order from `a` then appending
--- new values from `b`. Comparison is case-insensitive but the original
--- casing is retained for display.
function M.merge_aliases(a, b)
    local seen = {}
    local out = {}
    if type(a) == "table" then
        for _, v in ipairs(a) do
            local key = lower(v)
            if key ~= "" and not seen[key] then
                seen[key] = true
                out[#out + 1] = v
            end
        end
    end
    if type(b) == "table" then
        for _, v in ipairs(b) do
            local key = lower(v)
            if key ~= "" and not seen[key] then
                seen[key] = true
                out[#out + 1] = v
            end
        end
    end
    return out
end

M._frontmatter_matches = frontmatter_matches

return M
