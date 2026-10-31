"""The effects engine changes real sound: levels, an equalizer boost, bypass, order, and how long the stack's
changes interrupt it. Runs against a private PipeWire graph (bxdesktop/pwgraph.py), never the person's session.

Needs `bingux-effects` (BINGUX_EFFECTS_BIN, or in CARGO_TARGET_DIR), PipeWire with its LV2 loader and the Linux
Studio Plugins (`LV2_PATH` can point at an unpacked copy). Skipped when any is missing. Run with `-s` to see the
measured numbers; they are also written to `effects-audio.json` in the test's temporary directory.
"""

from __future__ import annotations

import json
import os
import shutil
import time
from pathlib import Path

import pytest

from bxdesktop.pwgraph import MICROPHONE, SPEAKER, Engine, PrivateGraph, db, dropouts, level_db, sine, slot

pytestmark = pytest.mark.app

REPO = Path(__file__).resolve().parents[3]
TONES = [200.0, 1000.0, 5000.0]
QUANTUM = 1024


def _binary() -> Path | None:
    candidates = [os.environ.get("BINGUX_EFFECTS_BIN", "")]
    target = os.environ.get("CARGO_TARGET_DIR")
    for base in ([Path(target)] if target else []) + [REPO / "build" / "cargo", REPO / "services/audio-effects/target"]:
        candidates += [str(base / "release/bingux-effects"), str(base / "debug/bingux-effects")]
    return next((Path(c) for c in candidates if c and Path(c).is_file()), None)


def _lv2_has_lsp() -> bool:
    paths = os.environ.get("LV2_PATH", "~/.lv2:/usr/lib64/lv2:/usr/lib/lv2:/usr/lib/x86_64-linux-gnu/lv2").split(":")
    return any((Path(p).expanduser() / "lsp-plugins.lv2").is_dir() for p in paths)


def _lv2_loader() -> bool:
    dirs = [os.environ.get("SPA_PLUGIN_DIR", "")] + [
        "/usr/lib64/spa-0.2",
        "/usr/lib/spa-0.2",
        "/usr/lib/x86_64-linux-gnu/spa-0.2",
    ]
    return any(d and (Path(d) / "filter-graph/libspa-filter-graph-plugin-lv2.so").is_file() for d in dirs)


@pytest.fixture(scope="module")
def report(tmp_path_factory):
    numbers: dict = {}
    yield numbers
    out = tmp_path_factory.getbasetemp() / "effects-audio.json"
    out.write_text(json.dumps(numbers, indent=2))
    print(f"\nmeasured numbers: {out}\n{json.dumps(numbers, indent=2)}")


@pytest.fixture(scope="module")
def engine():
    binary = _binary()
    if binary is None or not all(
        shutil.which(t)
        for t in ("pipewire", "wireplumber", "pw-cat", "pw-record", "pw-link", "pw-cli", "pw-dump", "wpctl")
    ):
        pytest.skip("needs bingux-effects and PipeWire's tools")
    if not _lv2_has_lsp() or not _lv2_loader():
        pytest.skip("needs the Linux Studio Plugins and PipeWire's LV2 loader (set LV2_PATH, SPA_PLUGIN_DIR)")
    graph = PrivateGraph()
    graph.start()
    engine = Engine(graph, binary)
    engine.start([])
    yield engine
    engine.stop()
    graph.stop()
    shutil.rmtree(graph.root, ignore_errors=True)


def hear(engine: Engine, seconds: float = 3.0, amplitude: float = 0.1) -> dict[float, float]:
    """The level of each test tone through the stack, relative to what was played."""
    samples = engine.graph.capture(sine(TONES, seconds, amplitude), "bingux_effects_output", SPEAKER)
    loud = next(i for i, x in enumerate(samples) if abs(x) > 1e-4) + 24000
    middle = samples[loud : loud + 96000]
    reference = 20 * __import__("math").log10(amplitude / len(TONES))
    return {f: level_db(middle, f) - reference for f in TONES}


def settle(engine: Engine, plugins: list[dict]) -> None:
    engine.write_preset(plugins)
    time.sleep(2.5)


def test_an_empty_stack_is_transparent(engine, report):
    settle(engine, [])
    levels = hear(engine)
    report["empty_stack_db"] = {str(f): round(v, 2) for f, v in levels.items()}
    assert all(abs(v) < 0.3 for v in levels.values()), levels


EQ_BELL = dict(ftl_0=1, ftr_0=1, fl_0=1000, fr_0=1000, gl_0=db(12), gr_0=db(12), ql_0=1.5, qr_0=1.5)


