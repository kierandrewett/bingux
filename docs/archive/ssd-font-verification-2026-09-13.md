# SSD title rasterization

The former `gsk_render_node_draw()` path rasterized GTK snapshots through Cairo.
The fix uses the rooted GTK window's own GSK renderer and downloads its texture
into the existing premultiplied ARGB32 buffer. Font, widget layout, geometry,
protocol, borders and compositor configuration are unchanged.

## Reproduction and verification

Nested Gnoblin session: `/tmp/gnoblin-user-config.NZ64us`, started with Gnoblin's
`GNOME_DEVKIT_HEADLESS=1 bash scripts/run-normal-config-devkit.sh`. This copied
the user's normal Gnoblin/Bingux/GTK/dconf configuration. Only the copied
renderer executable path was changed to the test build. Stage resource scale: 1.

The regression test captures the real painter's snapshot and independently
renders the same node with GTK's normal GSK renderer. Before the fix, 685 of
5,520 title pixels differed by more than 8/255. After the fix, every RGBA byte
in that title region matches, at widths 521/522/523, focused and unfocused.
The comparison selected `GskGLRenderer`; explicitly selecting Cairo also passes,
so the implementation respects GTK's software-renderer fallback.

Run from the Gnoblin checkout while the private session is active:

```sh
python3 tests/nested-desktop-check.py /tmp/gnoblin-user-config.NZ64us --run bash /home/kieran/dev/bingux/packages/bingux-frame/test-title-rendering.sh /tmp/gnoblin-user-config.NZ64us/font-tests
python3 tests/nested-ssd-check.py /tmp/gnoblin-user-config.NZ64us
python3 tests/nested-desktop-check.py /tmp/gnoblin-user-config.NZ64us
```

The SSD input test passed 30 focus changes with zero native-fallback visibility
flashes, retained external presentation, and right-click menu activation of
Minimize on the original window. The CSD pixel/negotiation checks passed across
three script/config reloads. The full desktop screenshot was visually inspected.

The configured `/home/kieran/dev/bingux/build/bingux-frame` was rebuilt and
compared byte-for-byte with the tested private binary. No live host config or
compositor was changed; the host was GNOME, with no `org.gnoblin.Shell` owner.
The next Gnoblin session uses the rebuilt binary. The private session was stopped
after testing. Mixed-DPI transport remains a separate limitation, not covered by
the 1x title test.

Proof files are in `/home/kieran/.local/state/gnoblin/proofs/2026-09-13-ssd-font/`:
`title-actual.png`, `title-gtk-reference.png`, `title-old-cairo-path.png`,
`bingux-ssd.png`, and `bingux-menu.png`.
