# Desktop customisation and search providers

Open **Bingux Settings > Desktop > Customise Desktop**. This opens a full-screen layer-shell editor on the Settings window's screen. It shows the current wallpaper behind the existing top bar, dock, sidebar and control centre. These containers keep their actual widget instances and actions.

Drag a widget from the centre palette into the left, centre or right section of the top bar, or into the dock. A widget already on the desktop moves instead of being duplicated. Sidebar panel widgets can move within the sidebar. Drag a widget back into the palette to remove it. Keep at least one sidebar panel.

The open centre palette has Widgets and Apps views. Drag an installed app into the dock to pin it. The dock preview uses the saved pins and order. The control centre stays open beside the palette; drag its tiles to change their order or drag one into the palette to remove it.

The palette uses shared widget components with sample content. It does not continuously capture live widgets. The item you pick up follows the cursor, and a line marks its insertion position. Fixed Space and Flexible space items can be added more than once to the top bar. Flexible spaces share the available width in their section. Fixed spaces have an adjustable width from 8 to 160 pixels.

Label and Icon tiles create independent items in the top bar or dock. Edit their text or choose a symbol from the icon grid. They follow the container display mode unless you set an override. Removing an instance clears its options; Undo restores both the item and its appearance.

A popped-out sidebar uses its saved desktop edge while the editor is open. The
editor moves the existing content into that edge surface. Done and Cancel both
restore the floating window without changing its content or popped-out state.

On Gnoblin, the editor stays above fullscreen apps and below its real containers.
Closing it restores their previous stacking order without changing the app's
fullscreen state.

Click a container to open its settings beside it. Right-click a placed widget, double-click its palette preview, or focus the preview and press Enter to open widget options. Move and Remove are in those options. The icon grid and dock pointer settings expand on request. Undo and Redo reverse complete drops and individual settings changes.

Changes stay in the preview until **Done**. Cancel discards the preview. Escape closes the current options panel first, then the editor. Restore defaults resets the top-bar and sidebar widget arrangement in the preview. Existing settings changes are also saved when applying the desktop.

The live shell uses the same widget instances after reparenting. Menus follow their controls, including controls moved to the dock. Crowded top-bar sections move controls into the existing More menu. Live top-bar reordering remains persistent after using the editor.

## Search providers

**Settings > Search** lists website search engines and built-in providers. Use Add to create a website engine with a name, unique shortcut and URL containing `{query}`. `%s` is also accepted by the form. The search terms are URL-encoded before substitution. Bingux opens the result through its existing browser launcher.

An enabled engine can be made the default. The current default cannot be disabled or removed until another engine is selected. Custom shortcuts use `shortcut: search terms`. The built-in `wiki:`, `gh:`, `maps:` and `yt:` shortcuts remain reserved.

Built-in provider rows have independent enable switches and detail pages. Files and folders accepts absolute search locations separated by semicolons. An empty field uses the system-configured locations. Other detail pages describe their triggers and provide the relevant controls. Connected process providers remain managed by system configuration; the Add form creates website search engines.

Press Apply to save search changes. The settings helper restarts the user search service when its search configuration changes. A restart failure is reported after the file is saved.

## Persistence and validation

User overrides are stored atomically in `$XDG_CONFIG_HOME/bingux/settings.json`. They remain separate from package-managed files. The settings backend validates engine URLs, unique shortcuts, enabled defaults, desktop ranges, widget destinations and duplicate widgets before replacing the file. The search daemon validates engine configuration again when loading it.

The shell imports the effective runtime layout once into version 1 of the desktop settings. This includes top-bar and overflow order, dock pins and app order, sidebar edge, and control-centre choices. The source state is retained in `layout-before-import.json`. Spacing instances use unique IDs such as `spring:1` and `spacer:1`; fixed widths are stored in `desktop.widgetOptions["spacer:1"].width`. Existing imported arrangements do not gain spacing items automatically. Later launches and the editor load the saved layout. Unsupported versions are rejected. JPEG XL and other wallpapers that Qt cannot read directly are converted to a cached PNG by the settings helper.

## Checks

- `python3 tests/settings-backend.test.py`
- `node --test tests/desktop-layout.test.mjs`
- `cargo test --locked --manifest-path packages/bingux-searchd/Cargo.toml`
- `python3 tests/search-engines.py` after building the search daemon; `BINGUX_SEARCHD_BIN` selects a different build.
- `bash tests/bingux-settings.sh` with `BINGUX_QUICKSHELL` set to a compatible Quickshell runtime.
- In a private Gnoblin test session: `tests/desktop-customise.py`, `tests/desktop-layout-live.py`, `BINGUX_SHELL_TEST=dock-behaviour tests/dock-state.py`, and `tests/popup-anchor.py`.
- `tests/customise-reload-live.py` repeats native widget drags and UI reloads. It checks the process ID, saved layout, editor reopening and IPC after each reload.
- `tests/customise-fullscreen-live.py` checks compositor stacking, native palette input and cancellation over a fullscreen app with the compositor bridge connected.
- `tests/quickshell-no-dumps-live.py` deliberately crashes a private shell and checks that it writes neither a kernel core file nor a Quickshell minidump.

The full-shell layout fixture ends its private process group after its assertions complete. It does not change the user's settings or stop the user's shell.

The editor keeps the captured drag image in a pointer-transparent layer window.
Native Wayland drag and drop still carries the widget ID between containers.
This avoids the zero-size drag-icon actor observed with the current Qt/Mutter
combination. The image is captured once at pickup and released when the drag ends.

The runtime patch keeps rounded clipping textures in the same visual tree as
their widgets. This lets Qt release their old window references when containers
move or reload. Successful reload banners are suppressed because their separate
QML engine can block IPC after an editor reload. Reload errors are still shown.
