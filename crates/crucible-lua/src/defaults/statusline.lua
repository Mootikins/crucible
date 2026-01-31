-- Default Crucible statusline
--
-- Override by calling crucible.statusline.setup() in your init.lua.
-- See: cru config --default-statusline

crucible.statusline.setup({
    left = {
        crucible.statusline.mode(),
        crucible.statusline.model({ max_length = 25, fg = "cyan" }),
    },
    center = {},
    right = {
        crucible.statusline.notification({
            fg = "yellow",
            fallback = crucible.statusline.context({ fg = "gray" }),
        }),
    },
    separator = " ",
})
