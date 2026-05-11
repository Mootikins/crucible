--- Tests for lua/dedupe.lua + lua/write.lua merge logic.
---
--- We mock cru.kiln in two flavours:
---   • An empty kiln (no search hits) — every entity becomes a new note.
---   • A seeded kiln with a pre-existing entity note — fresh facts get
---     merged into the existing body, verbatim duplicates are skipped.
---
--- create_note writes are captured in a `created` map so tests can
--- assert path + body shape without touching the real filesystem.

local dedupe = require("lua.dedupe")
local write = require("lua.write")

local function make_kiln_mock(seed)
    seed = seed or {}
    local state = {
        notes = {}, -- path -> { body, properties, ... }
        create_log = {},
        search_log = {},
    }
    for path, note in pairs(seed) do
        state.notes[path] = note
    end

    local kiln = {}

    function kiln.search(query, opts)
        state.search_log[#state.search_log + 1] = { query = query, opts = opts }
        -- Crude substring match against title + canonical_name + aliases.
        -- Good enough for tests — production goes through embeddings,
        -- which would map "JD" → "Jane Doe" by semantic similarity.
        local results = {}
        local q = (query or ""):lower()
        for path, note in pairs(state.notes) do
            local title = (note.title or ""):lower()
            local props = note.properties or {}
            local canon = (props.canonical_name or ""):lower()
            local match = title:find(q, 1, true) or canon:find(q, 1, true)
            if not match and type(props.aliases) == "table" then
                for _, alias in ipairs(props.aliases) do
                    if (alias or ""):lower():find(q, 1, true) then
                        match = true
                        break
                    end
                end
            end
            if match then
                results[#results + 1] = {
                    path = path,
                    title = note.title or path,
                    score = 0.9,
                    snippet = nil,
                }
            end
        end
        return results
    end

    function kiln.get(path)
        local note = state.notes[path]
        if not note then return nil end
        -- Return a shallow copy so callers can't mutate our seed.
        local copy = {}
        for k, v in pairs(note) do copy[k] = v end
        return copy
    end

    function kiln.create_note(opts)
        state.create_log[#state.create_log + 1] = opts
        -- Simulate "writing" the note into our in-memory kiln so
        -- subsequent search/get calls within the same test see it.
        local props = opts.frontmatter or {}
        state.notes[opts.path] = {
            path = opts.path,
            title = props.canonical_name or opts.path,
            body = opts.body,
            properties = props,
        }
        return "/abs/" .. opts.path, nil
    end

    return kiln, state
end

-- ─────────────────────────────────────────────────────────────────────────────
-- Fact parsing
-- ─────────────────────────────────────────────────────────────────────────────

describe("dedupe.parse_existing_facts", function()
    it("returns empty list when body is empty", function()
        assert.equal(0, #dedupe.parse_existing_facts(""))
        assert.equal(0, #dedupe.parse_existing_facts(nil))
    end)

    it("extracts only bullets under the Facts heading", function()
        local body = [=[
# Jane Doe

## Background
- Some bio

## Facts
- works on Crucible (source: [[Sessions/old]])
- prefers Rust
- another fact

## See also
- Linked elsewhere
]=]
        local lines = dedupe.parse_existing_facts(body)
        assert.equal(3, #lines)
        assert.truthy(lines[1]:find("works on Crucible", 1, true))
        assert.truthy(lines[2]:find("prefers Rust", 1, true))
        assert.truthy(lines[3]:find("another fact", 1, true))
    end)
end)

describe("dedupe.fact_core", function()
    it("strips trailing source citation", function()
        local a = "- works on X (source: [[A]])"
        local b = "- works on X (source: [[B]])"
        assert.equal(dedupe.fact_core(a), dedupe.fact_core(b))
    end)

    it("is case-insensitive", function()
        local a = "- Built the parser"
        local b = "- built the parser"
        assert.equal(dedupe.fact_core(a), dedupe.fact_core(b))
    end)
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- find_existing
-- ─────────────────────────────────────────────────────────────────────────────

describe("dedupe.find_existing", function()
    it("returns nil when search yields no hits", function()
        local kiln, _ = make_kiln_mock({})
        local existing = dedupe.find_existing(
            { name = "Jane Doe", aliases = {} },
            { kiln = kiln, threshold = 0.6 }
        )
        assert.is_nil(existing)
    end)

    it("returns the matching note when canonical_name matches", function()
        local kiln, _ = make_kiln_mock({
            ["Entities/Jane Doe.md"] = {
                title = "Jane Doe",
                body = "# Jane Doe\n\n## Facts\n- existing fact\n",
                properties = {
                    type = "entity",
                    canonical_name = "Jane Doe",
                    aliases = {},
                    entity_type = "person",
                },
            },
        })

        local existing = dedupe.find_existing(
            { name = "Jane Doe", aliases = {} },
            { kiln = kiln, threshold = 0.5 }
        )
        assert.truthy(existing)
        assert.equal("Jane Doe", existing.properties.canonical_name)
    end)

    it("ignores notes without entity frontmatter", function()
        local kiln, _ = make_kiln_mock({
            ["Notes/Jane Doe topic.md"] = {
                title = "Jane Doe",
                body = "Unrelated note",
                properties = { type = "note" },
            },
        })

        local existing = dedupe.find_existing(
            { name = "Jane Doe", aliases = {} },
            { kiln = kiln, threshold = 0.5 }
        )
        assert.is_nil(existing)
    end)

    it("matches via alias overlap", function()
        local kiln, _ = make_kiln_mock({
            ["Entities/Jane Doe.md"] = {
                title = "Jane Doe",
                body = "",
                properties = {
                    type = "entity",
                    canonical_name = "Jane Doe",
                    aliases = { "JD", "Janie" },
                },
            },
        })

        local existing = dedupe.find_existing(
            { name = "JD", aliases = {} },
            { kiln = kiln, threshold = 0.5 }
        )
        assert.truthy(existing)
    end)
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- process()
-- ─────────────────────────────────────────────────────────────────────────────

describe("dedupe.process", function()
    it("creates new notes for unseen entities", function()
        local kiln, state = make_kiln_mock({})
        local summary = dedupe.process(
            {
                { name = "Crucible", type = "project", facts = { "is the runtime" } },
                { name = "Jane",     type = "person",  facts = { "works on Crucible" } },
            },
            { source_wikilink = "[[Sessions/2026-05-11-abc]]" },
            { kiln = kiln }
        )

        assert.equal(2, #summary.created)
        assert.equal(0, #summary.merged)
        assert.equal(2, #state.create_log)

        -- Verify body shape on at least one
        local first = state.create_log[1]
        assert.truthy(first.body:find("## Facts", 1, true))
        assert.truthy(first.body:find("source: %[%[Sessions/"))
    end)

    it("merges new facts into existing notes without overwriting user content", function()
        local existing_body = [=[
# Jane

## Background
Some human-authored bio.

## Facts
- works on Crucible (source: [[Sessions/old]])
- prefers Rust
]=]
        local kiln, state = make_kiln_mock({
            ["Entities/Jane.md"] = {
                title = "Jane",
                body = existing_body,
                properties = {
                    type = "entity",
                    canonical_name = "Jane",
                    aliases = {},
                    entity_type = "person",
                },
            },
        })

        local summary = dedupe.process(
            { { name = "Jane", type = "person", facts = { "builds the parser" } } },
            { source_wikilink = "[[Sessions/2026-05-11-new]]" },
            { kiln = kiln }
        )

        assert.equal(0, #summary.created)
        assert.equal(1, #summary.merged)
        assert.equal(1, #state.create_log)

        local merged = state.create_log[1]
        assert.truthy(merged.overwrite)
        -- Existing facts preserved
        assert.truthy(merged.body:find("works on Crucible", 1, true))
        assert.truthy(merged.body:find("prefers Rust", 1, true))
        -- New fact appended with source
        assert.truthy(merged.body:find("builds the parser", 1, true))
        assert.truthy(merged.body:find("Sessions/2026-05-11-new", 1, true))
    end)

    it("skips facts that are verbatim duplicates of existing facts", function()
        local existing_body = [=[
# Jane

## Facts
- works on Crucible (source: [[Sessions/old]])
]=]
        local kiln, state = make_kiln_mock({
            ["Entities/Jane.md"] = {
                title = "Jane",
                body = existing_body,
                properties = {
                    type = "entity",
                    canonical_name = "Jane",
                    aliases = {},
                    entity_type = "person",
                },
            },
        })

        local summary = dedupe.process(
            { { name = "Jane", facts = { "works on Crucible", "writes Lua" } } },
            { source_wikilink = "[[Sessions/2026-05-11-new]]" },
            { kiln = kiln }
        )

        assert.equal(1, #summary.merged)
        local merged = state.create_log[1]
        -- "works on Crucible" should appear exactly once
        local count = 0
        for _ in merged.body:gmatch("works on Crucible") do count = count + 1 end
        assert.equal(1, count)
        -- "writes Lua" should be the new appended fact
        assert.truthy(merged.body:find("writes Lua", 1, true))
    end)

    it("skips entities with no facts", function()
        local kiln, state = make_kiln_mock({})
        local summary = dedupe.process(
            { { name = "Empty", facts = {} } },
            { source_wikilink = "[[Sessions/x]]" },
            { kiln = kiln }
        )

        assert.equal(0, #summary.created)
        assert.equal(1, #summary.skipped)
        assert.equal(0, #state.create_log)
    end)

    it("skips entities with empty/nil names", function()
        local kiln, state = make_kiln_mock({})
        local summary = dedupe.process(
            { { name = "", facts = { "fact" } } },
            {},
            { kiln = kiln }
        )

        assert.equal(0, #summary.created)
        assert.equal(1, #summary.skipped)
        assert.equal(0, #state.create_log)
    end)
end)

-- ─────────────────────────────────────────────────────────────────────────────
-- write helpers
-- ─────────────────────────────────────────────────────────────────────────────

describe("write.slugify", function()
    it("preserves alnum + spaces + hyphens + dots + underscores", function()
        assert.equal("Jane Doe-1.0_v2", write.slugify("Jane Doe-1.0_v2"))
    end)

    it("replaces colons and slashes with underscores", function()
        assert.equal("a_b_c", write.slugify("a:b/c"))
    end)

    it("returns 'untitled' for empty input", function()
        assert.equal("untitled", write.slugify(""))
        assert.equal("untitled", write.slugify("///..."))
    end)
end)

describe("write.write_digest", function()
    it("writes to Sessions/<date>-<prefix>.md with session-digest frontmatter", function()
        local kiln, state = make_kiln_mock({})
        local extraction = {
            digest = { summary = "Worked on auth.", topics = { "auth" }, decisions = {}, action_items = {} },
            entities = { { name = "auth" } },
        }
        local _, err = write.write_digest(
            extraction,
            { session_id = "abcdef0123456789", date = "2026-05-11" },
            { kiln = kiln }
        )

        assert.is_nil(err)
        assert.equal(1, #state.create_log)
        local call = state.create_log[1]
        assert.equal("Sessions/2026-05-11-abcdef01.md", call.path)
        assert.equal("session-digest", call.frontmatter.type)
        assert.equal("abcdef0123456789", call.frontmatter.session_id)
        assert.equal("workspace", call.frontmatter.scope)
    end)

    it("appends a -<n> suffix on collision", function()
        -- Seed an existing file at the un-suffixed path so the first
        -- attempt collides. The mock create_note doesn't reject
        -- duplicates by default — override it to mimic the real
        -- kiln's overwrite=false behaviour.
        local kiln, state = make_kiln_mock({
            ["Sessions/2026-05-11-abcdef01.md"] = { body = "old" },
        })
        local orig_create = kiln.create_note
        kiln.create_note = function(opts)
            if state.notes[opts.path] and not opts.overwrite then
                return nil, "file exists at " .. opts.path
            end
            return orig_create(opts)
        end

        local extraction = {
            digest = { summary = "", topics = {}, decisions = {}, action_items = {} },
            entities = {},
        }
        local abs, err = write.write_digest(
            extraction,
            { session_id = "abcdef0123456789", date = "2026-05-11" },
            { kiln = kiln }
        )

        assert.is_nil(err)
        assert.truthy(abs)
        -- The successful write should be at -1
        local last = state.create_log[#state.create_log]
        assert.equal("Sessions/2026-05-11-abcdef01-1.md", last.path)
    end)

    it("propagates non-collision errors instead of looping", function()
        local kiln = {
            create_note = function(_) return nil, "permission denied" end,
        }
        local extraction = {
            digest = { summary = "", topics = {}, decisions = {}, action_items = {} },
            entities = {},
        }
        local abs, err = write.write_digest(
            extraction,
            { session_id = "abc", date = "2026-05-11" },
            { kiln = kiln }
        )
        assert.is_nil(abs)
        assert.truthy(err:find("permission denied", 1, true))
    end)
end)
