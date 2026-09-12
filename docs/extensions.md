# Write a Bingux extension

An extension is a folder with an `extension.json` manifest and QML files. It can add widgets,
run background code, expose actions, open popups and provide its own settings page.
Its QML can use Qt and Quickshell directly. The host does not prescribe a widget's contents.

Extensions are trusted code. They run in the shell process with the user's access to files,
processes and the network. This interface is not a sandbox. A native crash or an infinite
JavaScript loop can stop the shell. Use a separate process for work that needs isolation.

## Install and manage

Copy an extension into `~/.local/share/bingux/extensions/<id>/`. The folder name must match
its manifest ID. System packages can use `/usr/share/bingux/extensions/<id>/` or
`/usr/local/share/bingux/extensions/<id>/`. XDG data paths are respected; a user copy takes precedence.

Open **Bingux Settings > Extensions**, select **Reload extensions**, then enable the extension.
Its widgets appear in **Customise UI**. Drag them into the bar, dock, sidebar or control centre.
The normal layout file stores their position and display overrides.

From a source checkout, the same controls are available from the command line:

```sh
python3 shell/bingux/extensions.py list
python3 shell/bingux/extensions.py enable org.bingux.example
python3 shell/bingux/extensions.py disable org.bingux.example
```

The running shell also exposes `extensions.status`, `reload`, `enable(id)`, `disable(id)` and
`invoke(id, jsonPayload)` through its existing IPC:

```sh
binguxctl ipc extensions status
binguxctl ipc extensions reload
binguxctl ipc extensions invoke org.bingux.example/increment '{}'
```

Enable state is stored in `~/.config/bingux/extensions.json`. Discovery alone does not enable code.
Missing or disabled widgets retain their layout position and show an unavailable marker.
Re-enable the extension to restore them. Reload restarts enabled extension components.
Start the shell with `BINGUX_NO_EXTENSIONS=1` to bypass all extensions during recovery.

## Manifest

```json
{
    "id": "org.example.home",
    "name": "Home",
    "apiVersion": 1,
    "entry": "Service.qml",
    "settings": "Settings.qml",
    "widgets": [
        {
            "id": "status",
            "name": "Home status",
            "icon": "user-home-symbolic",
            "component": "Status.qml",
            "preview": "Preview.qml"
        }
    ]
}
```

Only `id`, `name` and `apiVersion` are required at the top level. `entry`, `settings`,
`widgets`, `icon` and `preview` are optional. Preview defaults to the widget component.
Extra metadata is accepted. IDs use lower-case letters, digits, dots, hyphens and underscores,
start with a letter or digit, and are at most 128 characters. Component paths are relative
QML files inside the extension folder.

The saved widget ID is `extension:org.example.home/status`. Widget IDs are unique within an
extension. The current layout supports one placement per widget ID; an extension can declare
several widgets, including several widgets that use the same component.

`apiVersion` selects the host contract. It is independent of the Bingux release. There is no
shell-version whitelist. API 1 is the first contract under validation. New optional features
can be added to it; incompatible changes need a new API version and a documented migration.
Do not silently change API 1 behaviour or remove it with an ordinary shell update.

## Component context

Each entry point, widget, preview and settings component declares:

```qml
required property var context
```

Widgets provide `implicitWidth` and `implicitHeight`. The host supplies their actual size.
Entry points can be `Item` or `QtObject` components. Use `Component.onCompleted` to start
work and `Component.onDestruction` to stop it. QML children are destroyed with their component.
Clean up external changes, processes and signal connections that your code owns.

| Member                           | Meaning                                                                |
| -------------------------------- | ---------------------------------------------------------------------- |
| `apiVersion`                     | Host contract, currently `1`                                           |
| `extensionId`, `widgetId`        | Owner IDs; `widgetId` is empty outside widgets                         |
| `preview`                        | Use sample data and do not start live work                             |
| `editing`                        | Customise UI is active, or this is a preview                           |
| `container`                      | Current placement, such as `top-right`, `dock` or `sidebar`            |
| `anchorWindow`, `anchorItem`     | Current window and item for a widget popup                             |
| `presentation`                   | Effective label, icon, display mode, `showIcon` and `showText`         |
| `theme`                          | Shell theme; supported tokens listed below                             |
| `reportError(message)`           | Add an error to Settings > Extensions                                  |
| `registerAction(name, callback)` | Register `<extensionId>/<name>`; removed when the context is destroyed |
| `invokeAction(id, payload)`      | Call a registered action and return its result                         |
| `publish(name, payload)`         | Send an event to extension contexts                                    |
| `event(name, payload)`           | Signal for events from extension contexts                              |
| `unstable`                       | Direct access to shell objects, outside the compatibility contract     |

Stable theme tokens are `text`, `muted`, `accent`, `warning`, `elevated`, `popupSurface`,
`fontFamily`, `fontSize`, `fontSmall`, `gap`, `padding`, `radius` and `barHeight`.
The theme object exposes more fields; use the documented fields for compatibility.

Use `Connections { target: context; function onEvent(name, payload) { ... } }` to receive events.
Action names and event names should include the extension ID to avoid collisions. Actions are
synchronous; start slow work asynchronously. A missing action throws an error.

The host disables widget input during customisation. Extensions must also honour `editing`
for their own shortcuts and separate windows. Previews always have `editing: true`.
A preview can use the same component with mock data, or a separate preview component.

## Shared shell controls

Full custom QML is supported. These optional helpers reuse Bingux's existing components:

- `context.createButton(parent, properties)` creates the shell button. Set `label`, `iconName`
  and connect its `clicked` signal. Its presentation and window follow the widget placement.
- `context.createFace(parent, properties)` creates the shell icon/text presentation.
- `context.createPopup(contentComponent, properties)` creates the shell popup. It follows the
  widget across containers, uses normal dismissal and motion, and closes during customisation.
  Its content receives `context`. Set `popupWidth` and content `implicitHeight`; toggle
  `popup.visible` to open and close. This helper returns `null` in previews.

Buttons and faces belong to the supplied parent; use a parent inside the widget. The context
owns popups and destroys them on unload. A QML component can also own its
own UI, processes, models and windows without using the helpers. See the runnable
[counter example](../examples/extensions/org.bingux.example/Counter.qml).

## Settings and deeper changes

The optional `settings` component appears under **Configure** on the Extensions page.
It can provide any QML UI. Store its settings separately from the desktop layout, for example
in `$XDG_CONFIG_HOME/bingux/extensions/<id>/`. Keep access tokens in a separate private file
or a secret service. The extension controls its own schema and migration.

In the main shell process, `context.unstable` exposes `root`, `topBar`, `dock`, `controlCentre`
and `sidebar`. These are actual objects, so an extension can connect signals, create children
or replace behaviour. This access is deliberately open. Restore modifications on unload and
check members before use. The objects can change between releases. `unstable` is `null` in
standalone Settings and other processes without the main shell.

Use the supported context when it covers a feature. If an internal hook proves useful to
several extensions, add it to the public contract with tests instead of forcing each extension
to maintain its own workaround.
