-- Bingux OS: a user session is Gnoblin with the Bingux shell. Installed and
-- personal drop-ins live in conf.d/; Bingux keeps bingux-background.lua there.
local g = require("gnoblin")
g.load("/usr/share/gnoblin/conf.d/*.lua")
g.load("conf.d/**/*.lua")
