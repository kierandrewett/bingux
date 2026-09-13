-- Bingux integration defaults for Gnoblin.
-- Load from init.lua with:
--   local g = require("gnoblin")
--   g.load("/usr/share/gnoblin/conf.d/*.lua")
-- Source installs use their installed share/gnoblin/conf.d path instead.

return {
    shell = {
        -- Bingux renders the OSD surface and consumes Gnoblin's OSD records.
        osd = false,
        ["window-menu"] = { "binguxctl", "ipc", "shell", "windowMenu" },
    },
    protocols = {
        -- These protocols are used by Bingux panels and capture/search surfaces.
        ["wlr-layer-shell"] = true,
        ["wlr-screencopy"] = true,
        ["ext-foreign-toplevel-list"] = true,
        ["wlr-foreign-toplevel-management"] = true,
        ["ext-data-control"] = true,
        ["ext-background-effect-v1"] = true,
    },
    ["window-rules"] = {
        {
            match = {
                layer = "^(bingux-[a-z0-9-]+|gnoblin-(shell-popup|dock-tooltip))$",
            },
            -- Bingux owns this surface motion. Avoid two animations.
            animation = "none",
        },
        {
            match = {
                layer = "^(bingux-terminal-sidebar|bingux-sidebar-corner)$",
            },
            -- Keep the sidebar and its rounded corner on the same blur path.
            ["blur-ignore-shadows"] = true,
            blur = 24,
            opacity = 1.0,
        },
        {
            match = {
                layer = "^(bingux-bar-tooltip|gnoblin-dock-tooltip|gnoblin-shell-popup)$",
            },
            -- Tooltips and the small calendar/control-centre popups are
            -- translucent materials; keep their compositor blur enabled even
            -- when the user's config has no per-surface rule yet.
            ["blur-ignore-shadows"] = true,
            blur = 24,
            opacity = 1.0,
        },
        {
            match = { layer = "^bingux-capture$" },
            -- The preview must remain pixel-aligned with the captured screen.
            blur = 0,
            opacity = 1.0,
        },
        {
            match = { layer = "^bingux-capture-controls$" },
            -- Blur only the toolbar/settings region published by BlurRegion.
            ["blur-ignore-shadows"] = true,
            blur = 24,
            opacity = 1.0,
        },
    },
}
