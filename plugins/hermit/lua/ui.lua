--- Hermit Oil UI views
--- Splash screen and digest viewer

local awareness = require("awareness")
local config = require("config")
local digest_mod = require("digest")

local M = {}
local oil = cru.oil

-- Helper: styled key-value row
local function kv_row(label, value, opts)
    opts = opts or {}
    return oil.row({ gap = 2 },
        oil.text(label .. ":", { fg = opts.label_color or "cyan" }),
        oil.text(tostring(value), opts.value_style or {})
    )
end

--- Splash view — shown on session start or /hermit status
-- @view name="hermit-splash" desc="Hermit kiln overview"
function M.splash_view(ctx)
    local profile = awareness.get()

    local tag_text = ""
    if profile.tags and #profile.tags > 0 then
        local names = {}
        for i = 1, math.min(5, #profile.tags) do
            table.insert(names, profile.tags[i].tag)
        end
        tag_text = table.concat(names, ", ")
    end

    local orphan_style = {}
    if profile.orphan_count > 0 then
        orphan_style = { fg = "yellow" }
    else
        orphan_style = { fg = "green" }
    end

    return oil.col({ border = "rounded", padding = 1, gap = 1 },
        oil.row({ gap = 2 },
            oil.text("hermit", { bold = true, fg = "cyan" }),
            oil.text("v0.1.0", { fg = "gray", dim = true })
        ),
        oil.text("knowledge daemon", { fg = "gray", italic = true }),
        oil.hr(),
        kv_row("Notes", tostring(profile.note_count)),
        kv_row("Orphans", tostring(profile.orphan_count), { value_style = orphan_style }),
        kv_row("Tags", tag_text),
        kv_row("Last scan", profile.refreshed_at or "never"),
        oil.hr(),
        oil.text("q close  r refresh  d digest", { fg = "gray", dim = true })
    )
end

--- Keyboard handler for splash view
-- @view.handler name="hermit-splash"
function M.splash_handler(key, ctx)
    if key == "q" then
        ctx:close_view()
    elseif key == "r" then
        awareness.invalidate()
        awareness.refresh(true)
        ctx:refresh()
    elseif key == "d" then
        local profile = awareness.get()
        local result = digest_mod.format(profile, 1)
        ctx:open_view("hermit-digest", { digest = result })
    end
end

--- Digest view — renders daily digest with styled sections
-- @view name="hermit-digest" desc="Hermit daily digest"
function M.digest_view(ctx)
    local state = ctx.state or {}
    local result = state.digest

    if not result then
        local profile = awareness.get()
        result = digest_mod.format(profile, 1)
    end

    local children = {
        oil.text("Kiln Digest", { bold = true, fg = "cyan" }),
        oil.text(os.date("%Y-%m-%d %H:%M"), { fg = "gray", dim = true }),
        oil.hr(),
    }

    -- Overview section
    table.insert(children, oil.text("Overview", { bold = true }))
    table.insert(children, kv_row("Notes", tostring(result.note_count)))
    table.insert(children, kv_row("Orphans", tostring(result.orphan_count)))
    table.insert(children, kv_row("Tags tracked", tostring(result.tag_count)))
    table.insert(children, oil.text(""))

    -- Render the markdown content as plain text lines
    if result.markdown then
        for line in result.markdown:gmatch("[^\n]+") do
            -- Skip the title and generated date (already rendered above)
            if not line:match("^# Kiln Digest") and
               not line:match("^Generated:") and
               not line:match("^## Overview") and
               not line:match("^$") then
                -- Style headers
                local header = line:match("^## (.+)")
                if header then
                    table.insert(children, oil.text(""))
                    table.insert(children, oil.text(header, { bold = true, fg = "yellow" }))
                else
                    -- Style list items
                    local item = line:match("^%- (.+)")
                    if item then
                        table.insert(children, oil.text("  " .. item, {}))
                    else
                        table.insert(children, oil.text(line))
                    end
                end
            end
        end
    end

    table.insert(children, oil.text(""))
    table.insert(children, oil.hr())
    table.insert(children, oil.text("q close  r refresh", { fg = "gray", dim = true }))

    return oil.col({ border = "rounded", padding = 1, gap = 0 }, unpack(children))
end

--- Keyboard handler for digest view
-- @view.handler name="hermit-digest"
function M.digest_handler(key, ctx)
    if key == "q" then
        ctx:close_view()
    elseif key == "r" then
        awareness.invalidate()
        local profile = awareness.refresh(true)
        local result = digest_mod.format(profile, 1)
        ctx.state.digest = result
        ctx:refresh()
    end
end

return M
