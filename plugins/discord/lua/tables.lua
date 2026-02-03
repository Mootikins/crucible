--- Markdown table → box-drawing table transformer
--- Matches Crucible TUI rendering style (single-line box chars).
--- Used to wrap tables in code blocks for Discord, which has no native table support.

local M = {}

-- Box-drawing characters matching crucible-cli TUI renderer
local BOX = {
    top_left     = "┌",
    top_right    = "┐",
    bottom_left  = "└",
    bottom_right = "┘",
    horizontal   = "─",
    vertical     = "│",
    top_t        = "┬",
    bottom_t     = "┴",
    left_t       = "├",
    right_t      = "┤",
    cross        = "┼",
}

--- Parse a single markdown table from lines.
--- Returns {headers = {str...}, rows = {{str...}...}} or nil.
local function parse_table(lines)
    if #lines < 2 then return nil end

    local function split_row(line)
        line = line:match("^%s*|?(.-)%s*|?%s*$") or line
        local cells = {}
        for cell in (line .. "|"):gmatch("(.-)%s*|") do
            table.insert(cells, cell:match("^%s*(.-)%s*$") or cell)
        end
        return cells
    end

    local function is_separator(line)
        return line:match("^[%s|:%-]+$") ~= nil and line:find("%-%-") ~= nil
    end

    local headers = split_row(lines[1])
    if #headers == 0 then return nil end

    if not is_separator(lines[2]) then return nil end

    local rows = {}
    for i = 3, #lines do
        local cells = split_row(lines[i])
        while #cells < #headers do
            table.insert(cells, "")
        end
        table.insert(rows, cells)
    end

    return { headers = headers, rows = rows }
end

--- Render a parsed table using box-drawing characters.
local function render_table(tbl)
    local num_cols = #tbl.headers
    if num_cols == 0 then return "" end

    local col_widths = {}
    for i = 1, num_cols do
        col_widths[i] = math.max(3, #tbl.headers[i])
    end
    for _, row in ipairs(tbl.rows) do
        for i = 1, num_cols do
            local cell = row[i] or ""
            col_widths[i] = math.max(col_widths[i], #cell)
        end
    end

    local function border(left, mid, right)
        local parts = { left }
        for i = 1, num_cols do
            table.insert(parts, BOX.horizontal:rep(col_widths[i] + 2))
            if i < num_cols then
                table.insert(parts, mid)
            end
        end
        table.insert(parts, right)
        return table.concat(parts)
    end

    local function data_row(cells)
        local parts = { BOX.vertical }
        for i = 1, num_cols do
            local cell = cells[i] or ""
            local pad = col_widths[i] - #cell
            table.insert(parts, " " .. cell .. (" "):rep(pad) .. " " .. BOX.vertical)
        end
        return table.concat(parts)
    end

    local out = {}
    table.insert(out, border(BOX.top_left, BOX.top_t, BOX.top_right))
    table.insert(out, data_row(tbl.headers))
    table.insert(out, border(BOX.left_t, BOX.cross, BOX.right_t))
    for _, row in ipairs(tbl.rows) do
        table.insert(out, data_row(row))
    end
    table.insert(out, border(BOX.bottom_left, BOX.bottom_t, BOX.bottom_right))

    return table.concat(out, "\n")
end

--- Find and transform all markdown tables in text into box-drawing code blocks.
--- Non-table content passes through unchanged.
function M.transform(text)
    if not text or text == "" then return text end
    if not text:find("|") then return text end

    local lines = {}
    for line in (text .. "\n"):gmatch("(.-)\n") do
        table.insert(lines, line)
    end

    local out = {}
    local i = 1

    while i <= #lines do
        local line = lines[i]

        if line:find("|") and i + 1 <= #lines
            and lines[i + 1]:match("^[%s|:%-]+$")
            and lines[i + 1]:find("%-%-") then

            local table_lines = { line, lines[i + 1] }
            local j = i + 2
            while j <= #lines and lines[j]:find("|") do
                table.insert(table_lines, lines[j])
                j = j + 1
            end

            local parsed = parse_table(table_lines)
            if parsed then
                table.insert(out, "```")
                table.insert(out, render_table(parsed))
                table.insert(out, "```")
                i = j
            else
                table.insert(out, line)
                i = i + 1
            end
        else
            table.insert(out, line)
            i = i + 1
        end
    end

    local result = table.concat(out, "\n")
    if not text:match("\n$") then
        result = result:gsub("\n$", "")
    end
    return result
end

return M
