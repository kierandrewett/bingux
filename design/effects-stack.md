# Effects stack

Status: the engine (`services/audio-effects`) is done and measured. The Settings
popup is **not built** yet (see "Not done").

## Easy Effects plugins and what Bingux hosts

Source: `src/*.cpp` and `tags_plugin_name.hpp` of github.com/wwmm/easyeffects
(master, 30 Oct 2026). Bingux hosts the same plugin wherever Easy Effects hosts
a published one; the URIs are in `services/audio-effects/src/effects.rs`.

| Easy Effects                                                    | Bingux hosts                                                                                                                                                      | Package (Fedora / Debian / Arch)                           | Status                 |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | ---------------------- |
| Equalizer, Mid/side equalizer                                   | LSP `para_equalizer_x32_lr`, `_ms`                                                                                                                                | lsp-plugins-lv2 / lsp-plugins-lv2 / lsp-plugins            | done                   |
| Compressor, Limiter, Gate, Expander                             | LSP `compressor_stereo`, `limiter_stereo`, `gate_stereo`, `expander_stereo` (Easy Effects uses the `sc_` side-chain variants; a chain has no external side-chain) | same                                                       | done                   |
| Multiband compressor, Multiband gate                            | LSP `mb_compressor_stereo`, `mb_gate_stereo`                                                                                                                      | same                                                       | done                   |
| Filter, Delay, Loudness                                         | LSP `filter_stereo`, `comp_delay_x2_stereo`, `loud_comp_stereo`                                                                                                   | same                                                       | done                   |
| Auto gain                                                       | LSP `autogain_stereo` (Easy Effects uses libebur128 in its own code)                                                                                              | same                                                       | done, different plugin |
| Bass enhancer, Exciter, Crusher, De-esser, Reverb, Stereo tools | Calf `BassEnhancer`, `Exciter`, `Crusher`, `Deesser`, `Reverb`, `StereoTools`                                                                                     | lv2-calf-plugins / calf-plugins / calf                     | done                   |
| Maximizer                                                       | ZamAudio `ZaMaximX2`                                                                                                                                              | lv2-zam-plugins / zam-plugins / zam-plugins                | done                   |
| Bass loudness                                                   | MDA `Loudness`                                                                                                                                                    | lv2-mdala-plugins / mda-lv2 / mda.lv2                      | done                   |
| Autotune                                                        | x42 `fat1` (mono, one instance per side)                                                                                                                          | lv2-x42-plugins / x42-plugins / x42-plugins                | done                   |
| Convolver                                                       | PipeWire's built-in convolver (Easy Effects uses zita-convolver)                                                                                                  | pipewire                                                   | done, different engine |
| Pitch                                                           | Rubber Band R3 LADSPA (Easy Effects uses SoundTouch)                                                                                                              | ladspa-rubberband-plugins / rubberband-ladspa / rubberband | done, different plugin |
| Crossfeed (libbs2b)                                             | none                                                                                                                                                              | Fedora ships the library only                              | not available          |
| Crystalizer, Voice suppressor, Crosstalk canceller              | none                                                                                                                                                              | Easy Effects' own algorithms                               | not available          |
| RNNoise, DeepFilterNet, Speex                                   | none                                                                                                                                                              | no distribution packages a plugin wrapper                  | not available          |
| Echo canceller (WebRTC)                                         | none                                                                                                                                                              | needs the played sound as a reference                      | not available          |
| Harmonizer (Rakarrack Plus)                                     | none                                                                                                                                                              | not packaged as a plugin                                   | not available          |
| Level meter, Output level, Spectrum                             | none                                                                                                                                                              | analysis views of Easy Effects                             | not built              |

All 23 hosted plugins were found and read on Fedora 43 (LSP 1.2.29, Calf 0.90.9,
Zam 4.4, MDA 1.2.10, x42 0.21, Rubber Band) by `bingux-effects scan`.

The LV2 loader: PipeWire's `filter-chain` hosts LV2 through
`libspa-filter-graph-plugin-lv2.so`. It ships in `pipewire-module-filter-chain-lv2`
(Fedora), `libspa-0.2-modules` (Debian trixie, checked in its file list) and
`pipewire-audio` (Arch, checked in its file list). The old docs saying PipeWire
has no LV2 loader were wrong for those distributions.

## Chain model

An ordered stack per direction (Output: applications to the device; Input:
microphone to applications). Each effect is one `pipewire` process holding one
`filter-chain` (capture stream, plugin, playback stream). A head stage (the
virtual device "Effects (Output)", or the microphone) and a tail stage (the
speakers, or the virtual microphone) stay up. The supervisor
(`bingux-effects run`) joins them with links through one long-lived `pw-cli`.
Add = start a stage beside the others, feed it from the head until it is
processing, then switch the links. Remove = switch the links, then stop it.
Reorder and bypass = links only; every stage keeps its state. A control moved =
`set-param` on the stage. A crashed stage leaves the chain and is restarted
(three tries). Settings edits the preset file and sends `SIGHUP`
(`bingux-effects reload`); `chain::plan` works out the difference.

Per-app chains (Players/Recorders): not supported. A chain belongs to a device.
Per-app would need a stream-to-chain routing table and a chain per app.

Plugin output control ports (gain reduction, level) are not readable through
PipeWire 1.4: the filter graph exposes them only to links inside the graph, not
in `Props`. Per-effect meters are therefore not possible; chain input and output
levels can come from the sink monitors.

## Presets

Version 2: effect `type` is the kind (`compressor`), `controls` are the plugin's
port symbols (`al`, `cr`, `ftl_0`). Version 1 files (Bingux's own plugins) are
migrated by `legacy.rs` when the store starts; the original is kept in
`presets/legacy/` with a notes file. EasyEffects presets import through
`import_table.rs`, generated from Easy Effects' own `BIND_LV2_PORT` lines.

## Measured on a private PipeWire 1.4.11 + WirePlumber graph

Non-realtime daemon, machine load average about 10, quantum 1024 at 48 kHz.

- Gain/level: empty chain 0.00 dB at 200 Hz, 1 kHz and 5 kHz; LSP equalizer
  bell +12 dB at 1 kHz (Q 1.5): +12.00 dB at 1 kHz, +0.68 and +0.63 dB at 200 Hz
  and 5 kHz; the same effect switched off: 0.00 dB everywhere.
- Latency added by one or three chained stages: 0 samples (click test).
- Interruptions during a 1 kHz tone (dropout = at least 12 samples under -26 dB):
  baseline none; reorder 4 dropouts in 20 operations (2048, 2048, 1026, 2049
  samples, i.e. one or two cycles); toggle 7 in 20 (six of 240 samples, which is
  the limiter's 5 ms look-ahead appearing or disappearing, one of 2049); add and
  remove 3 in 20 (4336, 2049, 2050); control changes 0 in 40.
  Spawning `pw-link` for every change gave 30 to 100 ms gaps; the persistent
  `pw-cli` gave the figures above.

## Not done

The Settings popup, its shared components (master-detail list, vertical slider,
level bar), taxonomy entries, motion tests, layout probes and screenshots. The
Settings app still uses the old engine API and does not build.
