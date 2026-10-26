#!/usr/bin/env python3
"""Run Bingux's real bsm integration pieces in headless Sway against fakes.

What runs for real: shell/bingux/Wallpaper.qml, BsmMode, BsmSession,
BsmReadiness, BsmSurfaceProbe, BsmDesktop and SessionPower (the production
files, copied unchanged), the Loader that shell.qml uses to start BsmDesktop
(spliced out of shell.qml), Quickshell, Qt and real Wayland layer-shell
windows in a headless Sway. What is faked: the bridge's ready.sock
(tests/bsm-fake-ready.py, which checks every line against the wire format),
`bsmctl` and `gnoblinctl` (scripts on PATH that log their command line and
answer from files this test writes), and the shell's own bar, dock, theme and
preferences, which are small stand-ins (the real ones need a full Bingux
build). The stand-in bar and dock are real PanelWindows with the production
namespaces, so frames are really swapped for them.

Checks, each in its own shell process set:
 1. integrated, dock enabled: wallpaper and desktop report; the desktop is not
    ready before the delayed dock has rendered; the wallpaper with a picture is
    not ready before the picture has faded in; nothing violates the wire format
 2. integrated, dock disabled: ready without the dock; the dock is never reported
 3. no socket, then a socket: one warning, no retry storm, full report on
    connect, full re-report after the connection is dropped
 4. bad BSM_GENERATION: no connection, one warning, no crash
 5. power actions through fake bsmctl: command line, no gnoblinctl, no black
    overlay, pending until bsm's Completed, reason code on Failed, immediate
    refusal, notification on a failed lock, monitor loss, Switch User offered
 6. standalone (BSM_SESSION_ID unset): no socket connection, no bsmctl, the
    gnoblinctl command runs, the local fade turns the screen black

Needs sway, grim, Pillow and a Quickshell binary (QS_TEST_BIN, default qs).
"""

import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SHELL = ROOT / "shell/bingux"
QS = os.environ.get("QS_TEST_BIN", "qs")
BACKGROUND = (0x33, 0x66, 0x99)
GENERATION = "4242"

spec = importlib.util.spec_from_file_location("bsm_fake_ready", ROOT / "tests/bsm-fake-ready.py")
fake_ready = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fake_ready)

COPIED = [
    "BsmMode.qml",
    "BsmSession.qml",
    "BsmSession.js",
    "BsmReadiness.qml",
    "BsmReadiness.js",
    "BsmSurfaceProbe.qml",
    "BsmDesktop.qml",
    "SessionPower.qml",
    "Wallpaper.qml",
]

STUBS = {
    "BinguxPreferences.qml": """pragma Singleton
import QtQuick
import Quickshell
QtObject {
    property bool loaded: true
    property url wallpaperUrl: Quickshell.env("BSMTEST_WALLPAPER") || ""
}
""",
    "Theme.qml": """pragma Singleton
import QtQuick
QtObject {
    property bool reducedMotion: false
    property color text: "white"
    property color muted: "#aaaaaa"
    property string fontFamily: "sans"
    property int fontHeading: 16
    property int fontSmall: 12
}
""",
    "SessionContext.qml": """pragma Singleton
import QtQuick
QtObject {
    property bool continuing: false
}
""",
    "SessionServices.qml": """pragma Singleton
import QtQuick
QtObject {
    function allows(capability) {
        return true;
    }
}
""",
    "ShellNotifications.qml": """pragma Singleton
import QtQuick
QtObject {
    function send(title, body) {
        console.info("BSMTEST_NOTIFY " + title + " | " + body);
    }
}
""",
    "ControlRow.qml": """import QtQuick
Item {
    property string title
    property string iconName
    property bool navigation
    signal clicked
    implicitWidth: 100
    implicitHeight: 20
}
""",
}

