# Bingux desktop-shell contract

## Scope

The Bingux desktop shell is an optional profile feature. It supplies the top bar, dock, search surface, notification surface, and on-screen display for a Gnoblin session. It is not a compositor and it does not modify GNOME Shell UI.

The reference implementation uses Quickshell 0.3.x for layer-shell surfaces. Quickshell already supplies `zwlr_layer_shell_v1`, `zwlr_foreign_toplevel_manager_v1`, StatusNotifierItem menus, desktop-entry actions, and the desktop-notification service. Bingux pins the exact Quickshell package through its flake lock.

Quickshell is pre-1.0. Bingux source must target one pinned Quickshell release line. A Quickshell update is a deliberate compatibility change, not an automatic API promise.

## Process model

```text
Gnoblin org.gnoblin.Shell.SuperReleased
                    |
                    v
          bingux-searchd user service
                    |
       $XDG_RUNTIME_DIR/bingux/search-v1.sock
                    |
                    v
      Quickshell bingux desktop-shell process
```

`bingux-searchd` is the sole consumer of the Gnoblin D-Bus signal. The QML process does not parse D-Bus output or start ad-hoc monitors. It connects to one local Unix socket and renders typed records from the daemon.

The shell and daemon run as the profile user. The socket directory has mode `0700`. The socket has mode `0600`. The service does not listen on TCP or another network transport.

A missing or unsupported Gnoblin D-Bus service is an unavailable integration point. The daemon must retry with bounded backoff and report an unavailable state to the shell. It must not emulate the Super key or subscribe to the development-only Mutter key signal.

## Metrics socket protocol v1

`bingux-statusd` samples `/proc/stat`, `/proc/meminfo`, and `/proc/net/dev` once per second. It publishes
newline-delimited UTF-8 JSON records at `$XDG_RUNTIME_DIR/bingux/metrics-v1.sock`.

The service sends the most recent record when a client connects. It then sends one record after each sample. The
first record after service start has `null` CPU and network rates because there is no previous sample. The socket
directory has mode `0700`, and the socket has mode `0600`.

```json
{
  "protocolVersion": 1,
  "type": "metrics",
  "cpuPercent": 17.25,
  "memoryTotalBytes": 67331813376,
  "memoryUsedBytes": 34184560640,
  "networkReceiveBytesPerSecond": 306151.62,
  "networkTransmitBytesPerSecond": 62684.49
}
```

`cpuPercent`, `networkReceiveBytesPerSecond`, and `networkTransmitBytesPerSecond` can be `null`. The QML client
must retain a received sample for no more than three seconds. It must then show the metric as unavailable while it
reconnects with bounded backoff. A metrics failure must not stop the top bar, tray, or search interface.

## Desktop UI rules

- The top bar is a top-layer surface with a positive exclusive zone.
- The dock is a bottom-layer surface. It must not reserve work area unless a profile explicitly selects that behaviour.
- Search is an overlay surface. It asks for on-demand keyboard focus only while visible. It returns focus when it closes.
- Notifications and on-screen displays are overlay surfaces with no keyboard focus.
- All icon controls have at least a 24 by 24 pixel pointer target.
- Search supports typing, Up and Down, Enter, and Escape. Escape closes the surface and clears transient selection.
- The tray uses StatusNotifierItem and DBusMenu support. Right-click menus must come from the item when it exposes one.
- Dock actions use `.desktop` entry actions. Gnoblin does not currently export a dynamic application-menu protocol for foreign toplevels. Do not claim that arbitrary in-window menus are available until a separate, versioned Gnoblin interface exists.

## Search socket protocol v1

The transport is newline-delimited UTF-8 JSON. One record is one line. Each record is at most 64 KiB. The receiver must reject malformed JSON, records over the limit, unknown required fields, and protocol versions other than `1`.

Every request contains these fields:

```json
{
  "protocolVersion": 1,
  "type": "query",
  "requestId": "opaque-client-request-id"
}
```

`requestId` is an opaque ASCII string with 1 to 64 characters from `[A-Za-z0-9_-]`. It avoids JavaScript integer precision loss. A response that refers to a request must repeat the same `requestId`.

### Daemon-to-shell events

```json
{
  "protocolVersion": 1,
  "type": "show-search",
  "monotonicUsec": "123456789"
}
```

`show-search` is emitted only after the daemon validates a Gnoblin `SuperReleased` signal with protocol version `1`. `monotonicUsec` is a decimal string because the value can exceed JavaScript safe integer precision.

```json
{
  "protocolVersion": 1,
  "type": "integration-state",
  "name": "gnoblin-super-release",
  "state": "ready"
}
```

`state` is `ready` or `unavailable`. The shell shows no error popup for an unavailable event source. It keeps normal search access available through the top-bar action.

### Shell-to-daemon requests

A query request is:

```json
{
  "protocolVersion": 1,
  "type": "query",
  "requestId": "q-01",
  "query": "firefox",
  "limit": 20
}
```

`query` is UTF-8 text with at most 512 bytes. `limit` is an integer from 1 to 50. The daemon returns zero or more partial result records followed by one completed record:

```json
{
  "protocolVersion": 1,
  "type": "results",
  "requestId": "q-01",
  "complete": false,
  "elapsedUsec": 820,
  "results": [
    {
      "resultId": "app:firefox.desktop",
      "providerId": "apps",
      "kind": "application",
      "title": "Firefox",
      "subtitle": "Web browser",
      "icon": "firefox",
      "score": 0.98
    }
  ]
}
```

`kind` is one of `application`, `file`, `folder`, `database`, `calculation`, `weather`, `chat`, or `action`. `title`, `subtitle`, and `icon` are plain text. The UI must render them as text, not as markup. `score` is a finite number from 0 through 1.

