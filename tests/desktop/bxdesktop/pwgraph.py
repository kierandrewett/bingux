"""A private PipeWire + WirePlumber graph with a null speaker and a null microphone, for the audio tests.

Nothing here reaches the person's session: the graph lives in a runtime directory of its own, WirePlumber has no
D-Bus, ALSA, Bluetooth or camera monitor, and PipeWire's network discovery is left out by using a copy of the
stock configuration without it. Samples are read and measured in pure Python so the test needs nothing installed
beyond PipeWire and the plugins under test.
"""

from __future__ import annotations

import array
import cmath
import json
import math
import os
import shutil
import struct
import subprocess
import tempfile
import time
from pathlib import Path

RATE = 48000
SPEAKER = "test_out"
MICROPHONE = "test_mic"


def wav_write(path: Path, left: list[float], right: list[float] | None = None) -> None:
    right = left if right is None else right
    samples = array.array("f")
    for a, b in zip(left, right):
        samples.append(a)
        samples.append(b)
    data = samples.tobytes()
    header = (
        b"RIFF"
        + struct.pack("<I", 36 + len(data))
        + b"WAVEfmt "
        + struct.pack("<IHHIIHH", 16, 3, 2, RATE, RATE * 8, 8, 32)
    )
    path.write_bytes(header + b"data" + struct.pack("<I", len(data)) + data)


def wav_read(path: Path) -> tuple[list[float], list[float]]:
    raw = path.read_bytes()
    at = raw.index(b"data")
    size = struct.unpack("<I", raw[at + 4 : at + 8])[0]
    samples = array.array("f")
    samples.frombytes(raw[at + 8 : at + 8 + size - size % 8])
    return list(samples[0::2]), list(samples[1::2])


def sine(freqs: list[float], seconds: float, amplitude: float) -> list[float]:
    n = int(RATE * seconds)
    return [amplitude / len(freqs) * sum(math.sin(2 * math.pi * f * i / RATE) for f in freqs) for i in range(n)]


def level_db(samples: list[float], freq: float) -> float:
    """The level of one frequency in dB (a Hann-windowed single-bin transform)."""
    n = len(samples)
    k = 2 * math.pi * freq / RATE
    rot = cmath.exp(-1j * k)
    phase = 1 + 0j
    total = 0j
    norm = 0.0
    for i, x in enumerate(samples):
        w = 0.5 - 0.5 * math.cos(2 * math.pi * i / (n - 1))
        total += x * w * phase
        norm += w
        phase *= rot
    return 20 * math.log10(2 * abs(total) / norm + 1e-12)


def dropouts(samples: list[float], run: int = 12, floor: float = 0.05) -> list[tuple[int, int]]:
    """Runs of at least `run` samples below `floor` in a loud sine: (start, length). The ends are ignored."""
    skip = int(0.3 * RATE)
    loud = [i for i, x in enumerate(samples) if abs(x) > 0.01]
    if not loud:
        return []
    lo, hi = loud[0] + skip, loud[-1] - skip
    found: list[tuple[int, int]] = []
    start = None
    for i in range(lo, hi):
        if abs(samples[i]) < floor:
            start = i if start is None else start
        else:
            if start is not None and i - start >= run:
                found.append((start, i - start))
            start = None
    return found