QMLDIR = """singleton BinguxPreferences 1.0 BinguxPreferences.qml
singleton Theme 1.0 Theme.qml
singleton SessionContext 1.0 SessionContext.qml
singleton SessionServices 1.0 SessionServices.qml
singleton ShellNotifications 1.0 ShellNotifications.qml
singleton BsmMode 1.0 BsmMode.qml
singleton BsmSession 1.0 BsmSession.qml
singleton BsmReadiness 1.0 BsmReadiness.qml
BsmSurfaceProbe 1.0 BsmSurfaceProbe.qml
BsmDesktop 1.0 BsmDesktop.qml
ControlRow 1.0 ControlRow.qml
SessionPower 1.0 SessionPower.qml
Wallpaper 1.0 Wallpaper.qml
"""

WALLPAPER_SHELL = """//@ pragma UseQApplication
import QtQuick
import Quickshell
ShellRoot {
    Wallpaper {}
}
"""

DESKTOP_SHELL = """//@ pragma UseQApplication
import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    id: root
    property bool dockShown: false
    QtObject {
        id: profileSettings
        property bool dockEnabled: Quickshell.env("BSMTEST_DOCK") === "1"
    }
    Timer {
        interval: parseInt(Quickshell.env("BSMTEST_DOCK_DELAY") || "0")
        running: true
        onTriggered: root.dockShown = true
    }
    PanelWindow {
        id: topBar
        implicitHeight: 32
        color: "#202020"
        anchors {
            top: true
            left: true
            right: true
        }
        WlrLayershell.namespace: "bingux-top-bar"
        SessionPower {
            id: power
        }
    }
    PanelWindow {
        id: dock
        visible: root.dockShown && profileSettings.dockEnabled
        implicitHeight: 48
        color: "#303030"
        anchors {
            bottom: true
            left: true
            right: true
        }
        WlrLayershell.namespace: "bingux-dock"
    }
    // Spliced from the production shell.qml.
    @LOADER@
    Connections {
        target: BsmSession
        function onTrackerChanged() {
            const op = BsmSession.operation;
            console.info("BSMTEST_STATUS " + BsmSession.status + " " + (op ? op.phase + " " + op.action : "-"));
        }
    }
    Connections {
        target: power
        function onErrorChanged() {
            console.info("BSMTEST_ERROR " + power.error);
        }
        function onPendingChanged() {
            console.info("BSMTEST_PENDING " + power.pending);
        }
    }
    IpcHandler {
        target: "bsmtest"
        function act(action: string): void {
            power.pending = action;
            power.execute();
        }
        function request(action: string): void {
            console.info("BSMTEST_REQUEST " + BsmSession.request(action, {
                notify: true
            }));
        }
        function actions(): void {
            console.info("BSMTEST_ACTIONS " + power.actions.map(a => a.id).join(","));
        }
        function state(): void {
            console.info("BSMTEST_STATE busy=" + power.busy + " transitioning=" + power.transitioning + " blackout=" + power.blackout + " integrated=" + power.integrated);
        }
    }
}
"""

FAKE_BSMCTL = r"""#!/usr/bin/env python3
import json, os, sys, time
directory = os.environ["BSMTEST_DIR"]
with open(directory + "/calls.log", "a") as log:
    log.write(json.dumps(["bsmctl"] + sys.argv[1:]) + "\n")
if sys.argv[1:] == ["monitor", "--json"]:
    with open(directory + "/monitor.fifo") as fifo:
        for line in fifo:
            sys.stdout.write(line)
            sys.stdout.flush()
    sys.exit(0)
answers = {}
try:
    answers = json.load(open(directory + "/answers.json"))
except OSError:
    pass
answer = answers.get(sys.argv[1], {"exit": 0, "stdout": '{"operation_id": 1}'})
sys.stdout.write(answer["stdout"] + "\n")
sys.exit(answer["exit"])
"""

FAKE_GNOBLINCTL = r"""#!/usr/bin/env python3
import json, os, sys, time
with open(os.environ["BSMTEST_DIR"] + "/calls.log", "a") as log:
    log.write(json.dumps(["gnoblinctl"] + sys.argv[1:]) + "\n")
time.sleep(30)
"""


def brace_block(text, start):
    depth = 0
    index = text.index("{", start)
    while True:
        if text[index] == "{":
            depth += 1
        elif text[index] == "}":
            depth -= 1
            if depth == 0:
                return text[start : index + 1]
        index += 1


