# Bingux status service

`bingux-statusd` publishes desktop metrics through
`$XDG_RUNTIME_DIR/bingux/metrics-v1.sock`. A managed Bingux installation starts
it through `bingux.target` in the Gnoblin session.

The service also reports microphone capture owners through PulseAudio or
PipeWire's PulseAudio interface. It uses `pactl subscribe` to follow changes
and bounded JSON snapshots to identify applications and input devices.
Sink monitors, peak meters and corked streams do not count as microphone
captures. Muted capture streams remain listed with `muted: true`.

It also follows the default output and input for the shell's volume and
microphone OSD. A record goes to `osd-v2.sock` when the level or mute state of
the same default device changes. The protocol is in
[docs/desktop-shell.md](../../docs/desktop-shell.md#osd-socket-protocol-v2).

The capture fields supplement the compositor's privacy booleans:

| Field                         | Type                     | Meaning                                                                                |
| ----------------------------- | ------------------------ | -------------------------------------------------------------------------------------- |
| `microphoneCapturesAvailable` | Boolean                  | `true` after a successful audio query; `false` before initialization or after failure. |
| `microphoneCaptures`          | Array, up to 32 captures | Current owners; empty when unavailable or when no stream captures audio.               |
| `app`                         | String                   | Application name; defaults to `Unknown application`.                                   |
| `appId`                       | String                   | Application ID or executable name; empty if unknown.                                   |
| `device`                      | String                   | Input description or name; defaults to `Audio input`.                                  |
| `muted`                       | Boolean                  | `true` when the stream or source is muted; otherwise `false`.                          |

Capture strings contain at most 128 bytes and no control characters. Inspect
the audio server's application and device properties with
`pactl -f json list source-outputs` and `pactl -f json list sources`.
For example, an active recorder can produce:

```json
{
    "microphoneCapturesAvailable": true,
    "microphoneCaptures": [
        { "app": "Recorder", "appId": "org.example.Recorder", "device": "Built-in microphone", "muted": false }
    ]
}
```

When a compatible status service sends the capture feed, Bingux uses it and stops its Python
microphone watcher. Older status services and installations without an
active status service retain the Python watcher. If the status feed becomes
stale, Bingux starts that fallback again. An audio-server failure reports
unavailable capture data through the status service while it reconnects.

## Network data usage recording

The daemon also hosts the recorder of the `bingux-data-usage` crate on its own
thread, so no extra service is needed. Once a minute it adds the traffic of
Ethernet, Wi-Fi, mobile and VPN interfaces to
`$XDG_DATA_HOME/bingux/data-usage.sqlite3` (`~/.local/share/bingux/` by
default); Bingux Settings reads it for the Data usage page. The recorder is
independent of the metrics feed: a full disk or a damaged database is logged to
stderr at most once a minute, recording retries, and the feed is never
affected. When the daemon stops the recorder finishes its round and stops with
it. See `packages/bingux-data-usage/README.md` for the schema, retention and
reset handling.
