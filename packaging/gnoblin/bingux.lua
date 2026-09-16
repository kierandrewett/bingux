-- Bingux integration defaults for Gnoblin.
-- Load from init.lua with:
--   local g = require("gnoblin")
--   g.load("/usr/share/gnoblin/conf.d/*.lua")
-- Source installs use their installed share/gnoblin/conf.d path instead.

return {
    ["frame-renderers"] = {
        -- Bingux's GTK/libadwaita renderer supplies the actual SSD titlebar.
        -- The installer rewrites this placeholder to the selected prefix.
        bingux = { "/usr/local/libexec/bingux/bingux-frame", "--compact" },
    },
    shortcuts = {
        {
            name = "bingux-emoji",
            binding = "<Super>period",
            command = { "binguxctl", "emoji", "open" },
        },
    },
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
            -- Give every ordinary application the same outer treatment. The
            -- compositor also reconstructs client-side decoration corners so
            -- CSD and Bingux's server frame have the same silhouette.
            match = { type = "window" },
            frame = {
                mode = "auto",
                renderer = "bingux",
                extents = { 36, 0, 0, 0 },
            },
            corners = {
                mode = "force",
                radius = 12,
                smoothing = 0.5,
                ["shadow-animation"] = { duration = 180, easing = "ease-out-cubic" },
                shadow = {
                    { x = 0, y = 8, blur = 24, spread = 0, opacity = 0.14 },
                    { x = 0, y = 2, blur = 5, spread = 0, opacity = 0.18 },
                },
            },
            borders = {
                ["inner-width"] = 1,
                ["inner-color"] = "#505050bf",
                ["outer-width"] = 1,
                ["outer-color"] = "#00000080",
            },
        },
        {
            match = { type = "window", focused = true },
            corners = {
                shadow = {
                    { x = 0, y = 10, blur = 36, spread = 0, opacity = 0.22 },
                    { x = 0, y = 2, blur = 5, spread = 0, opacity = 0.28 },
                },
            },
        },
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
