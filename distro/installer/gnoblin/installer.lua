-- Bingux OS live installer session: Gnoblin with the greeter's defaults, the
-- greeter autostart switched off, the user's wallpaper drop-ins, and the
-- installer UI started in the greeter's place. bingux-live-session runs this
-- file with `gnoblin --config`; the packaged path is rewritten only when the
-- installer is installed under a different prefix.

gnoblin.load("/usr/share/bingux/greeter/gnoblin.lua")
gnoblin.configure.autostart["bingux-greeter"] = { name = "bingux-greeter", enable = false }

-- The same wallpaper and blur drop-ins the live desktop loads, so the installer
-- card sits on the picture the greeter showed.
local home = os.getenv("XDG_CONFIG_HOME") or ((os.getenv("HOME") or "/") .. "/.config")
gnoblin.load(home .. "/gnoblin/conf.d/**/*.lua")

gnoblin.configure.autostart["bingux-installer-ui"] = {
    name = "bingux-installer-ui",
    command = { "/usr/bin/bingux-installer-ui" },
}
