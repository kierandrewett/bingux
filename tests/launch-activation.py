#!/usr/bin/env python3
"""Launch-token transport through real GIO on an isolated private D-Bus session."""

import json
import os
from pathlib import Path
import subprocess
import shutil
import shlex
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
LAUNCHER = ROOT / "shell/bingux/launch-application.py"
DESKTOP = "org.bingux.TokenFixture"
TOKEN = "fixture-token ;$(touch forbidden) ' \\"  # Opaque argv data, never shell code.
INTERFACE = """
<node><interface name="org.freedesktop.Application">
<method name="Activate"><arg type="a{sv}" direction="in"/></method>
<method name="ActivateAction"><arg type="s" direction="in"/>
<arg type="av" direction="in"/><arg type="a{sv}" direction="in"/></method>
</interface></node>
"""
NATIVE_APP = r"""
#include <gtk/gtk.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <unistd.h>

static FILE *record;
static const char *action;

static void event(const char *stage, gboolean active) {
    fprintf(record, "{\"stage\":\"%s\",\"action\":\"%s\",\"pid\":%d,"
            "\"nice\":%d,\"policy\":%d,\"active\":%s,\"monotonic_us\":%" G_GINT64_FORMAT "}\n",
            stage, action, getpid(), getpriority(PRIO_PROCESS, 0), sched_getscheduler(0),
            active ? "true" : "false", g_get_monotonic_time());
    fflush(record);
}
static void active_changed(GObject *window, GParamSpec *pspec, gpointer data) {
    (void)pspec; (void)data;
    event("active-changed", gtk_window_is_active(GTK_WINDOW(window)));
}
static void activate(GtkApplication *app, gpointer data) {
    (void)data;
    GtkWidget *window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), "bingux-activation-proof");
    gtk_window_set_default_size(GTK_WINDOW(window), 440, 180);
    gtk_window_set_child(GTK_WINDOW(window), gtk_label_new("Bingux launch activation fixture"));
    g_signal_connect(window, "notify::is-active", G_CALLBACK(active_changed), NULL);
    event("present", FALSE);
    gtk_window_present(GTK_WINDOW(window));
}
int main(int argc, char **argv) {
    if (argc != 3) return 2;
    action = g_str_equal(argv[1], "proof") ? "proof" : "default";
    record = fopen(argv[2], "a");
    if (!record) return 3;
    fprintf(record, "{\"stage\":\"environment\",\"pid\":%d,\"token_present\":%s,"
            "\"startup_present\":%s}\n", getpid(),
            g_getenv("XDG_ACTIVATION_TOKEN") ? "true" : "false",
            g_getenv("DESKTOP_STARTUP_ID") ? "true" : "false");
    event("start", FALSE);
    GtkApplication *app = gtk_application_new("dev.bingux.ActivationProbe", G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    int status = g_application_run(G_APPLICATION(app), 1, argv);
    event("exit", FALSE);
    g_object_unref(app);
    fclose(record);
    return status;
}
"""


class LaunchActivationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bingux-token-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.record = self.root / "record.json"
        self.apps = self.root / "applications"
        self.apps.mkdir()
        self.binaries = self.root / "bin"
        self.binaries.mkdir()
        self.stub("systemctl", "exit 0")
        self.stub("systemd-run", "exit 1")
        self.child = self.root / "child.py"
        self.child.write_text(
            "import json, os, sys\nfrom pathlib import Path\n"
            "Path(os.environ['BINGUX_TOKEN_RECORD']).write_text(json.dumps({"
            "'argv':sys.argv[1:], 'cwd':os.getcwd(),"
            "'token':os.environ.get('XDG_ACTIVATION_TOKEN'),"
            "'startup':os.environ.get('DESKTOP_STARTUP_ID')}))\n"
        )
        self.desktop = self.apps / f"{DESKTOP}.desktop"
        self.write_desktop()
        self.env = os.environ | {
            "HOME": str(self.root),
            "XDG_DATA_HOME": str(self.root),
            "XDG_DATA_DIRS": str(self.root),
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "XDG_CACHE_HOME": str(self.root / "cache"),
            "XDG_RUNTIME_DIR": str(self.root),
            "PATH": str(self.binaries) + os.pathsep + os.environ["PATH"],
            "BINGUX_TOKEN_RECORD": str(self.record),
        }
        for name in ("XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID", "DISPLAY", "WAYLAND_DISPLAY"):
            self.env.pop(name, None)

    def stub(self, name, body):
        path = self.binaries / name
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o700)

    def write_desktop(self, startup=False, dbus=False):
        self.desktop.write_text(
            f"[Desktop Entry]\nType=Application\nName=Token fixture\nPath={self.root}\n"
            f"StartupNotify={str(startup).lower()}\nDBusActivatable={str(dbus).lower()}\n"
            f"Exec={sys.executable} {self.child} default %k\nActions=proof;new-window;\n"
            f"[Desktop Action proof]\nName=Localized action label\n"
            f"Exec={sys.executable} {self.child} proof %k\n"
            f"[Desktop Action new-window]\nName=New window\n"
            f"Exec={sys.executable} {self.child} new-window %k\n"
        )

    def launch(self, *flags, env=None):
        self.record.unlink(missing_ok=True)
        result = subprocess.run(
            [sys.executable, str(LAUNCHER), *flags, "--", DESKTOP],
            env=env or self.env,
            capture_output=True,
            text=True,
            timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        deadline = time.monotonic() + 2
        while not self.record.exists() and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(self.record.exists(), result.stdout + result.stderr)
        return json.loads(self.record.read_text())

    def test_exec_receives_explicit_token_without_startup_notify(self):
        for flags, action in (([], "default"), (["--action=proof"], "proof"), (["--new-window"], "new-window")):
            with self.subTest(action=action):
                row = self.launch("--host-launch", f"--activation-token={TOKEN}", *flags)
                self.assertEqual(row["token"], TOKEN)
                self.assertIsNone(row["startup"])
                self.assertEqual(row["argv"], [action, str(self.desktop)])
                self.assertEqual(row["cwd"], str(self.root))
                self.assertFalse((self.root / "forbidden").exists())

    def test_tokenless_launch_cannot_inherit_activation(self):
        env = self.env | {"XDG_ACTIVATION_TOKEN": "stale-token", "DESKTOP_STARTUP_ID": "stale-startup"}
        for flags in ([], ["--action=proof"], ["--activation-token="]):
            with self.subTest(flags=flags):
                row = self.launch("--host-launch", *flags, env=env)
                self.assertIsNone(row["token"])
                self.assertIsNone(row["startup"])

    def test_explicit_token_replaces_stale_environment(self):
        env = self.env | {"XDG_ACTIVATION_TOKEN": "stale-token", "DESKTOP_STARTUP_ID": "stale-startup"}
        row = self.launch("--host-launch", f"--activation-token={TOKEN}", env=env)
        self.assertEqual(row["token"], TOKEN)
        self.assertIsNone(row["startup"])

    def test_manager_and_rejection_fallback(self):
        # Model the actual manager boundary: argv survives; per-click environment
        # does not. This is a private process, never the host user manager.
        self.stub(
            "systemd-run",
            '''while [ "$1" != -- ]; do shift; done
shift
unset XDG_ACTIVATION_TOKEN DESKTOP_STARTUP_ID
exec "$@"''',
        )
        for manager in (True, False):
            if not manager:
                self.stub("systemd-run", "exit 1")
            for mode in ("--dock-feedback", "--notify-errors"):
                with self.subTest(manager=manager, mode=mode):
                    row = self.launch(mode, "--action=proof", f"--activation-token={TOKEN}")
                    self.assertEqual(row["token"], TOKEN)
                    self.assertEqual(row["argv"][0], "proof")

    def dbus_launch(self, *flags):
        import gi

        gi.require_version("Gio", "2.0")
        from gi.repository import Gio, GLib

        connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        connection.call_sync(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            GLib.Variant("(su)", (DESKTOP, 0)),
            None,
            Gio.DBusCallFlags.NONE,
            2000,
            None,
        )
        calls = []

        def called(_connection, _sender, _path, _interface, method, parameters, invocation):
            calls.append((method, parameters.unpack()))
            invocation.return_value(GLib.Variant("()", ()))

        registration = connection.register_object(
            "/org/bingux/TokenFixture",
            Gio.DBusNodeInfo.new_for_xml(INTERFACE).interfaces[0],
            called,
            None,
            None,
        )
        process = subprocess.Popen(
            [sys.executable, str(LAUNCHER), "--host-launch", *flags, "--", DESKTOP],
            env=self.env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 6
            while time.monotonic() < deadline and (process.poll() is None or not calls):
                GLib.MainContext.default().iteration(False)
                if process.poll() not in (None, 0):
                    break
                time.sleep(0.005)
            stdout, stderr = process.communicate(timeout=1)
            self.assertEqual(process.returncode, 0, stdout + stderr)
            self.assertEqual(len(calls), 1, calls)
            self.assertFalse(self.record.exists(), "D-Bus launch fell back to Exec")
            return calls[0]
        finally:
            if process.poll() is None:
                process.kill()
                process.communicate()
            connection.unregister_object(registration)

    def test_gio_startup_notify_exec(self):
        self.write_desktop(startup=True)
        row = self.launch("--host-launch", f"--activation-token={TOKEN}")
        self.assertEqual(row["token"], TOKEN)
        self.assertEqual(row["startup"], TOKEN)

    def test_gio_dbus_startup_data(self):
        self.write_desktop(startup=True, dbus=True)
        for flags, expected in (
            ([], "Activate"),
            (["--action=proof"], "ActivateAction"),
            (["--new-window"], "ActivateAction"),
        ):
            with self.subTest(flags=flags):
                method, parameters = self.dbus_launch(f"--activation-token={TOKEN}", *flags)
                self.assertEqual(method, expected)
                self.assertEqual(parameters[-1], {"activation-token": TOKEN, "desktop-startup-id": TOKEN})
                if flags:
                    self.assertEqual(parameters[0], "proof" if flags[0] == "--action=proof" else "new-window")

    def test_gio_dbus_metadata_and_tokenless_semantics(self):
        for startup, flags in ((False, [f"--activation-token={TOKEN}"]), (True, [])):
            with self.subTest(startup=startup):
                self.write_desktop(startup=startup, dbus=True)
                self.env.update(XDG_ACTIVATION_TOKEN="stale-token", DESKTOP_STARTUP_ID="stale-startup")
                method, parameters = self.dbus_launch(*flags, "--action=proof")
                self.assertEqual((method, parameters), ("ActivateAction", ("proof", [], {})))


def native_check(binary, plugin_directory):
    """Load the actual native QML plugin without a display, bus or user config."""
    with tempfile.TemporaryDirectory(prefix="bingux-token-qml-") as temporary:
        root = Path(temporary)
        module = root / "qml/Bingux/Wayland"
        module.mkdir(parents=True)
        shutil.copy2(Path(plugin_directory) / "libbinguxwaylandsync.so", module)
        shutil.copy2(ROOT / "native/wayland-sync/qmldir", module)
        config = root / "config"
        config.mkdir()
        qml = (ROOT / "tests/launch-activation.qml").read_text()
        (config / "shell.qml").write_text(qml.split("    // Native surfaces below")[0] + "}\n")
        runtime = root / "runtime"
        runtime.mkdir(mode=0o700)
        env = {
            "PATH": os.environ["PATH"],
            "HOME": str(root),
            "XDG_CONFIG_HOME": str(root / "user-config"),
            "XDG_CACHE_HOME": str(root / "cache"),
            "XDG_DATA_HOME": str(root / "data"),
            "XDG_DATA_DIRS": str(root / "data"),
            "XDG_RUNTIME_DIR": str(runtime),
            "QT_QPA_PLATFORM": "offscreen",
            "QSG_RHI_BACKEND": "software",
            "QT_QUICK_BACKEND": "software",
            "DBUS_SESSION_BUS_ADDRESS": f"unix:path={root}/nonexistent-bus",
            "QML_IMPORT_PATH": str(root / "qml"),
            "BINGUX_ACTIVATION_OFFSCREEN": "1",
        }
        result = subprocess.run(
            [str(Path(binary).resolve()), "--path", str(config), "--no-color", "--log-rules", "qml.info=true"],
            env=env,
            capture_output=True,
            text=True,
            timeout=10,
        )
        print(result.stdout + result.stderr)
        return (
            0 if result.returncode == 0 and "BINGUX_ACTIVATION_OFFSCREEN_PASS" in result.stdout + result.stderr else 1
        )


def prepare_native(directory):
    """Prepare a private real-window fixture; never launch it or install it."""
    import gi

    gi.require_version("GLib", "2.0")
    from gi.repository import GLib

    root = Path(directory).resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    apps = root / "data/applications"
    apps.mkdir(parents=True)
    source = root / "activation-window.c"
    source.write_text(NATIVE_APP)
    binary = root / "activation-window"
    flags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "--libs", "gtk4"], text=True))
    subprocess.run(["cc", "-O2", "-Wall", "-Wextra", "-Werror", str(source), "-o", str(binary), *flags], check=True)
    shutil.copy2(LAUNCHER, root / "launch-application.py")
    shutil.copy2(ROOT / "tests/launch-activation.qml", root / "shell.qml")

    def desktop_argument(value):
        value = str(value).replace("%", "%%")
        for character in ("\\", '"', "`", "$"):
            value = value.replace(character, "\\" + character)
        return '"' + value + '"'

    entry = GLib.KeyFile()
    for key, value in {"Type": "Application", "Name": "Bingux activation fixture", "Actions": "proof;"}.items():
        entry.set_string("Desktop Entry", key, value)
    for group, action in (("Desktop Entry", "default"), ("Desktop Action proof", "proof")):
        entry.set_string(
            group, "Exec", " ".join(desktop_argument(value) for value in (binary, action, root / "windows.jsonl"))
        )
    entry.set_string("Desktop Action proof", "Name", "Open activation proof")
    (apps / "dev.bingux.ActivationProbe.desktop").write_text(entry.to_data()[0])
    run = root / "run-native.sh"
    run.write_text(
        "#!/bin/sh\nset -eu\n"
        'if [ "$#" != 2 ]; then echo "usage: run-native.sh RAW_QUICKSHELL QML_IMPORT_DIRECTORY" >&2; exit 2; fi\n'
        f"fixture_dir={shlex.quote(str(root))}\n"
        'exec env XDG_CONFIG_HOME="$fixture_dir/config" XDG_CACHE_HOME="$fixture_dir/cache" '
        'XDG_DATA_HOME="$fixture_dir/data" XDG_DATA_DIRS="$fixture_dir/data" '
        'QML_IMPORT_PATH="$2" "$1" --path "$fixture_dir/shell.qml" --no-color --log-rules "qml.info=true"\n'
    )
    run.chmod(0o700)
    print(f"Prepared {root}; no application was started. See tests/launch-activation.md for native use.")
    return 0


def main():
    if len(sys.argv) == 4 and sys.argv[1] == "--native-check":
        return native_check(sys.argv[2], sys.argv[3])
    if len(sys.argv) == 3 and sys.argv[1] == "--prepare-native":
        return prepare_native(sys.argv[2])
    if "--private-bus" not in sys.argv:
        env = dict(os.environ)
        for name in (
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_ACTIVATION_TOKEN",
            "DESKTOP_STARTUP_ID",
        ):
            env.pop(name, None)
        return subprocess.run(
            ["dbus-run-session", "--", sys.executable, __file__, "--private-bus", *sys.argv[1:]],
            env=env,
            timeout=90,
        ).returncode
    sys.argv.remove("--private-bus")
    unittest.main()


if __name__ == "__main__":
    raise SystemExit(main())
