-- Bingux OS live installer session: Gnoblin with the greeter's defaults, the
-- greeter's own clients switched off, the user's wallpaper drop-ins, and the
-- installer UI started in the greeter's place. bingux-live-session runs this
-- file with `gnoblin --config`; the packaged path is rewritten only when the
-- installer is installed under a different prefix.
--
-- Gnoblin's Lua has no os or io library. If this file fails to load, Gnoblin
-- runs its built-in defaults: a black screen, an error panel and no installer.
-- tests/installer-gnoblin-config.py loads it through Gnoblin's own loader.

gnoblin.load("/usr/share/bingux/greeter/gnoblin.lua")
gnoblin.configure.autostart["bingux-greeter"] = { name = "bingux-greeter", enable = false }
-- The live account's user manager already runs bingux-statusd (bingux.target).
-- A second one exits on the busy socket, and Gnoblin shows that as an error.
gnoblin.configure.autostart["bingux-statusd"] = { name = "bingux-statusd", enable = false }

-- The same wallpaper and blur drop-ins the live desktop loads, so the installer
-- card sits on the picture the greeter showed. `~/` is the account's home.
gnoblin.load("~/.config/gnoblin/conf.d/**/*.lua")

gnoblin.configure.autostart["bingux-installer-ui"] = {
    name = "bingux-installer-ui",
    command = { "/usr/bin/bingux-installer-ui" },
}
