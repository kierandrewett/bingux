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

local defaults = {
    window_management = {
        -- Drag a window to a screen edge to tile it, as in GNOME.
        edge_tiling = true,
        -- wl-copy from wl-clipboard 2.2.1 opens a small window and waits for
        -- keyboard focus before it sets the clipboard. With "prevent" that
        -- window never gets focus, so wl-copy does not exit. Gnoblin has no
        -- per-window focus grant, so this is the supported setting.
        focus_new_windows = "allow",
    },
    -- The media keys control the player the shell shows in the Control Centre:
    -- the one that is playing, else the first. `playerctl` picks the first
    -- player on the bus, which is often a browser tab that is not playing.
    shortcuts = {
        ["media-play-pause"] = { command = { "binguxctl", "media", "toggle" } },
        ["media-next"] = { command = { "binguxctl", "media", "next" } },
        ["media-previous"] = { command = { "binguxctl", "media", "previous" } },
    },
    autostart = {
        -- Gnoblin launches this ordered list in order. Keep wallpaper first so
        -- it can map while the desktop shell starts.
        { name = "bingux-background", command = { "bingux-wallpaper" } },
        { name = "bingux", command = { "bingux", "--no-color" } },
        -- Search, the window switcher, capture and emoji share one process. It
        -- is a session child, tied to the Gnoblin configuration and stopped
        -- when this module is removed.
        { name = "bingux-popouts", command = { "bingux-popouts" } },
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
    shortcuts = {
        -- Super+L locks the session. Gnoblin's Lua has no os or io library, so
        -- the choice is made when the key is pressed, from the environment
        -- Gnoblin runs in: a session hosted by bsm (BSM_SESSION_ID set by
        -- bsm-helper) asks bsm, which protects the session and runs the
        -- transition; otherwise this starts the standalone lock client, as the
        -- Control Centre Lock button does.
        ["bingux-lock"] = {
            binding = "<Super>l",
            command = {
                "sh",
                "-c",
                "if [ -n \"$BSM_SESSION_ID\" ]; then exec bsmctl lock; fi; exec bingux-lock",
            },
        },
    },
    ["frame-renderers"] = {
        -- The installer rewrites this packaged path for its selected prefix.
        -- GDM does not promise the user's command-link directory on PATH.
        bingux = { "/usr/local/libexec/bingux/bingux-frame", "--compact" },
    },
    -- Bingux serves Screenshot. It is tried first; Gnoblin's own backend
    -- and then any installed backend follow when Bingux is not installed.
    -- Other interfaces use Gnoblin's backend, then the first installed one.
    portals = {
        default = { "gnoblin", "*" },
        interfaces = {
            ["org.freedesktop.impl.portal.Screenshot"] = { "bingux", "gnoblin", "*" },
        },
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
                -- The edge ring and the shadows follow the colour scheme; see `edge_rules` below.
                -- Reconstruct only detected client-drawn rounded corners
                -- before applying Bingux's outer shape.
                ["remove-csd"] = true,
                -- Fullscreen and maximised windows skip the rounded-clip pass,
                -- so video can be scanned out directly and does not drop frames.
                ["keep-maximized"] = false,
                ["keep-fullscreen"] = false,
                ["keep-tiled"] = true,
                ["shadow-animation"] = { duration = 180, easing = "ease-out-cubic" },
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

-- Apply public setting names through the API, preserving earlier declarations.
local autostart = defaults.autostart
local window_rules = defaults["window-rules"]
defaults.autostart = nil
defaults["window-rules"] = nil
gnoblin.configure(defaults)
for _, entry in ipairs(autostart) do
    gnoblin.configure.autostart[entry.name] = entry
end
for _, rule in ipairs(window_rules) do
    gnoblin.window_rule(rule)
end

-- Window edge. The shell's menus draw a 1 px line on each side of their edge (Theme.qml:
-- `Theme.outline` inside, `Theme.panelOuterOutline` outside). A window gets the outside line only,
-- as the border rule with a negative width, which Gnoblin draws outward from the same bounds it
-- clips the window to, so it touches the edge all round. Two things were tried and failed on the live
-- session: a shadow layer with spread 1 as the ring (it sat 2 px away from the window on the left
-- and top and left a gap of desktop showing between ring and window), and a positive width (an inner
-- line in the window's own pixels). The soft shadows follow. Gnoblin replaces a shadow list as a
-- whole, so each scheme states the full list for unfocused and focused windows.
local edges = {
    dark = {
        ring = "#00000080",
        shadow = {
            { x = 0, y = 10, blur = 40, spread = 0, opacity = 0.16 },
            { x = 0, y = 3, blur = 12, spread = 0, opacity = 0.10 },
        },
        focused = {
            { x = 0, y = 18, blur = 56, spread = 0, opacity = 0.28 },
            { x = 0, y = 6, blur = 18, spread = 0, opacity = 0.16 },
        },
    },
    light = {
        ring = "#00000038",
        shadow = {
            { x = 0, y = 10, blur = 40, spread = 0, opacity = 0.10 },
            { x = 0, y = 3, blur = 12, spread = 0, opacity = 0.06 },
        },
        focused = {
            { x = 0, y = 18, blur = 56, spread = 0, opacity = 0.20 },
            { x = 0, y = 6, blur = 18, spread = 0, opacity = 0.12 },
        },
    },
}

local function edge_rules(scheme)
    local edge = scheme == "prefer-light" and edges.light or scheme == "default" and edges.light or edges.dark
    return {
        {
            match = { type = "window" },
            corners = { border_width = -1, border_color = edge.ring, shadow = edge.shadow },
        },
        { match = { type = "window", focused = true }, corners = { shadow = edge.focused } },
    }
end

-- `gnoblin.snapshot().window_rules` is empty inside the event handler, so the list is
-- rebuilt from this module's own rules. Another file's rules would be dropped by that; the
-- only one today is the minimal native frame rule of the default init.lua, which the
-- frame rule above overrides for every window anyway.
local function apply_edges(scheme)
    local rules = {}
    for _, rule in ipairs(window_rules) do
        rules[#rules + 1] = rule
    end
    for _, rule in ipairs(edge_rules(scheme)) do
        rules[#rules + 1] = rule
    end
    gnoblin.configure({ window_rules = rules })
end

-- The first edge rules are plain rules; the list is replaced only when the scheme changes.
for _, rule in ipairs(edge_rules(gnoblin.appearance.color_scheme())) do
    gnoblin.window_rule(rule)
end
gnoblin.events.on("gnoblin.appearance.color-scheme-changed", function(event)
    apply_edges(event.color_scheme)
end)
