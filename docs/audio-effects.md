# Sound effects

Sound > Effects puts a chain of plugins between the applications and the
speakers (output effects) or between a microphone and the applications (input
effects). Settings is a front end: it edits plain preset files and a small
engine, `bingux-effects`, plays them through PipeWire.

## Engine

Each direction runs one supervisor, `bingux-effects run <direction>`. It starts a
`pipewire` process for every effect (one PipeWire `filter-chain` each) plus a head
and a tail stage, and joins them with links:

```
applications -> Effects (Output) -> effect 1 -> effect 2 -> ... -> speakers
microphone   -> effect 1 -> ... -> Effects (Input) -> applications
```

The preset file is the source of truth. Settings writes it and runs
`bingux-effects reload <direction>` (a `SIGHUP`); the engine changes only what
differs: a moved control is set live, an effect switched off, on or moved only
changes links, an added effect starts beside the others and joins when it is
processing, a removed one leaves the links first. A stage that crashes leaves the
chain and is restarted up to three times. `bingux-effects status <direction>`
prints each effect's state.

See [design/effects-stack.md](../design/effects-stack.md) for the measurements
(interruptions while the stack changes) and what the engine cannot do: no
per-application chains, no plugin output meters (PipeWire 1.4 does not publish
them).

Commands:

```sh
bingux-effects restore            # start each direction that is on (run at login)
bingux-effects run output         # run one direction (what the systemd unit does)
bingux-effects reload output      # read the preset again
bingux-effects render output      # print the configuration of every stage
bingux-effects scan               # the plugins found, as JSON
bingux-effects status output
bingux-effects import FILE output [NAME]
```

## Install the plugins

Bingux ships no plugin. The effects are the published plugins Easy Effects hosts.
PipeWire must have its LV2 loader.

| Needs                | Fedora                             | Debian               | Arch             |
| -------------------- | ---------------------------------- | -------------------- | ---------------- |
| LV2 loader           | `pipewire-module-filter-chain-lv2` | `libspa-0.2-modules` | `pipewire-audio` |
| Metadata reader      | `lilv-libs`                        | `liblilv-0-0`        | `lilv`           |
| Linux Studio Plugins | `lsp-plugins-lv2`                  | `lsp-plugins-lv2`    | `lsp-plugins`    |
| Calf                 | `lv2-calf-plugins`                 | `calf-plugins`       | `calf`           |
| ZamAudio             | `lv2-zam-plugins`                  | `zam-plugins`        | `zam-plugins`    |
| MDA                  | `lv2-mdala-plugins`                | `mda-lv2`            | `mda.lv2`        |
| x42                  | `lv2-x42-plugins`                  | `x42-plugins`        | `x42-plugins`    |
| Rubber Band          | `ladspa-rubberband-plugins`        | `rubberband-ladspa`  | `rubberband`     |

An effect whose plugin is missing stays in the stack, is left out of the sound
and reports the package that has it.

## Effects

The list, with each plugin's address, is `services/audio-effects/src/effects.rs`;
the mapping to Easy Effects' plugins is in the design note. Controls come from the
plugin's own port metadata (range, default, scale, unit, enumeration choices).

## Files

```
~/.config/bingux/effects/
    state.json                      what is on, the preset, the device
    output/presets/<name>.json      one preset per file
    input/presets/<name>.json
```

`state.json`:

```json
{
    "output": {
        "enabled": true,
        "preset": "Night",
        "device": "alsa_output.usb-Fosi_Audio_K5_Pro-00.analog-stereo",
        "autoload": { "alsa_output.pci-0000_00_1f.3.analog-stereo": "Voice clarity" }
    },
    "input": { "enabled": false, "preset": "Flat", "device": null, "autoload": {} }
}
```

`device` is the real device by PipeWire node name. `autoload` names a preset to
use whenever that device is chosen.

### Preset format

```json
{
    "format": "bingux-effects-preset",
    "version": 2,
    "name": "Night",
    "direction": "output",
    "plugins": [
        {
            "id": "compressor-1",
            "type": "compressor",
            "enabled": true,
            "controls": { "al": 0.0251, "cr": 6.0, "at": 5.0, "rt": 250.0 }
        },
        { "id": "limiter-1", "type": "limiter", "controls": { "th": 0.708 } }
    ]
}
```

| Field      | Meaning                                                                                                                           |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `type`     | The kind of effect (`effects.rs`): `equalizer`, `compressor`, `limiter`, `gate`, `reverb`, ...                                    |
| `controls` | Values by the plugin's port symbol, as the plugin stores them (LSP gains are linear). A missing control has the plugin's default. |
| `enabled`  | An effect that is off is left out of the sound and keeps its settings.                                                            |
| `path`     | Convolver only: the impulse response file.                                                                                        |

Presets written for Bingux's old plugins (version 1) are migrated when the store
starts; the original stays in `presets/legacy/` with a `.notes.txt` listing what
could not be carried across.

Factory presets, written once on first use and never rewritten if deleted:
output: Flat, Voice clarity, Night (compressor and limiter), Bass boost,
Loudness; input: Flat, Voice.

## The page

Not rebuilt yet: the Settings page still follows the old engine (see the design
note, "Not done").

## Import

`bingux-effects import` reads a Bingux preset (version 1 or 2), an EasyEffects
preset (settings map through `import_table.rs`; the equalizer's bands, slopes and
mute carry over) and Equalizer APO / AutoEq text. Whatever has no equivalent is
listed after the import.
