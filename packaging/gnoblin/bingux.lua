-- Bingux integration defaults for Gnoblin.
-- Load from init.lua with:
--   local g = require("gnoblin")
--   g.load("/usr/share/gnoblin/conf.d/*.lua")
-- Source installs use their installed share/gnoblin/conf.d path instead.

-- Window open and close follow macOS: a slight scale from the centre with a
-- quick fade. Minimize and restore keep Gnoblin's dock-aware presets, which
-- travel to and from the dock icon.
gnoblin.animation({
    name = "bingux-open",
    event = "open",
    duration = 280,
    origin = "center",
    ease = "ease-out-expo",
    keyframes = {
        { at = 0, scale = 0.9, opacity = 0 },
        { at = 0.45, opacity = 1 },
        { at = 1, scale = 1, opacity = 1 },
    },
})

gnoblin.animation({
    name = "bingux-close",
    event = "close",
    duration = 180,
    origin = "center",
    ease = "ease-out-quad",
    from = { scale = 1, opacity = 1 },
    to = { scale = 0.94, opacity = 0 },
})

return {
    window_management = {
        -- Drag a window to a screen edge to tile it, as in GNOME.
        edge_tiling = true,
    },
    autostart = {
        -- The wallpaper is a tiny separate process listed first, so it maps before the shell loads.
        ["bingux-background"] = { command = { "bingux-wallpaper" } },
        bingux = { command = { "bingux", "--no-color" } },
        -- Search, the window switcher, capture and emoji share one process that
        -- starts after the dock is mapped. It is a session child, tied to the
        -- Gnoblin configuration and stopped when this module is removed.
        ["bingux-popouts"] = { command = { "bingux-popouts" } },
    },
    -- Bingux owns Alt+Tab and Super+Tab in its switcher. Clear Mutter's
    -- defaults so these accelerators can be registered by the shell.
    keybindings = {
        wm = {
            switch_applications = {},
            switch_applications_backward = {},
            switch_windows = {},
            switch_windows_backward = {},
        },
    },
    ["frame-renderers"] = {
        -- The installer rewrites this packaged path for its selected prefix.
        -- GDM does not promise the user's command-link directory on PATH.
        bingux = { "/usr/local/libexec/bingux/bingux-frame", "--compact" },
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
            -- Keep client-drawn decorations intact. A compositor frame is
            -- supplied only when the client explicitly requests one.
            match = { type = "window" },
            animation = {
                open = "bingux-open",
                close = "bingux-close",
                ["dialog-open"] = "gnome-dialog-open",
                ["dialog-close"] = "gnome-dialog-close",
                minimize = "gnome-minimize",
                restore = "gnome-restore",
                ["workspace-switch"] = "gnome-workspace-switch",
                resize = "gnome-resize",
            },
            frame = {
                mode = "auto",
                renderer = "bingux",
                extents = { 36, 0, 0, 0 },
            },
            corners = {
                mode = "force",
                radius = 12,
                smoothing = 0.5,
                border_width = 1,
                border_color = "#505050bf",
                -- Reconstruct only detected client-drawn rounded corners
                -- before applying Bingux's outer shape.
                ["remove-csd"] = true,
                -- Fullscreen and maximised windows skip the rounded-clip pass,
                -- so video can be scanned out directly and does not drop frames.
                ["keep-maximized"] = false,
                ["keep-fullscreen"] = false,
                ["keep-tiled"] = true,
                ["shadow-animation"] = { duration = 180, easing = "ease-out-cubic" },
                shadow = {
                    { x = 0, y = 8, blur = 42, spread = 4, opacity = 0.22 },
                    { x = 0, y = 3, blur = 14, spread = 1, opacity = 0.18 },
                    { x = 0, y = 1, blur = 4, spread = 0, opacity = 0.22 },
                },
            },
        },
        {
            match = { type = "window", focused = true },
            corners = {
                shadow = {
                    { x = 0, y = 18, blur = 64, spread = 8, opacity = 0.38 },
                    { x = 0, y = 8, blur = 24, spread = 2, opacity = 0.26 },
                    { x = 0, y = 2, blur = 6, spread = 0, opacity = 0.32 },
                },
            },
        },
        -- Bingux layer surfaces publish their own ext-background-effect
        -- regions. Do not add rule blur to them: the rule blurs the whole
        -- layer buffer whenever the client clears its region, which blurs
        -- the screen behind hidden full-screen popups and the wallpaper.
        {
            match = {
                layer = "^(bingux-top-bar|bingux-dock|bingux-search|gnoblin-shell-popup)$",
            },
            opacity = 0.9,
        },
        {
            match = {
                layer = "^(bingux-top-bar|bingux-terminal-sidebar|bingux-sidebar-corner|bingux-panel-outline|bingux-dock|bingux-search|gnoblin-shell-popup|bingux-bar-tooltip|gnoblin-dock-tooltip|bingux-notifications|bingux-capture-feedback|bingux-capture-controls)$",
            },
            opacity = 1.0,
        },
        {
            match = {
                layer = "^(bingux-[a-z0-9-]+|gnoblin-(shell-popup|dock-tooltip))$",
            },
            -- Bingux owns these surface transitions. Gnoblin's registered
            -- animation system can preview layer-shell motion with
            -- `gnoblinctl animation preview NAME --namespace NAME`, but keep
            -- compositor motion disabled here to avoid running both systems.
            -- Remove or narrow this rule explicitly if a panel opts in to a
            -- named `layer-open` / `layer-close` compositor animation.
            animation = "none",
        },
        {
            match = {
                layer = "^(bingux-terminal-sidebar|bingux-sidebar-corner)$",
            },
            opacity = 1.0,
        },
        {
            match = {
                layer = "^(bingux-bar-tooltip|gnoblin-dock-tooltip|gnoblin-shell-popup)$",
            },
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
            opacity = 1.0,
        },
    },
}