class PrivateGraph:
    def __init__(self, root: Path | None = None):
        self.root = Path(
            root or tempfile.mkdtemp(prefix="bingux-pw-", dir=os.environ.get("BINGUX_TEST_TMP", "/var/tmp"))
        )
        self.runtime = self.root / "run"
        self.procs: list[subprocess.Popen] = []
        self.env = self._env()

    def _env(self) -> dict[str, str]:
        env = {
            k: v
            for k, v in os.environ.items()
            if k not in ("PIPEWIRE_REMOTE", "PULSE_SERVER", "DBUS_SESSION_BUS_ADDRESS", "WAYLAND_DISPLAY", "DISPLAY")
        }
        env.update(
            HOME=str(self.root / "home"),
            XDG_RUNTIME_DIR=str(self.runtime),
            XDG_CONFIG_HOME=str(self.root / "config"),
            XDG_DATA_HOME=str(self.root / "data"),
            XDG_STATE_HOME=str(self.root / "state"),
            XDG_CACHE_HOME=str(self.root / "cache"),
            PIPEWIRE_CONFIG_DIR=str(self.root / "pwconf"),
        )
        return env

    def start(self) -> None:
        for sub in (
            "home",
            "config",
            "data",
            "state",
            "cache",
            "run",
            "pwconf/pipewire.conf.d",
            "config/wireplumber/wireplumber.conf.d",
        ):
            (self.root / sub).mkdir(parents=True, exist_ok=True)
        self.runtime.chmod(0o700)
        share = Path("/usr/share/pipewire")
        for name in ("pipewire.conf", "client.conf", "jack.conf", "pipewire-pulse.conf"):
            if (share / name).is_file():
                shutil.copy(share / name, self.root / "pwconf" / name)
        (self.root / "pwconf/pipewire.conf.d/90-test.conf").write_text(
            "context.objects = [\n"
            f"  {{ factory = adapter args = {{ factory.name = support.null-audio-sink node.name = {SPEAKER} media.class = Audio/Sink"
            " audio.position = [ FL FR ] monitor.channel-volumes = true object.linger = true } }\n"
            f"  {{ factory = adapter args = {{ factory.name = support.null-audio-sink node.name = {MICROPHONE} media.class = Audio/Source/Virtual"
            " audio.position = [ FL FR ] monitor.passthrough = true } }\n]\n"
        )
        (self.root / "config/wireplumber/wireplumber.conf.d/90-test.conf").write_text(
            "wireplumber.profiles = { main = { support.dbus = disabled monitor.alsa = disabled monitor.alsa-midi = disabled"
            " monitor.bluez = disabled monitor.libcamera = disabled monitor.v4l2 = disabled } }\n"
        )
        self.procs.append(self._spawn(["pipewire"], "pipewire.log"))
        for _ in range(100):
            if (self.runtime / "pipewire-0").exists():
                break
            time.sleep(0.1)
        self.procs.append(self._spawn(["wireplumber"], "wireplumber.log"))
        for _ in range(100):
            if SPEAKER in self.run(["pw-link", "-i"]).stdout:
                break
            time.sleep(0.1)
        time.sleep(1.0)

    def _spawn(self, argv: list[str], log: str, env: dict[str, str] | None = None) -> subprocess.Popen:
        sink = (self.root / log).open("w")
        return subprocess.Popen(
            argv, env=env or self.env, stdout=sink, stderr=subprocess.STDOUT, start_new_session=True
        )

    def run(self, argv: list[str], env: dict[str, str] | None = None, **kwargs) -> subprocess.CompletedProcess:
        return subprocess.run(
            argv, env=env or self.env, capture_output=True, text=True, timeout=30, check=False, **kwargs
        )

    def stop(self) -> None:
        for proc in reversed(self.procs):
            proc.terminate()
        for proc in self.procs:
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
        self.procs.clear()

    # --- sound -------------------------------------------------------------------------------------------------

    def record(self, path: Path, seconds: float, target: str = SPEAKER, sink: bool = True) -> subprocess.Popen:
        argv = ["pw-record", "--rate", str(RATE), "--channels", "2", "--format", "f32"]
        if sink:
            argv += ["-P", "{ stream.capture.sink=true }"]
        return subprocess.Popen(
            argv + ["--target", target, str(path)], env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )

    def play(self, path: Path, target: str) -> subprocess.Popen:
        if target == MICROPHONE:
            # A virtual microphone is not a sink a player can pick: feed its input ports by hand.
            proc = subprocess.Popen(
                ["pw-cat", "--playback", "-P", "{ node.autoconnect=false node.name=tonesource }", str(path)],
                env=self.env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            for _ in range(100):
                if "tonesource:output_FL" in self.run(["pw-link", "-o"]).stdout:
                    break
                time.sleep(0.05)
            for side in ("FL", "FR"):
                self.run(["pw-link", f"tonesource:output_{side}", f"{MICROPHONE}:input_{side}"])
            return proc
        return subprocess.Popen(
            ["pw-cat", "--playback", "--target", target, str(path)],
            env=self.env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

    def capture(
        self, samples: list[float], play_target: str, record_target: str, sink: bool = True, between=None
    ) -> list[float]:
        """Play `samples` to `play_target` and return what `record_target` heard (left channel).

        `between(play_started)` runs while the sound plays, for changes made mid-stream."""
        work = self.root / "work"
        work.mkdir(exist_ok=True)
        source, heard = work / "play.wav", work / "heard.wav"
        wav_write(source, samples)
        recorder = self.record(heard, len(samples) / RATE + 2, record_target, sink)
        time.sleep(0.6)
        player = self.play(source, play_target)
        started = time.monotonic()
        if between:
            between(started)
        player.wait()
        time.sleep(0.4)
        recorder.terminate()
        recorder.wait()
        return wav_read(heard)[0]


class Engine:
    """The bingux-effects supervisor of one direction, running against a private graph."""

    def __init__(
        self,
        graph: PrivateGraph,
        binary: Path,
        direction: str = "output",
        device: str = SPEAKER,
        extra_env: dict[str, str] | None = None,
    ):
        self.graph, self.binary, self.direction, self.device = graph, binary, direction, device
        self.env = dict(graph.env, **(extra_env or {}))
        self.proc: subprocess.Popen | None = None
        self.config = graph.root / "config/bingux/effects"

    def write_preset(self, plugins: list[dict], name: str = "T", reload: bool = True) -> None:
        preset = {
            "format": "bingux-effects-preset",
            "version": 2,
            "name": name,
            "direction": self.direction,
            "plugins": plugins,
        }
        (self.config / self.direction / "presets").mkdir(parents=True, exist_ok=True)
        (self.config / self.direction / "presets" / f"{name}.json").write_text(json.dumps(preset))
        state_path = self.config / "state.json"
        state = json.loads(state_path.read_text()) if state_path.exists() else {}
        state[self.direction] = {"enabled": True, "preset": name, "device": self.device, "autoload": {}}
        state_path.write_text(json.dumps(state))
        if reload and self.proc is not None:
            subprocess.run([str(self.binary), "reload", self.direction], env=self.env, check=False)

    def start(self, plugins: list[dict]) -> None:
        self.write_preset(plugins, reload=False)
        log = (self.graph.root / "engine.log").open("w")
        self.proc = subprocess.Popen(
            [str(self.binary), "run", self.direction],
            env=self.env,
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        sink = "bingux_effects_output" if self.direction == "output" else "bingux_effects_input"
        for _ in range(200):
            if sink in self.graph.run(["pw-link", "-io"]).stdout:
                time.sleep(1.0)
                return
            time.sleep(0.1)
        raise RuntimeError("the effects device never appeared:\n" + (self.graph.root / "engine.log").read_text())

    def stop(self) -> None:
        if self.proc is not None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
            self.proc = None


def slot(slot_id: str, kind: str, enabled: bool = True, **controls: float) -> dict:
    return {"id": slot_id, "type": kind, "enabled": enabled, "controls": controls}


def db(x: float) -> float:
    return 10 ** (x / 20)
