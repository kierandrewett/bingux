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

Sound > Effects opens a large popup (a bottom sheet on a phone). A popup rather
than a subpage because the page is a task of its own: the stack and the selected
effect's page sit side by side and need more width than the Settings content
column has, and the stack, top bar and page scroll separately.

- **Top bar:** the master switch for the direction, the Output and Input tabs,
  and the presets menu (pick, new, duplicate, rename, import, export, delete).
- **Stack (left):** a section header naming the direction and an Add effect
  button; one row for each effect (icon, name, power switch, delete, drag grip;
  a row whose plugin is missing or has stopped says why); the device the sound
  goes to; and the level of the whole chain before and after. Up and Down move
  between rows, Delete on a focused row removes the effect, Space on its
  switch toggles it, Alt+Up and Alt+Down move it.
- **Page (right):** the effect's name and Reset, input and output gain where
  the plugin has them, the controls as cards, and "Using &lt;project&gt;". The
  equalizer has a bank of faders (one for each band), an editor for the selected
  band, Add band, Remove band and Flat response. The compressor, limiter, gate,
  expander, bass enhancer, reverb, delay and filter have curated cards;
  every other effect's cards are generated from the plugin's port metadata.
- **Phone:** the stack is the page; a row pushes the effect's page with Back
  (`master_detail`).

Tabs are Output and Input only. **Players and Recorders** (per-application
effects) are not offered: a stack belongs to a device, and an application chain
would need its own routing and lifetime handling, which the engine does not have.
There are no per-effect meters: PipeWire 1.4 does not publish a plugin's output
ports, so only the level of the whole chain is shown.

## Where each plugin comes from

Where Easy Effects hosts a published plugin, Bingux hosts the same one.

| Effects                                                                                                                               | Plugin               | Why                                                                                                                 |
| ------------------------------------------------------------------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Equalizer, mid/side equalizer, filter, compressor, limiter, gate, expander, multiband compressor and gate, delay, loudness, auto gain | Linux Studio Plugins | What Easy Effects uses (auto gain: LSP's own; Easy Effects measures with libebur128 in its code)                    |
| Bass enhancer, exciter, crusher, de-esser, reverb, stereo tools                                                                       | Calf Studio Gear     | What Easy Effects uses                                                                                              |
| Maximizer                                                                                                                             | ZamAudio (ZaMaximX2) | What Easy Effects uses                                                                                              |
| Bass loudness                                                                                                                         | MDA                  | What Easy Effects uses                                                                                              |
| Autotune                                                                                                                              | x42 (fat1)           | What Easy Effects uses                                                                                              |
| Pitch                                                                                                                                 | Rubber Band          | Easy Effects uses SoundTouch, which no distribution packages as a plugin; Rubber Band is the packaged pitch shifter |
| Convolver                                                                                                                             | PipeWire's convolver | Easy Effects uses zita-convolver in its own code                                                                    |

**Unavailable:** crossfeed (Fedora ships libbs2b without a plugin), crystalizer,
voice suppressor and crosstalk canceller (Easy Effects' own algorithms), noise
reduction by RNNoise, DeepFilterNet or Speex (no packaged plugin), echo
canceller (needs the played sound as a reference) and harmonizer (not packaged
as a plugin). They are not listed in Add effect and nothing stands in for them.

## Import

`bingux-effects import` reads a Bingux preset (version 1 or 2), an EasyEffects
preset (settings map through `import_table.rs`; the equalizer's bands, slopes and
mute carry over) and Equalizer APO / AutoEq text. Whatever has no equivalent is
listed after the import.