def test_an_equalizer_boost_raises_its_frequency_and_bypass_removes_it(engine, report):
    settle(engine, [slot("equalizer-1", "equalizer", **EQ_BELL)])
    boosted = hear(engine)
    settle(engine, [slot("equalizer-1", "equalizer", False, **EQ_BELL)])
    bypassed = hear(engine)
    report["equalizer_plus_12db_at_1khz_db"] = {str(f): round(v, 2) for f, v in boosted.items()}
    report["equalizer_bypassed_db"] = {str(f): round(v, 2) for f, v in bypassed.items()}
    assert 11.0 < boosted[1000.0] < 13.0, boosted
    assert abs(boosted[200.0]) < 1.5 and abs(boosted[5000.0]) < 1.5, boosted
    assert all(abs(v) < 0.3 for v in bypassed.values()), bypassed


def test_the_order_of_two_effects_changes_the_sound(engine, report):
    # A hard compressor then 12 dB of gain, against the gain first: 12 dB apart for a quiet tone.
    compressor = slot("compressor-1", "compressor", al=db(-30), cr=100, at=1, rt=50, mk=1)
    gain = slot("equalizer-1", "equalizer", g_out=db(12))
    settle(engine, [compressor, gain])
    compress_first = hear(engine)[1000.0]
    settle(engine, [gain, compressor])
    gain_first = hear(engine)[1000.0]
    report["compressor_then_gain_db"] = round(compress_first, 2)
    report["gain_then_compressor_db"] = round(gain_first, 2)
    assert compress_first - gain_first > 6.0, (compress_first, gain_first)


def _interruptions(engine, label, plugins_after, report, bound):
    """Play a loud 1 kHz tone, make `plugins_after` changes during it, count the dropouts."""
    found: list[int] = []

    def make(started):
        for when, plugins in plugins_after:
            time.sleep(max(0.0, started + when - time.monotonic()))
            engine.write_preset(plugins)

    samples = engine.graph.capture(sine([1000.0], 9.0, 0.3), "bingux_effects_output", SPEAKER, between=make)
    found = [length for _, length in dropouts(samples)]
    report[f"interruptions_{label}"] = {
        "operations": len(plugins_after),
        "dropouts": len(found),
        "lengths_in_samples": found,
    }
    assert all(length <= bound for length in found), (label, found)
    return found


def test_control_changes_do_not_interrupt_the_sound(engine, report):
    def eq(gain):
        return slot("equalizer-1", "equalizer", g_out=db(gain))

    settle(engine, [eq(0)])
    changes = [(2.0 + 0.3 * i, [eq(1.0 if i % 2 else -1.0)]) for i in range(20)]
    found = _interruptions(engine, "twenty_control_changes", changes, report, 0)
    assert found == []


def test_structural_changes_interrupt_for_no_more_than_a_few_cycles(engine, report):
    a, b, c = slot("eq-a", "equalizer"), slot("lim-b", "limiter"), slot("gate-c", "gate")
    settle(engine, [b, a])
    cycles = 4 * QUANTUM
    _interruptions(engine, "four_reorders", [(2, [a, b]), (4, [b, a]), (6, [a, b]), (8, [b, a])], report, cycles)
    _interruptions(
        engine,
        "four_bypass_toggles",
        [(2, [dict(b, enabled=False), a]), (4, [b, a]), (6, [b, dict(a, enabled=False)]), (8, [b, a])],
        report,
        cycles,
    )
    _interruptions(
        engine, "two_adds_two_removes", [(2, [b, a, c]), (4, [b, a]), (6, [c, b, a]), (8, [b, a])], report, 2 * cycles
    )


def test_the_input_direction_runs_microphone_sound_through_the_stack(engine, report):
    engine.stop()
    mic = Engine(engine.graph, engine.binary, direction="input", device=MICROPHONE)
    mic.start([slot("equalizer-1", "equalizer", **EQ_BELL)])
    try:
        time.sleep(2.0)
        samples = engine.graph.capture(sine(TONES, 3.0, 0.1), MICROPHONE, "bingux_effects_input", sink=False)
        loud = next(i for i, x in enumerate(samples) if abs(x) > 1e-4) + 24000
        middle = samples[loud : loud + 96000]
        reference = 20 * __import__("math").log10(0.1 / len(TONES))
        levels = {f: level_db(middle, f) - reference for f in TONES}
        report["input_equalizer_plus_12db_at_1khz_db"] = {str(f): round(v, 2) for f, v in levels.items()}
        assert 11.0 < levels[1000.0] < 13.0, levels
    finally:
        mic.stop()