The completed record has `complete: true`. It can contain an empty `results` array. The daemon must discard results for a request that the client has superseded with a later request.

Activation is separate from a query:

```json
{
  "protocolVersion": 1,
  "type": "activate",
  "requestId": "a-01",
  "resultId": "app:firefox.desktop"
}
```

The shell never receives an executable command from a result. The daemon resolves the short-lived `resultId` and performs or forwards the action. It returns `activated` or an `error` record. A stale or unknown result ID fails without action.

An error record is:

```json
{
  "protocolVersion": 1,
  "type": "error",
  "requestId": "q-01",
  "code": "invalid-request",
  "message": "query must contain at most 512 bytes"
}
```

Valid codes are `invalid-request`, `unsupported-protocol`, `unavailable`, `provider-failed`, and `unknown-result`. Error messages must not include a command line, secret, or stack trace.

## Search-provider contract v1

A provider is a long-lived, profile-trusted process. Its manifest is JSON and follows the same
version policy. Bingux discovers manifests only from configured profile paths. It does not scan
`PATH`, a network location, or arbitrary writable directories.

```json
{
  "kind": "bingux.search-provider",
  "protocolVersion": 1,
  "id": "apps",
  "displayName": "Applications",
  "command": ["/nix/store/example/bin/bingux-provider-apps"],
  "startup": "eager",
  "priority": 100,
  "timeoutMs": 20
}
```

`id` matches `[a-z0-9]+(?:-[a-z0-9]+)*` and contains at most 64 bytes. `command` is a non-empty
argument array. The host does not pass a shell string. `startup` is `eager` or `lazy`. `priority` is
an integer from 0 to 1000. `timeoutMs` is an integer from 1 to 10,000.

The provider protocol uses newline-delimited UTF-8 JSON on standard input and standard output.
Each record is at most 64 KiB. The host sends this record after it starts a provider:

```json
{
  "protocolVersion": 1,
  "type": "hello",
  "hostId": "bingux-searchd"
}
```

The provider must return this record before the manifest timeout:

```json
{
  "protocolVersion": 1,
  "type": "hello",
  "accepted": true
}
```

The host gives each provider query a provider-local `queryId` that matches the same ASCII rule as
`requestId`. A provider must return one or more result records, then exactly one completion record:

```json
{
  "protocolVersion": 1,
  "type": "query",
  "queryId": "provider-query-01",
  "query": "firefox",
  "limit": 20
}
```

```json
{
  "protocolVersion": 1,
  "type": "results",
  "queryId": "provider-query-01",
  "complete": false,
  "results": [
    {
      "resultId": "firefox.desktop",
      "kind": "application",
      "title": "Firefox",
      "subtitle": "Web browser",
      "icon": "firefox",
      "score": 0.98
    }
  ]
}
```

Provider-local `resultId` values match `[A-Za-z0-9._:-]{1,128}`. The combined UTF-8 byte length of
`title`, `subtitle`, and `icon` must not exceed 24 KiB, and these fields must not contain control
characters. The host maps them to short-lived opaque socket result identifiers. It may split a
provider result batch into multiple socket records to keep each record within 64 KiB. It never sends
a provider command or executable text to QML.

Activation is a separate provider record:

```json
{
  "protocolVersion": 1,
  "type": "activate",
  "activationId": "provider-activation-01",
  "resultId": "firefox.desktop"
}
```

The provider responds with `{"protocolVersion":1,"type":"activated","activationId":"provider-activation-01"}`
or with one `error` record containing the matching `queryId` or `activationId`. Valid provider
error codes are `invalid-request`, `unavailable`, and `provider-failed`.

The host starts eager providers before the first query, runs provider queries concurrently, and
enforces each manifest timeout. A malformed, oversized, out-of-order, or version-mismatched
provider record stops that provider and reports `provider-failed` for the affected request. A
provider must not make a network request on the query path. Weather reads a local cache. AI chat is
explicit user work after activation and has no instant-result promise. SQLite providers must use
configured, parameterised queries.

Provider code and manifests are trusted profile software. They run with the profile user
permissions. A manifest must not contain a secret. A provider that needs a credential receives a
profile-declared runtime secret path or environment variable from SOPS-Nix configuration.

## Performance rule

For the built-in application and calculation indexes, a warm query request must produce its first result record within 10 ms and its completed record within 30 ms at the 95th percentile. The benchmark measures daemon socket request to response. It does not treat compositor paint time or remote provider work as part of this target.

File discovery uses `rg --files` or an equivalent background index refresh. It does not walk the file system synchronously for each keypress.

## Version policy

Version 1 records contain `protocolVersion: 1`. New optional fields can be added in version 1. A required-field change, changed field meaning, or removed field requires a new protocol version and a separate socket path. The v1 host rejects other versions rather than guessing.

## Sources

- Quickshell installation and package guidance: <https://quickshell.org/docs/v0.3.0/guide/install-setup/>
- Quickshell distribution and version guidance: <https://quickshell.org/docs/v0.3.0/guide/distribution/>
- Quickshell layer-shell surfaces: <https://quickshell.org/docs/v0.3.0/types/Quickshell/PanelWindow/>
- Quickshell foreign toplevels: <https://quickshell.org/docs/v0.3.0/types/Quickshell.Wayland/ToplevelManager/>
- Quickshell StatusNotifierItem support: <https://quickshell.org/docs/v0.3.0/types/Quickshell.Services.SystemTray/SystemTrayItem/>
- Quickshell notification server: <https://quickshell.org/docs/v0.3.0/types/Quickshell.Services.Notifications/NotificationServer/>
- Gnoblin interface source: `~/dev/gnoblin/src/gnome-shell-overlay/js/ui/components/gnoblinControl.js`
