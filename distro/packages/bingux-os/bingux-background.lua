-- Seeded from /etc/skel for new accounts. `bingux-session export` replaces this
-- file with one that names the account's own blurred wallpaper at its first
-- session. Until then the first compositor frame is the packaged default's
-- blurred twin, not Gnoblin's default colour.
gnoblin.configure {
    compositor = {
        background = {
            color = "#161616",
            image = "/usr/share/backgrounds/bingux-os/default-blur.jpg",
        },
    },
}
