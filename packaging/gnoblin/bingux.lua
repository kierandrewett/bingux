-- Bingux integration defaults for Gnoblin.
-- Load from init.lua with:
--   local g = require("gnoblin")
--   g.load("/usr/share/gnoblin/conf.d/*.lua")
-- Source installs use their installed share/gnoblin/conf.d path instead.

return {
    shell = {
        -- Bingux renders the OSD surface and consumes Gnoblin's OSD records.
        osd = false,
    },
    protocols = {
        -- These protocols are used by Bingux panels and capture/search surfaces.
        ["wlr-layer-shell"] = true,
        ["wlr-screencopy"] = true,
        ["ext-foreign-toplevel-list"] = true,
        ["wlr-foreign-toplevel-management"] = true,
        ["ext-data-control"] = true,
    },
    ["window-rules"] = {
        {
            match = {
                layer = "^(bingux-[a-z0-9-]+|gnoblin-(shell-popup|dock-tooltip))$",
            },
            -- Bingux owns this surface motion. Avoid two animations.
            animation = "none",
        },
    },
}
