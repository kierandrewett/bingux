# Sound effects

Sound > Effects puts a chain of plugins between the applications and the
speakers (output effects) or between a microphone and the applications (input
effects). Settings is a front end: it edits plain preset files and a small
engine, `bingux-effects`, plays them through PipeWire.

## Engine

The engine turns the active preset into a PipeWire `filter-chain`
configuration and runs it as its own `pipewire` client process, one for each
direction. It makes a virtual device ("Effects (Output)", a sink, or
"Effects (Input)", a source), makes it the default device and sends the result
to the real device. When it stops it gives the real device back as the default.

```
applications -> Effects (Output) -> plugin 1 -> plugin 2 -> ... -> speakers
microphone   -> plugin 1 -> ... -> Effects (Input) -> applications
```

Why this design: PipeWire's `filter-chain` is on every PipeWire system and
loads its own built-in filters and any LADSPA plugin, which includes the Linux
Studio Plugins (`lsp-plugins-ladspa`). Nothing else needs to be installed, and
the application is a thin editor of a documented file. Fedora's PipeWire is
built without the `filter-chain` LV2 loader, so LV2 and CLAP plugins cannot be
hosted this way (see [Gaps](#gaps)).

Commands:

```sh
bingux-effects restore            # start each direction that is on (run at login)
bingux-effects run output         # run one direction (what the systemd unit does)
bingux-effects render output      # print the filter-chain configuration
bingux-effects scan               # the plugins found, as JSON
bingux-effects import FILE output [NAME]
```

Under systemd the user unit `bingux-effects@output.service` runs a direction
and starts it at login; Settings writes the unit and enables it when the effects
are turned on. Without a user systemd the engine is started directly.

## Plugins

| Source | Plugins |
| --- | --- |
| PipeWire built-in | parametric equalizer (one biquad per band), gain, delay, convolver (impulse responses) |
| `bingux_dsp` (installed as a LADSPA library) | compressor, multiband compressor, limiter, maximizer, gate, multiband gate, expander, de-esser, auto gain, stereo tools, loudness, bass enhancer, exciter, crossfeed, echo, reverb, and the analyser tap the engine adds |
| Any LADSPA plugin on the system | listed by category, with controls from its port metadata |

Libraries are looked for in `LADSPA_PATH` (else `~/.ladspa`, `/usr/lib64/ladspa`,
`/usr/lib/ladspa` and the like), `BINGUX_LADSPA_PATH`, `~/.local/share/bingux/ladspa`
and `lib/ladspa` beside the installed program. The scan runs in the background
and is cached in `~/.cache/bingux/effects/plugins.json` by file date.

A LADSPA plugin with one audio input and one output runs once for each channel;
one with two of each runs as a stereo plugin. A plugin with side-chain inputs
cannot be used yet and is listed as unavailable with the reason.

Controls come from the plugin's range hints: bounds, default, logarithmic,
integer and toggled. LADSPA has no units, so a control's unit (dB, Hz, ms, %,
ratio) is guessed from its name.

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
  "version": 1,
  "name": "Night",
  "direction": "output",
  "plugins": [
    {
      "id": "equalizer-1",
      "type": "equalizer",
      "enabled": true,
      "preamp": -2.0,
      "bands": [
        { "type": "low_shelf", "frequency": 100.0, "gain": 4.0, "q": 0.707, "enabled": true }
      ]
    },
    {
      "id": "compressor-1",
      "type": "ladspa:bingux_dsp:bingux_compressor",
      "enabled": true,
      "controls": { "Threshold": -32.0, "Ratio": 6.0, "Makeup": 8.0 }
    }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `format`, `version` | Always `bingux-effects-preset` and `1`. A newer version is refused. |
| `name` | The preset's name. |
| `direction` | `output` or `input`. |
| `plugins` | The chain, in order. |
| `id` | Names a plugin in this preset: letters, digits and `-`, unique. |
| `type` | `equalizer`, `gain`, `delay`, `convolver` or `ladspa:<library>:<label>`, where `<library>` is the file name without `lib` and `.so`. |
| `enabled` | A plugin that is off is left out of the audio path. |
| `controls` | Control values by port name. A missing control has its default. Gain and delay controls are in dB and ms. |
| `preamp`, `bands` | Equalizer only. A band's `type` is `peaking`, `low_shelf`, `high_shelf`, `low_pass`, `high_pass`, `band_pass`, `notch` or `all_pass`. |
| `path` | Convolver only: the impulse response file. |

Factory presets, written once on first use and never rewritten if deleted:
output: Flat, Voice clarity, Night (compressor and limiter), Bass boost,
Loudness; input: Flat, Voice.

## The page

Sound > Effects follows EasyEffects' layout in Bingux's controls. From the top:
the master switch for the effects (in the page header), an Output / Input tab
rail, the spectrum strip, the preset group, then the chain list at the left and
the selected effect at the right.

- **Chain list:** Add effect, Close (hides the list), one row for each effect
  (select it, switch it on or off, delete it, drag its grip to reorder it) and
  the device the chain works on. A narrow window shows only the list; a row opens
  the effect's own page, `sound/effects/<direction>.<plugin id>`.
- **Pane:** Input and Output level meters (what reaches the chain and what
  leaves it), the effect's controls as titled cards in a grid (two columns when
  the pane is 560 px wide or more, else one), a footer that names the provider
  ("Using Bingux DSP") and Reset. Controls are sliders with their value and unit;
  Bingux has no decimal number field, so none is used.
- **Not built:** the Players and Recorders tabs (per-application effects) and
  "Show native window", which have no counterpart here.

## LV2 and CLAP

Hosting them needs an LV2 host (`lilv`) or a CLAP host and a process that runs
them as a PipeWire stream filter. What is on the development machine: `lilv-libs`
0.26 (runtime only; no `lilv-devel`, no `lv2-devel` headers, `serd-devel`,
`sord-devel` or `sratom-devel`), Calf's LV2 plugins, no CLAP host or headers, and
no `lilv`, `livi`, `clack` or `clap-sys` crate in the cargo registry or vendored
set. PipeWire 1.4.11's filter-graph plugins are `builtin`, `ladspa` and `ebur128`:
no LV2. So the engine cannot load an LV2 plugin today, and a host would need
either the `-devel` packages or a hand-written binding to `liblilv-0.so.0`.

## Live changes

A control moved on a slider is set in the running chain with `pw-cli` (at most
every 30 ms), so it is heard while it moves. Adding, removing, reordering,
switching off a plugin, or changing a band's type restarts the engine 0.4
seconds after the last change; the sound pauses for a fraction of a second.
Every change is also saved to the preset file 0.25 seconds after the last.

Gain reduction of the Bingux dynamics plugins is shown live. PipeWire does not
expose a plugin's output ports, so these plugins publish their readings in a
small shared file (`$XDG_RUNTIME_DIR/bingux/effects/<direction>.meters`, layout
in `packages/bingux-dsp/src/meters.rs`). The meter reads it once a frame while
the row is on screen in a focused window, and not at all otherwise.

## Import

Sound > Effects > Preset > Import reads:

- a Bingux preset;
- an EasyEffects preset (`output` or `input` section): equalizer (bell, shelf,
  pass, notch, all-pass bands; preamp from the gains), compressor, limiter,
  gate, expander, de-esser, auto gain, crossfeed, stereo tools (polarity only),
  bass enhancer and exciter (approximate amount), convolver (kernel path), delay
  (left time) and filter (as an equalizer band);
- Equalizer APO / AutoEq text: `Preamp:` and `Filter:` lines (PK, LS, LSC, HS,
  HSC, LP, LPQ, HP, HPQ, BP, NO, AP; Q or bandwidth) and `GraphicEQ:` (drawn as
  bell filters, approximate).

Whatever has no equivalent is listed in a dialog after the import.

## EasyEffects parity

| EasyEffects feature | Status | Reason |
| --- | --- | --- |
| Output and input pipelines | Done | One engine process for each direction |
| Presets: save, load, rename, delete, duplicate | Done | Plain JSON files |
| Preset import and export | Done | Bingux files; EasyEffects JSON and APO/AutoEq are imported (see above), export is Bingux format only |
| Autoload preset for a device | Partial | Per device; chosen in the Sound page's device list and the Preset group. A device that appears by hot-plug is not switched to automatically |
| Per-application effects | Not done | Effects apply to the whole device |
| Parametric equalizer | Done | Biquad bands, live; curve drawn from the same coefficients |
| Graphic equalizer, FIR/FFT equalizer | Not done | Use bands; APO GraphicEQ imports as bands |
| APO and AutoEq import | Done | See above |
| Compressor, limiter, gate, expander, de-esser | Done | `bingux_dsp`; no side-chain input or filters, fixed 3 ms limiter look-ahead, no oversampling |
| Multiband compressor, multiband gate, maximizer | Done | `bingux_dsp`: three bands (Linkwitz-Riley crossovers, adjustable), per-band compressor or gate with a gain-reduction meter for each band; the maximizer is gain into the look-ahead limiter. No more than three bands |
| Auto gain | Partial | Slow RMS follower, not EBU R128 |
| Bass enhancer, exciter, crystalizer | Partial | Bass enhancer and exciter exist; crystalizer does not |
| Bass loudness, loudness | Partial | One loudness plugin (shelves), not an equal-loudness contour |
| Convolver | Done | PipeWire built-in; a mono or stereo WAV impulse response |
| Crossfeed, stereo tools | Partial | Crossfeed done; stereo tools has width, balance, mid/side gain, mono, polarity, swap |
| Delay, echo, reverb | Done | Delay (built-in), echo, Freeverb-style reverb |
| Pitch | Not done | No LADSPA pitch shifter bundled |
| Noise suppression (RNNoise), Speex, WebRTC echo cancel, voice | Not done | `librnnoise.so.0` and `webrtc-audio-processing` are installed, but no LADSPA or LV2 plugin that wraps them is; writing the wrapper needs `rnnoise` headers (`rnnoise-devel` is not installed) |
| Level meters in and out | Done | Before and after the chain |
| Spectrum analyser | Done | `bingux_analyser` taps the end of every chain (2048-point FFT, 64 log bands, about 23 updates a second) and publishes into the meter file; the page draws it only while it is visible and focused, and nothing when there is no reading |
| Linux Studio Plugins | Partial | Any installed LADSPA build is listed with generated controls and grouped by category; untested here because they are not installed on the development machine. Side-chain and 3+ input variants are listed as unavailable |
| LV2 and CLAP plugins | Not done | See [LV2 and CLAP](#lv2-and-clap): no host library or headers are installed or vendored, and the installed PipeWire has no LV2 loader |
| Reorder by drag, bypass, per-plugin controls | Done | |
| Interactive curve (drag points) | Not done | The curve follows every edit; points are edited with the band sliders |
| Gain-reduction meters | Done | For the Bingux dynamics plugins only |

## Gaps

- LV2 and CLAP hosting (see the table). The route is a separate host process
  built on `lilv` and `clap`, run as a PipeWire stream filter.
- Output control ports of third-party LADSPA plugins cannot be read through
  PipeWire, so their meters do not show.
- The real device is fixed when the effects first start (or chosen on the Sound
  page); the engine does not follow a hot-plugged device.
