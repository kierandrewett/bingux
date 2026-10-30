#!/usr/bin/env python3
"""Exercise direct Calendar/Settings launch paths without a display or user manager."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT / "shell/bingux"
# The fake syscall boundary makes the negative-nice regression deterministic on
# unprivileged hosts. Real-child nice/policy are recorded separately, never faked.
SITECUSTOMIZE = r"""
import os
if "BINGUX_TEST_NICE" in os.environ:
    def getpriority(which, who):
        assert which == os.PRIO_PROCESS and who == 0
        return int(os.environ["BINGUX_TEST_NICE"])
    def setpriority(which, who, value):
        assert which == os.PRIO_PROCESS and who == 0 and value == 0
        if os.environ.get("BINGUX_TEST_DENIED") == "1":
            raise PermissionError("fixture denied priority reset")
        os.environ["BINGUX_TEST_NICE"] = str(value)
    os.getpriority = getpriority
    os.setpriority = setpriority
"""
CHILD = r"""
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <unistd.h>
static void quoted(FILE *out, const char *s) {
    fputc('"', out);
    for (; *s; s++) {
        unsigned char c = *s;
        if (c == '"' || c == '\\') fprintf(out, "\\%c", c);
        else if (c < 32) fprintf(out, "\\u%04x", c);
        else fputc(c, out);
    }
    fputc('"', out);
}
int main(int argc, char **argv) {
    FILE *out = fopen(getenv("BINGUX_TEST_RECORD"), "a");
    if (!out) return 2;
    char cwd[4096];
    fprintf(out, "{\"nice\":%d,\"policy\":%d,\"simulated_nice\":",
            getpriority(PRIO_PROCESS, 0), sched_getscheduler(0));
    quoted(out, getenv("BINGUX_TEST_NICE") ?: "");
    fputs(",\"environment\":", out);
    quoted(out, getenv("BINGUX_TEST_VALUE") ?: "");
    fputs(",\"cwd\":", out); quoted(out, getcwd(cwd, sizeof(cwd)) ?: "");
    fputs(",\"argv\":[", out);
    for (int i=1; i<argc; i++) { if(i>1) fputc(',',out); quoted(out,argv[i]); }
    fputs("]}\n",out);
    fclose(out);
    return 0;
}
"""
# Execute production QML JavaScript handlers. Only UI state and detached exec
# are substituted: the resulting argv is then run against private C binaries.
QML_COMMANDS = r"""
const fs = require("fs"), vm = require("vm"), url = require("url");
const shell = process.argv[1], commands = [];
const app = fs.readFileSync(shell+"/ApplicationLauncher.qml", "utf8");
const control = fs.readFileSync(shell+"/ControlCentre.qml", "utf8");
function method(source, name) {
    const match = source.match(new RegExp("^    function "+name+"\\([^\\n]*\\) \\{\\n.*?^    \\}", "ms"));
    if (!match) throw new Error("Missing handler "+name);
    return match[0];
}
const root = {desktopId:"org.gnome.Calendar", pendingCommand:[], attempts:0, windows:[], nativeWindows:[]};
const context = {
    root, Qt: {resolvedUrl: name => url.pathToFileURL(shell+"/"+name)},
    Quickshell: {execDetached: argv => commands.push(Array.from(argv))},
    focusRetry: {stop(){},start(){}}, launchStarted(){}, stop(){},
    compositor: {nativeProtocol:true},
};
// QML id properties resolve unqualified names to the object.
for (const name of ["pendingCommand", "attempts"]) Object.defineProperty(context,name,{
    get:()=>root[name], set:value=>root[name]=value
});
vm.createContext(context);
for (const match of app.matchAll(/^    readonly property var (\w+): (.*)$/gm)) {
    root[match[1]] = context[match[1]] = vm.runInContext(match[2], context);
}
vm.runInContext(method(control,"settings"),context);
context.settings("network; $(touch forbidden) ' \\");
vm.runInContext(method(app,"launch"),context);
context.launch(["gnome-calendar"],[]);
context.launch(["gnome-calendar", "--uuid", "source:event;$(touch forbidden)"],["gnome-calendar","--date","10/13/2026"]);
root.nativeWindows = [{appId:"org.gnome.Calendar",focused:true}];
const callback=app.match(/        onTriggered: \{\n(.*?)^        \}/ms);
if(!callback) throw new Error("Missing follow-up handler");
vm.runInContext("(function(){"+callback[1]+"})()",context);
console.log(JSON.stringify(commands));
"""
CALENDAR = r"""
import importlib.util, sys
from pathlib import Path
shell=Path(sys.argv[1]); sys.path.insert(0,str(shell))
spec=importlib.util.spec_from_file_location("calendar_launch",shell/"calendar-launch.py")
module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
class Reply:
    def unpack(self): return (True,)
class Bus:
    def call_sync(self,*args): return Reply()
