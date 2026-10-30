-- Gnoblin configuration for the Bingux login greeter session.
--
-- bingux-greeter-session passes this file to `gnoblin --greeter --config`.
-- The session runs as the greetd account before anyone logs in, so it starts
-- nothing but the greeter client and exposes nothing a user session needs.
-- The installer rewrites the packaged command path for its selected prefix.

local defaults = {
    autostart = {
        -- Status first: it publishes keyboard layouts for the greeter's bar.
        { name = "bingux-statusd", command = { "/usr/local/bin/bingux-statusd" } },
        { name = "bingux-greeter", command = { "/usr/local/bin/bingux-greeter" } },
    },
    protocols = {
        ["wlr-layer-shell"] = true,
        ["ext-background-effect-v1"] = true,
    },
    ["window-rules"] = {
        {
            match = { layer = "^bingux-greeter$" },
            opacity = 1.0,
            animation = "none",
        },
    },
}

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
