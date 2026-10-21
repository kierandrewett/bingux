# Bingux architecture

Bingux is a desktop shell. It owns the user-facing surfaces and their local
state. It does not own the compositor, login manager, system package profile or
machine hardware configuration.

The shell is a Quickshell configuration run by `bingux-shell`, a build of
Quickshell with the Bingux runtime patches and a Bingux entry point
(`packages/bingux-shell`). The QML ships as one packed archive that the binary
unpacks per user. Native helpers and daemons are small packages with typed
sockets or QML interfaces. Extensions add trusted QML without changing the
shell's built-in layout model.

## Runtime layers

| Layer              | Location                     | Responsibility                                                           |
| ------------------ | ---------------------------- | ------------------------------------------------------------------------ |
| Shell              | `shell/bingux/`              | Draws the top bar, dock, sidebar, popouts and control centre.            |
| Shell runtime      | `packages/bingux-shell/`     | Quickshell built into `bingux-shell`; runs the packed shell archive.     |
| Python helpers     | `shell/bingux/*.py`          | Desktop glue the shell spawns; installed under `libexec/bingux/shell`.   |
| Native helpers     | `packages/`                  | Builds the search, status, audio and QML plugin helpers.                 |
| User state         | `$XDG_CONFIG_HOME/bingux/`   | Stores layout, settings, search, notes and extension enable state.       |
| Extension registry | `shell/bingux/extensions.py` | Discovers trusted extension manifests and preserves disabled placements. |
| Packaging          | `packaging/`                 | Builds Fedora packages and user-systemd units.                           |

The shell reads state from the user's XDG configuration directory. Packagers
set the install prefix and QML import path. The shell does not generate or
require a machine-specific configuration.

## Process boundaries

Bingux does not replace compositor or system services. It calls system APIs and
uses user-systemd for long-running helper processes. Gnoblin's compositor
socket, D-Bus portal APIs and Wayland protocols are optional integrations; the
shell can still render its general UI when they are absent.

Tailscale, NetworkManager, MPRIS, PipeWire and notification services are
detected at runtime. Missing integrations remove only their controls. They do
not prevent the shell from starting.

Secrets belong to the integration that uses them. Extension tokens must stay in
an extension-owned private file or a secret service. The shell layout never
stores credentials.

## Gnoblin integration

Gnoblin supplies the compositor and session. The stable integration points are:

- `zwlr_layer_shell_v1` for top bar, dock, sidebar and popup surfaces.
- `zwlr_foreign_toplevel_manager_v1` for dock window listing and actions.
- The versioned Gnoblin compositor socket for window, workspace, input, privacy,
  and OSD state and operations.
- The local search, status and OSD sockets for long-running helper data.

The shell owns its own layer-shell surfaces. It does not patch or depend on
GNOME Shell UI. Another compositor can use the general shell surfaces when it
provides the required protocols.

## Extension model

Extensions are discovered from XDG data directories and enabled explicitly in
`$XDG_CONFIG_HOME/bingux/extensions.json`. A manifest declares QML widgets and an optional background entry point. Widget IDs are stored
in the existing layout file, so disabling an extension does not discard the
user's placement.

The public context provides theme, presentation, actions, events, shared
buttons and anchored popups. Extensions can use their own QML and Quickshell
types. Trusted extensions also receive an `unstable` escape hatch for direct
shell access. This keeps the supported interface useful without repeating the
rigid extension restrictions found in other desktop shells.

Read [Extensions](extensions.md) for the manifest and lifecycle contract.

## Session contexts

Three `bingux-shell` processes share the widget code: the desktop shell, the
lock client (`bingux-lock`) and the login greeter (`bingux-greeter`). The
`SessionContext` singleton names which one is running (`desktop`, `locked`,
`greeter`); `LockShell` and `GreeterShell` set it before anything else loads.

Two tables follow from it. `DesktopLayout.js` gives every widget a `contexts`
list and `allowed(id, mode)` decides what a bar may show. `SessionServices`
holds the capability table (`locked`: volume, power; `greeter`: power) and
every action path that launches, searches, opens tray menus, shows
notification content, changes settings or power asks `allows(capability)`
before acting. Extensions never load outside the desktop. The lock screen and
the greeter therefore reuse the bar's widgets without inheriting the desktop's
reach. [Secure session locking](locking.md) lists what the restricted bar
shows; [Login screen](login-screen.md) covers the greeter.

## Native helpers

The Rust search daemon owns provider lifecycle, search indexes and the search
socket. The Rust status daemon owns metric and OSD socket data. The C audio
meter reports per-stream peaks and keeps its PulseAudio/PipeWire monitor queue
small because it retains only current peak values. The Qt plugins provide text
layout and settings platform support. Each helper has a bounded protocol and
can restart without restarting the shell.

## Testing

Use `scripts/test-desktop --check` for static checks and `scripts/test-desktop`
for an isolated nested desktop. Gnoblin integration tests run from the Gnoblin
repository against a private session. The standalone installer test stages
files in a temporary directory and verifies that no user path is written.