module.Gio.bus_get_sync=lambda *args: Bus()
module.calendar_locale=lambda bus: "C"
module.default_calendar=lambda: None
sys.argv=["calendar-launch.py", "--date", "2026-10-13", "--uuid", "source:event;$(touch forbidden)"]
module.main()
"""


def run(argv, env, cwd, timeout=6):
    return subprocess.run(argv, env=env, cwd=cwd, capture_output=True, text=True, timeout=timeout)


def records(path, count):
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        try:
            rows = [json.loads(line) for line in path.read_text().splitlines()]
            if len(rows) == count:
                return rows
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.01)
    raise AssertionError(f"expected {count} child records: {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-boost", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.require_boost:
        for _ in range(25):
            if os.getpriority(os.PRIO_PROCESS, 0) < 0:
                break
            time.sleep(0.1)
    before = {"nice": os.getpriority(os.PRIO_PROCESS, 0), "policy": os.sched_getscheduler(0)}
    if args.require_boost and before["nice"] >= 0:
        print("NOT CHECKED: negative-parent inheritance unavailable", file=sys.stderr)
        return 77
    # No user-manager or GUI calls: a private PATH replaces both application
    # executables, and the Calendar D-Bus readiness/locale boundary is stubbed.
    with tempfile.TemporaryDirectory(prefix="bingux-direct-launch-") as temporary:
        root = Path(temporary)
        (root / "child.c").write_text(CHILD)
        subprocess.run(["cc", "-O2", "-Wall", "-Werror", str(root / "child.c"), "-o", str(root / "child")], check=True)
        for name in ("gnome-calendar", "gnome-control-center"):
            (root / name).symlink_to(root / "child")
        (root / "sitecustomize.py").write_text(SITECUSTOMIZE)
        qml = run(["node", "-e", QML_COMMANDS, str(SHELL)], os.environ, root)
        assert qml.returncode == 0, qml.stderr
        commands = json.loads(qml.stdout)
        assert len(commands) == 4, commands
        record = root / "record.jsonl"
        env = os.environ | {
            "PATH": str(root) + os.pathsep + os.environ["PATH"],
            "PYTHONPATH": str(root),
            "PYTHONDONTWRITEBYTECODE": "1",
            "BINGUX_TEST_RECORD": str(record),
            "BINGUX_TEST_VALUE": "literal $value ' \\",
        }
        expected = [
            ["network; $(touch forbidden) ' \\"],
            [],
            ["--date", "10/13/2026"],
            ["--uuid", "source:event;$(touch forbidden)"],
            ["--date", "10/13/2026"],
            ["--uuid", "source:event;$(touch forbidden)"],
        ]
        cases = []
        failures = []
        for mode in ("negative", "zero", "positive", "denied", "real"):
            case_env = dict(env)
            if mode != "real":
                case_env["BINGUX_TEST_NICE"] = {"negative": "-5", "zero": "0", "positive": "19", "denied": "-5"}[mode]
            if mode == "denied":
                case_env["BINGUX_TEST_DENIED"] = "1"
            record.unlink(missing_ok=True)
            outputs = []
            parents = []
            for command in commands:
                parents.append({"nice": os.getpriority(os.PRIO_PROCESS, 0), "policy": os.sched_getscheduler(0)})
                result = run(command, case_env, root)
                assert result.returncode == 0, result.stderr
                outputs.append(result.stderr)
            parents.extend([{"nice": os.getpriority(os.PRIO_PROCESS, 0), "policy": os.sched_getscheduler(0)}] * 2)
            result = run([sys.executable, "-c", CALENDAR, str(SHELL)], case_env, root)
            assert result.returncode == 0, result.stderr
            outputs.append(result.stderr)
            rows = records(record, 6)
            for index, row in enumerate(rows):
                assert row["argv"] == expected[index], row
                assert row["cwd"] == str(root), row
                assert row["environment"] == env["BINGUX_TEST_VALUE"], row
                assert row["policy"] == before["policy"], row
                if mode == "real" and row["nice"] != max(0, parents[index]["nice"]):
                    failures.append(f"real route {index}: parent nice {parents[index]['nice']}, child {row['nice']}")
                expected_nice = {"negative": "0", "zero": "0", "positive": "19", "denied": "-5", "real": ""}[mode]
                if row["simulated_nice"] != expected_nice:
                    failures.append(
                        f"{mode} route {index}: child inherited {row['simulated_nice']}, expected {expected_nice}"
                    )
            if mode == "denied" and not all("Could not reset inherited application priority" in s for s in outputs[:4]):
                failures.append("denied: missing reset diagnostic from QML command helper")
            assert not (root / "forbidden").exists(), "argv was interpreted by a shell"
            cases.append({"mode": mode, "parents": parents, "children": rows})
            print(f"{mode}: " + json.dumps(rows), flush=True)
        # Actual positive nice is separate from the deterministic fake boundary.
        record.unlink()
        positive = run(
            [
                sys.executable,
                "-c",
                "import os,sys; os.setpriority(os.PRIO_PROCESS,0,19); os.execvp(sys.argv[1],sys.argv[1:])",
                *commands[0],
            ],
            env,
            root,
        )
        assert positive.returncode == 0, positive.stderr
        if records(record, 1)[0]["nice"] != 19:
            failures.append("actual positive nice not preserved")
        # A dedicated exec trampoline must report missing executables cleanly.
        helper = SHELL / "application_spawn.py"
        missing = run([sys.executable, str(helper), "--", "bingux-no-such-direct-app-984a5"], env, root)
        if missing.returncode != 127 or "Could not launch" not in missing.stderr or "Traceback" in missing.stderr:
            failures.append(f"missing executable: code={missing.returncode}, stderr={missing.stderr.strip()}")
        assert os.sched_getscheduler(0) == before["policy"], "parent scheduler changed"
        report = {
            "parent": before,
            "cases": cases,
            "failures": failures,
            "sources": {
                str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in [
                    SHELL / "ApplicationLauncher.qml",
                    SHELL / "ControlCentre.qml",
                    SHELL / "calendar-launch.py",
                    helper,
                ]
                if p.exists()
            },
        }
        if args.output:
            args.output.mkdir(parents=True, exist_ok=True)
            (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        if failures:
            print("FAIL:\n" + "\n".join(failures), file=sys.stderr)
            return 1
        print(
            "PASS: six direct paths; negative/zero/positive/denied resets; argv/env/cwd/policy; actual positive nice; missing executable"
        )
        if before["nice"] >= 0:
            print("NOT CHECKED: actual negative-parent reset; run --require-boost in the isolated VM")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
