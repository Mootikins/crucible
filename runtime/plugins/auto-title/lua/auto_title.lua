--- Pure title logic for the auto-title plugin.
---
--- Named `auto_title`, not `title`: one Lua VM serves every plugin and
--- `package.loaded` is shared, so a generic module name is a collision waiting
--- for the next plugin that picks it (this repo has already had two plugins
--- ship a module called `config`).
---
--- Everything here is a string-in/string-out function, so the tests exercise
--- the code the plugin runs rather than a copy. Nothing in this file calls the
--- daemon; `init.lua` owns the one completion call.
---
--- Ported from `crucible-daemon/src/provider/title.rs`, which this plugin
--- replaces. The rules are the same rules — the point of the port is that the
--- prompt, the clip and the sanitizer stop being compiled in, not that titles
--- start looking different.

local M = {}

--- What the model is asked to do. The one product opinion this plugin exists
--- to make editable: override it with
--- `require("auto-title").setup{ prompt = "..." }` or `[plugins.auto-title]
--- prompt = "..."`.
M.SYSTEM_PROMPT = "You name conversations. Given the opening exchange of a "
  .. "session, reply with a short descriptive title (3 to 7 words) capturing its topic. "
  .. "Reply with the title only - no quotes, no trailing punctuation, no explanations."

--- Max characters of each message fed to the title prompt.
M.CLIP = 1500

--- Longest title kept whole. Longer ones are cut and get an ellipsis.
M.MAX_TITLE = 80

--- First `max` codepoints of `text`.
---
--- Codepoints, not bytes: a clip in the middle of a multi-byte character
--- produces invalid UTF-8, which some providers reject outright.
function M.clip(text, max)
  if type(text) ~= "string" then return "" end
  max = max or M.CLIP
  if utf8.len(text) == nil then
    -- Not valid UTF-8 to begin with. Clip by bytes rather than raising: the
    -- text came from a transcript, and a title is never worth an error.
    return text:sub(1, max)
  end
  if utf8.len(text) <= max then return text end
  return text:sub(1, utf8.offset(text, max + 1) - 1)
end

--- Render the opening exchange as the user turn of the title prompt.
---
--- An absent or empty assistant reply is left out entirely, rather than
--- appearing as an empty `Assistant:` line the model then tries to explain.
function M.exchange(user, assistant, clip)
  clip = clip or M.CLIP
  local out = "User: " .. M.clip(user, clip)
  if type(assistant) == "string" and assistant ~= "" then
    out = out .. "\n\nAssistant: " .. M.clip(assistant, clip)
  end
  return out
end

--- Characters a model wraps a title in when it ignores "no quotes".
local QUOTES = { ['"'] = true, ["'"] = true, ["`"] = true, ["\u{201c}"] = true, ["\u{201d}"] = true }

--- Strip quote characters from both ends, repeatedly.
local function trim_quotes(s)
  local changed = true
  while changed and s ~= "" do
    changed = false
    local first = s:match("^" .. utf8.charpattern)
    if first and QUOTES[first] then
      s = s:sub(#first + 1)
      changed = true
    end
    local last = s:match(utf8.charpattern .. "$")
    if last and QUOTES[last] then
      s = s:sub(1, #s - #last)
      changed = true
    end
  end
  return s
end

--- Normalize a model's reply into a title: first non-empty line, quotes and
--- `Title:` scaffolding stripped, whitespace collapsed, length-capped.
---
--- Every rule here answers something a model actually did. The scaffolding
--- strip is why `Title: Session archiving sweep` does not become a title
--- beginning with the word "Title".
function M.sanitize(raw)
  if type(raw) ~= "string" then return "" end

  local line = ""
  for candidate in (raw .. "\n"):gmatch("([^\n]*)\n") do
    local trimmed = candidate:match("^%s*(.-)%s*$")
    if trimmed ~= "" then
      line = trimmed
      break
    end
  end

  local title = trim_quotes(line):match("^%s*(.-)%s*$")
  for _, prefix in ipairs({ "Title:", "title:", "TITLE:" }) do
    if title:sub(1, #prefix) == prefix then
      title = title:sub(#prefix + 1):match("^%s*(.-)%s*$")
    end
  end

  while title:sub(-1) == "." do
    title = title:sub(1, -2)
  end

  -- Collapse every run of whitespace, newlines included, to one space.
  title = table.concat((function()
    local words = {}
    for word in title:gmatch("%S+") do words[#words + 1] = word end
    return words
  end)(), " ")

  if (utf8.len(title) or #title) > M.MAX_TITLE then
    local cut = M.clip(title, M.MAX_TITLE - 3):gsub("%s+$", "")
    return cut .. "..."
  end
  return title
end

return M