def shell_loader():
    """The Loader block shell.qml uses to start BsmDesktop, verbatim."""
    text = (SHELL / "shell.qml").read_text()
    start = text.index('    Loader {\n        active: BsmMode.integrated\n        source: "BsmDesktop.qml"')
    return brace_block(text, start + 4)


class Session:
    """One headless Sway plus a directory of fakes shared by the shells started in it."""

    def __init__(self):
        self.home = Path(tempfile.mkdtemp(prefix="bsm-shell-integration-"))
        self.runtime = self.home / "run"
        self.runtime.mkdir(mode=0o700)
        self.fake = self.home / "fake"
        self.fake.mkdir()
        self.bin = self.home / "bin"
        self.bin.mkdir()
        for name, text in (("bsmctl", FAKE_BSMCTL), ("gnoblinctl", FAKE_GNOBLINCTL)):
            (self.bin / name).write_text(text)
            (self.bin / name).chmod(0o755)
        (self.home / "sway.conf").write_text("output HEADLESS-1 resolution 1280x800 bg #336699 solid_color\n")
        self.environment = {
            key: value
            for key, value in os.environ.items()
            if key not in ("WAYLAND_DISPLAY", "DISPLAY", "BSM_SESSION_ID", "BSM_GENERATION")
        } | {
            "HOME": str(self.home),
            "XDG_RUNTIME_DIR": str(self.runtime),
            "WLR_BACKENDS": "headless",
            "WLR_RENDERER": os.environ.get("WLR_RENDERER", "pixman"),
            "WLR_LIBINPUT_NO_DEVICES": "1",
            "QT_QPA_PLATFORM": "wayland",
            "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
            "BSMTEST_DIR": str(self.fake),
        }
        self.sway = subprocess.Popen(
            ["sway", "-c", str(self.home / "sway.conf")],
            env=self.environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        for _ in range(100):
            sockets = [p for p in self.runtime.glob("wayland-*") if not p.name.endswith(".lock")]
            if sockets:
                self.environment["WAYLAND_DISPLAY"] = sockets[0].name
                break
            time.sleep(0.1)
        else:
            raise SystemExit("headless sway did not start")
        self.shells = []

    def close(self):
        for shell in self.shells:
            shell.stop()
        self.sway.terminate()
        self.sway.wait(10)
        shutil.rmtree(self.home, ignore_errors=True)

    def reset_fakes(self):
        for name in ("calls.log", "answers.json"):
            (self.fake / name).unlink(missing_ok=True)
        fifo = self.fake / "monitor.fifo"
        fifo.unlink(missing_ok=True)
        os.mkfifo(fifo)

    def calls(self):
        path = self.fake / "calls.log"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def answers(self, mapping):
        (self.fake / "answers.json").write_text(json.dumps(mapping))

    def fixture(self, kind, name):
        directory = self.home / name
        directory.mkdir()
        for file in COPIED:
            shutil.copy2(SHELL / file, directory / file)
        for file, text in STUBS.items():
            (directory / file).write_text(text)
        (directory / "qmldir").write_text(QMLDIR)
        if kind == "wallpaper":
            (directory / "shell.qml").write_text(WALLPAPER_SHELL)
        else:
            (directory / "shell.qml").write_text(DESKTOP_SHELL.replace("@LOADER@", shell_loader()))
        return directory

    def screenshot(self):
        path = self.home / "screen.png"
        subprocess.run(["grim", "-o", "HEADLESS-1", str(path)], env=self.environment, check=True, timeout=20)
        with Image.open(path) as image:
            rgb = image.convert("RGB")
            return rgb.getpixel((rgb.width // 2, rgb.height // 2))

    def start(self, kind, name, extra=None):
        shell = Shell(self, self.fixture(kind, name), extra or {})
        self.shells.append(shell)
        return shell


class Shell:
    def __init__(self, session, directory, extra):
        self.session = session
        self.directory = directory
        self.started = time.monotonic()
        self.lines = []
        self.process = subprocess.Popen(
            [QS, "-p", str(directory)],
            env=session.environment | extra,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.process.stdout:
            self.lines.append((time.monotonic(), line.rstrip("\n")))

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(10)
            except subprocess.TimeoutExpired:
                self.process.kill()

    def alive(self):
        return self.process.poll() is None

    def log(self, pattern):
        return [line for _, line in self.lines if re.search(pattern, line)]

    def wait_log(self, pattern, timeout=15):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            found = self.log(pattern)
            if found:
                return found
            time.sleep(0.05)
        raise AssertionError(
            "no log line matching %r in %.0fs; last lines:\n%s"
            % (pattern, timeout, "\n".join(text for _, text in self.lines[-15:]))
        )

    def ipc(self, *arguments):
        result = subprocess.run(
            [QS, "-p", str(self.directory), "ipc", "call", "bsmtest", *arguments],
            env=self.session.environment,
            capture_output=True,
            text=True,
            timeout=20,
        )
        assert result.returncode == 0, result.stderr
        return result.stdout


def kinds(events, component=None):
    return [e["event"] for _, _, e in events if component is None or e["component"] == component]


def first(events, component, event, **fields):
    for when, number, e in events:
        if e["component"] == component and e["event"] == event and all(e.get(k) == v for k, v in fields.items()):
            return when
    return None


def make_picture(path):
    Image.new("RGB", (64, 64), (200, 40, 40)).save(path)
    return path.as_uri()


def check_integrated_ready(session, results):
    server = fake_ready.FakeReadySocket(session.runtime / "bsm/ready.sock", GENERATION)
    server.start()
    picture = make_picture(session.home / "wallpaper.png")
    environment = {"BSM_SESSION_ID": "s0000000a", "BSM_GENERATION": GENERATION}
    wallpaper = session.start("wallpaper", "wallpaper-1", environment | {"BSMTEST_WALLPAPER": picture})
    desktop = session.start("desktop", "desktop-1", environment | {"BSMTEST_DOCK": "1", "BSMTEST_DOCK_DELAY": "2000"})
    try:
        events = server.wait_for(
            lambda e: kinds(e, "wallpaper").count("ready") and kinds(e, "desktop").count("ready"), 40
        )
        assert events is not None, "wallpaper and desktop never reported ready: %r\n%s" % (
            [e for _, _, e in server.snapshot()],
            "\n".join(text for s in (wallpaper, desktop) for _, text in s.lines[-10:]),
        )
        assert wallpaper.alive() and desktop.alive()
        assert not server.violations, server.violations
        assert server.connections == 2, "one connection per component, got %d" % server.connections
        desktop_events = [e for e in events if e[2]["component"] == "desktop"]
        # Unmapped surfaces are announced with launched; each mapped one follows.
        assert kinds(desktop_events) == ["launched"] + ["surface"] * 4 + ["ready"], [
            (round(w - desktop.started, 2), e) for w, _, e in desktop_events
        ]
        launched = desktop_events[0][2]
        assert launched["required"] == ["bar", "dock"], launched
        surfaces = [(e["role"], e["mapped"]) for _, _, e in desktop_events if e["event"] == "surface"]
        assert surfaces[:2] == [("bar", False), ("dock", False)] and set(surfaces[2:]) == {
            ("bar", True),
            ("dock", True),
        }, surfaces
        assert all(e["output"] == "HEADLESS-1" for _, _, e in desktop_events if e["event"] == "surface")
        t_ready = first(events, "desktop", "ready")
        t_dock = first(events, "desktop", "surface", role="dock", mapped=True)
        t_bar = first(events, "desktop", "surface", role="bar", mapped=True)
        assert t_ready >= t_dock >= t_bar
        # The dock window did not exist for 2 s; ready cannot come earlier than
        # its first frame. Startup begins before the first shell line, so allow
        # only a small margin for process start.
        assert t_ready - desktop.started >= 1.8, "ready after only %.2fs: not waiting for the dock" % (
            t_ready - desktop.started
        )
        assert t_ready - t_bar >= 1.0, "ready came %.2fs after the bar's frame; the dock should have held it back" % (
            t_ready - t_bar
        )
        wallpaper_events = [e for e in events if e[2]["component"] == "wallpaper"]
        assert kinds(wallpaper_events) == ["launched", "surface", "surface", "ready"], kinds(wallpaper_events)
        assert wallpaper_events[0][2]["required"] == ["wallpaper"]
        t_launched = first(events, "wallpaper", "launched")
        t_wall = first(events, "wallpaper", "surface", role="wallpaper", mapped=True)
        assert t_wall - t_launched >= 0.3, (
            "the wallpaper was reported %.2fs after launch: it did not wait for the picture to fade in"
            % (t_wall - t_launched)
        )
        results["integrated ready: wallpaper after picture fade-in, desktop after delayed dock"] = (
            "desktop ready %.2fs after start (bar frame at %.2fs, dock window shown at 2.00s); "
            "wallpaper frame %.2fs after launched"
            % (t_ready - desktop.started, t_bar - desktop.started, t_wall - t_launched)
        )
    finally:
        server.stop()
        wallpaper.stop()
        desktop.stop()


def check_dock_disabled(session, results):
    server = fake_ready.FakeReadySocket(session.runtime / "bsm/ready.sock", GENERATION)
    server.start()
    desktop = session.start(
        "desktop", "desktop-2", {"BSM_SESSION_ID": "s1", "BSM_GENERATION": GENERATION, "BSMTEST_DOCK": "0"}
    )
    try:
        events = server.wait_for(lambda e: "ready" in kinds(e), 30)
        assert events is not None, "no ready; log:\n" + "\n".join(text for _, text in desktop.lines[-10:])
        time.sleep(1.0)
        events = server.snapshot()
        assert not server.violations, server.violations
        assert kinds(events) == ["launched", "surface", "surface", "ready"], kinds(events)
        assert events[0][2]["required"] == ["bar"]
        assert [(e["role"], e["mapped"]) for _, _, e in events if e["event"] == "surface"] == [
            ("bar", False),
            ("bar", True),
        ]
        results["integrated, dock disabled: ready without the dock, dock never reported"] = "ok"
    finally:
        server.stop()
        desktop.stop()


def check_socket_lifecycle(session, results):
    path = session.runtime / "bsm/ready.sock"
    environment = {"BSM_SESSION_ID": "s2", "BSM_GENERATION": GENERATION, "BSMTEST_DOCK": "0"}
    desktop = session.start("desktop", "desktop-3", environment)
    server = fake_ready.FakeReadySocket(path, GENERATION)
    try:
        time.sleep(6)
        assert desktop.alive(), "the shell died without a socket"
        warnings = desktop.log(r"cannot reach")
        assert len(warnings) == 1, "expected one warning without a socket, got %d: %r" % (len(warnings), warnings)
        server.start()
        events = server.wait_for(lambda e: "ready" in kinds(e), 30)
        assert events is not None, "no report after the socket appeared; log:\n" + "\n".join(
            text for _, text in desktop.lines[-10:]
        )
        # The bar had rendered long before the socket existed: it is reported mapped at once.
        assert kinds(events) == ["launched", "surface", "ready"], kinds(events)
        assert events[1][2]["mapped"] is True
        before = len(events)
        server.drop_connections()
        events = server.wait_for(lambda e: len(e) >= before + 3, 30)
        assert events is not None, "no full re-report after the connection dropped: %r" % kinds(server.snapshot())
        # The surface was already rendered: the new connection reports it mapped at once.
        assert kinds(events[before:]) == ["launched", "surface", "ready"], kinds(events[before:])
        assert events[before][1] == 2, "re-report came on a new connection"
        assert not server.violations, server.violations
        assert len(desktop.log(r"cannot reach")) <= 3, "retry storm in the log"
        results["no socket: one warning, no crash; full report on connect and after a dropped connection"] = "ok"
    finally:
        server.stop()
        desktop.stop()


def check_bad_generation(session, results):
    server = fake_ready.FakeReadySocket(session.runtime / "bsm/ready.sock", GENERATION)
    server.start()
    desktop = session.start(
        "desktop", "desktop-4", {"BSM_SESSION_ID": "s3", "BSM_GENERATION": "abc", "BSMTEST_DOCK": "0"}
    )
    try:
        time.sleep(5)
        assert desktop.alive()
        assert server.connections == 0, "connected with an invalid generation"
        assert len(desktop.log(r"BSM_GENERATION is missing")) == 1
        results["invalid BSM_GENERATION: no connection, one warning"] = "ok"
    finally:
        server.stop()
        desktop.stop()


def feed(session, *events):
    """Write monitor events to the fake `bsmctl monitor`'s fifo."""
    for event in events:
        session.monitor.write((json.dumps(event) + "\n").encode())


def transition(operation, phase, reason=None, kind="power_fade"):
    return {"event": "transition", "operation_id": operation, "kind": kind, "phase": phase, "reason": reason}


def check_power_actions(session, results):
    session.reset_fakes()
    session.monitor = os.fdopen(os.open(session.fake / "monitor.fifo", os.O_RDWR), "wb", buffering=0)
    environment = {"BSM_SESSION_ID": "s4", "BSM_GENERATION": GENERATION, "BSMTEST_DOCK": "0"}
    # No ready socket here: the warning is expected and irrelevant.
    desktop = session.start("desktop", "desktop-5", environment)
    try:
        desktop.wait_log(r"BSMTEST_\w+|Configuration Loaded")
        deadline = time.monotonic() + 15
        while ["bsmctl", "monitor", "--json"] not in session.calls():
            assert time.monotonic() < deadline, "the desktop never started `bsmctl monitor --json`"
            time.sleep(0.1)
        desktop.ipc("actions")
        assert desktop.wait_log(r"BSMTEST_ACTIONS")[-1].endswith("suspend,logout,reboot,poweroff,switch-user")

        # Accepted, never completed until bsm says so.
        session.answers({"reboot": {"exit": 0, "stdout": '{"operation_id": 17}'}})
        desktop.ipc("act", "reboot")
        desktop.wait_log(r"BSMTEST_STATUS pending")
        assert ["bsmctl", "reboot", "--json"] in session.calls()
        assert not [c for c in session.calls() if c[0] == "gnoblinctl"], "gnoblinctl ran in integrated mode"
        feed(
            session,
            transition(17, "requested"),
            transition(17, "authorising"),
            transition(17, "preparing"),
            transition(17, "animating"),
        )
        desktop.wait_log(r"BSMTEST_STATUS pending animating reboot")
        time.sleep(1.0)
        assert not desktop.log(r"BSMTEST_STATUS completed"), "completed without bsm saying so"
        desktop.ipc("state")
        state = desktop.wait_log(r"BSMTEST_STATE")[-1]
        assert (
            "busy=true" in state
            and "transitioning=false" in state
            and "blackout=0" in state
            and "integrated=true" in state
        ), state
        colour = session.screenshot()
        assert colour == BACKGROUND, "the screen is %r, not the untouched background: a local overlay was drawn" % (
            colour,
        )
        feed(session, transition(17, "committed"))
        desktop.wait_log(r"BSMTEST_STATUS pending committed reboot")
        feed(session, transition(17, "completed"))
        desktop.wait_log(r"BSMTEST_STATUS completed")
        desktop.wait_log(r"BSMTEST_PENDING $")

        # Failed: the reason code is shown.
        session.answers({"poweroff": {"exit": 0, "stdout": '{"operation_id": 18}'}})
        desktop.ipc("act", "poweroff")
        desktop.wait_log(
            r"BSMTEST_STATUS pending requested poweroff|BSMTEST_STATUS pending . poweroff|BSMTEST_STATUS pending"
        )
        feed(session, transition(18, "preparing"), transition(18, "failed", "inhibited"))
        error = desktop.wait_log(r"BSMTEST_ERROR .*inhibited")[-1]
        assert "(inhibited)" in error and "blocking" in error, error
        assert session.screenshot() == BACKGROUND

        # Refused at once.
        session.answers({"suspend": {"exit": 1, "stdout": '{"error":"unauthorised","message":"no"}'}})
        desktop.ipc("act", "suspend")
        error = desktop.wait_log(r"BSMTEST_ERROR .*unauthorised")[-1]
        assert "(unauthorised)" in error, error

        # Lock through the shared client; a failure becomes a notification.
        session.answers({"lock": {"exit": 1, "stdout": '{"error":"already_locked"}'}})
        desktop.ipc("request", "lock")
        note = desktop.wait_log(r"BSMTEST_NOTIFY")[-1]
        assert "Could not lock the session" in note and "(already_locked)" in note, note
        assert ["bsmctl", "lock", "--json"] in session.calls()

        # The monitor ending while an operation is open fails it visibly.
        session.answers({"logout": {"exit": 0, "stdout": '{"operation_id": 19}'}})
        desktop.ipc("act", "logout")
        desktop.wait_log(r"BSMTEST_STATUS pending . logout|BSMTEST_STATUS pending")
        session.monitor.close()
        error = desktop.wait_log(r"BSMTEST_ERROR .*monitor_lost", 20)[-1]
        assert "(monitor_lost)" in error, error
        assert desktop.alive()
        results[
            "power actions: fake bsmctl command lines, pending until Completed, reason codes, no overlay, no gnoblinctl"
        ] = "ok"
    finally:
        desktop.stop()


def check_standalone(session, results):
    session.reset_fakes()
    server = fake_ready.FakeReadySocket(session.runtime / "bsm/ready.sock", GENERATION)
    server.start()
    picture = make_picture(session.home / "wallpaper-standalone.png")
    # BSM_SESSION_ID is unset; a stray BSM_GENERATION alone must not matter.
    environment = {"BSM_GENERATION": GENERATION, "BSMTEST_DOCK": "1", "BSMTEST_DOCK_DELAY": "200"}
    wallpaper = session.start("wallpaper", "wallpaper-s", environment | {"BSMTEST_WALLPAPER": picture})
    desktop = session.start("desktop", "desktop-s", environment)
    try:
        desktop.wait_log(r"Configuration Loaded")
        wallpaper.wait_log(r"Configuration Loaded")
        time.sleep(4)
        assert server.connections == 0, "a standalone shell connected to ready.sock"
        assert not (session.fake / "calls.log").exists(), "a standalone shell ran %r" % session.calls()
        assert not desktop.log(r"\[bsm\]") and not wallpaper.log(r"\[bsm\]"), "bsm log lines in a standalone shell"
        desktop.ipc("actions")
        assert desktop.wait_log(r"BSMTEST_ACTIONS")[-1].endswith("suspend,logout,reboot,poweroff"), desktop.log(
            r"BSMTEST_ACTIONS"
        )
        desktop.ipc("act", "suspend")
        deadline = time.monotonic() + 10
        while ["gnoblinctl", "session", "suspend"] not in session.calls():
            assert time.monotonic() < deadline, "the standalone path did not run gnoblinctl: %r" % session.calls()
            time.sleep(0.1)
        time.sleep(0.5)
        assert not [c for c in session.calls() if c[0] == "bsmctl"]
        desktop.ipc("state")
        state = desktop.wait_log(r"BSMTEST_STATE")[-1]
        assert "transitioning=true" in state and "integrated=false" in state, state
        colour = session.screenshot()
        assert colour == (0, 0, 0), "the standalone fade did not black out the screen: %r" % (colour,)
        results["standalone: no socket, no bsmctl; gnoblinctl runs and the local fade covers the screen"] = "ok"
    finally:
        server.stop()
        wallpaper.stop()
        desktop.stop()


def main():
    for tool in ("sway", "grim", QS):
        if shutil.which(tool) is None:
            print("missing tool: " + tool)
            return 1
    loader = shell_loader()
    assert "active: BsmMode.integrated" in loader and "item.start()" in loader, loader
    session = Session()
    results = {}
    try:
        for check in (
            check_integrated_ready,
            check_dock_disabled,
            check_socket_lifecycle,
            check_bad_generation,
            check_power_actions,
            check_standalone,
        ):
            check(session, results)
            print("PASS " + list(results)[-1], flush=True)
    finally:
        session.close()
    print(json.dumps(results, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
