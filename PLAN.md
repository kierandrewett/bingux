# Bingux production-readiness plan

This plan covers three linked pieces of work: a proper configuration system,
a dedicated Settings application, and a polish pass over the shell. Each
phase keeps the tree working and lands as its own set of commits.

## Goals

- Replace `settings.json` plus `settings-backend.py` with one declared
  configuration schema, a human-editable TOML file and a CLI. Graphical
  edits and manual edits use the same file and the same validation.
- Replace every `gnome-control-center` hand-off with a Bingux Settings
  application written in Rust with Freya. It follows the Bingux design
  tokens and the settings window rules that the earlier QML window used.
- Remove shipped test scaffolding, GNOME-only paths and silent failures
  that block calling the shell production ready.

## Phase 1: configuration system (`packages/bingux-config`)

- Rust library plus `bingux-config` binary. The library declares the schema
  with serde types, defaults and validation. The validation rules are a
  direct port of `shell/bingux/settings-backend.py`.
- File: `$XDG_CONFIG_HOME/bingux/bingux.toml`. Writes go through
  `toml_edit` so comments and key order survive graphical edits. Writes are
  atomic (temporary file, fsync, rename) under `bingux/config.lock`.
- First run migrates an existing `settings.json` into `bingux.toml` and
  renames the old file to `settings.json.migrated`.
- CLI: `dump` (JSON in the shape the shell expects), `get KEY`, `set KEY
VALUE`, `save` (stdin JSON patch), `check`, `path`, `defaults` (annotated
  TOML with every key), `wallpaper`, `import-layout`.
- Consumers switch to the binary: `BinguxPreferences.qml` (helper command
  and watched path), `bingux-searchd` (library dependency), `preview_limits.py`
  (`bingux-config dump`). `settings-backend.py` is deleted.
- Packaging: Makefile `daemons` target, `install-shell.py`, RPM spec.
- Documentation: `docs/configuration.md` lists every key, type, default and
  accepted values.

## Phase 2: Settings application (`packages/bingux-settings`)

- Freya 0.5.0-rc.10. Borderless window with a 47 px header bar, 260 px
  sidebar, overlay sidebar below 740 px, 600 px page column, boxed groups,
  48/64 px rows. Tokens come from `shell/bingux/Theme.qml`.
- Launch contract: `bingux-settings [PANEL]`. Panels match the names the
  shell already passes: `network`, `bluetooth`, `sound`, `power`, `users`,
  `background`, `display`, `desktop`, `search`.
- Pages and backends:
    - Appearance: wallpaper (`desktop.wallpaper`), accent colour and colour
      scheme (`gsettings org.gnome.desktop.interface`).
    - Desktop: dock, sidebar, top bar, monitors, clock (`bingux-config`).
    - Search: providers, engines, file locations, previews (`bingux-config`).
    - Displays: `org.gnome.Mutter.DisplayConfig` over D-Bus.
    - Sound: `wpctl` default devices and volumes.
    - Network: `nmcli` Wi-Fi list, connect, forget, airplane mode.
    - Bluetooth: `bluetoothctl` power, pair, connect, remove.
    - Power: `powerprofilesctl`, UPower battery, idle delay.
    - Keyboard and mouse: `gsettings org.gnome.desktop.peripherals` for
      pointer, touchpad and keyboard repeat. Gnoblin's Lua input settings
      override these keys (`design/lean-runtime.md`, "explicit Lua value
      overrides the current system preference"), so the page reads
      `gnoblinctl --json config show` first and shows a row as read-only with
      "Set by Gnoblin config" when the snapshot sets that field to anything
      other than `inherit`. Input sources are read-only: in a Gnoblin session
      the keymap list comes only from Lua `input_sources.sources`, and
      `org.gnome.desktop.input-sources` is ignored.
    - Date and time: `timedatectl`.
    - Users: AccountsService.
    - About: hostname, OS, kernel, hardware, Bingux and Gnoblin versions.
- Shell integration: the five launch sites call `bingux-settings PANEL`
  without `XDG_CURRENT_DESKTOP=GNOME`. The desktop entry, `binguxctl
settings`, the Makefile, `install-shell.py` and the RPM install the binary.
- Testing: headless `freya-testing` checks for the window shell and row
  components, then a QEMU run that installs the binary into a Gnoblin
  guest, opens it from the Control Centre and captures screenshots.

## Phase 3: shell polish

- Delete `NotificationTestButton.qml`; move `*Test.qml` to `tests/qml/`.
- Remove the duplicated block in `CONTRIBUTING.md`; archive dated audit
  documents under `docs/archive/`.
- Keep-awake: `systemd-inhibit` fallback when `gnome-session-inhibit` is
  absent.
- Public-IP lookup (`ipapi.co`) becomes opt-in through
  `privacy.public_ip_lookup` and is off by default.
- Replace `org.gnome.Settings-symbolic` with an Adwaita symbolic icon that
  ships with Bingux.
- Surface silent failures (capture acknowledgement, notification history,
  dock menus, search `unknown-result`) through `ShellNotifications`.
- Reconcile GNOME references in `README.md`, `docs/desktop-shell.md`,
  `docs/standalone.md`, `docs/portable-testing.md` and
  `shell/bingux/README.md`.
- Trim RPM `Requires` to what the shell calls.

## Out of scope for this pass

- Bluetooth pairing agent for PIN confirmation.
- Screenshot portal routing (#10) and tablet-pad OSD (#9).
- A graphical layout editor replacing the removed Customise UI.
