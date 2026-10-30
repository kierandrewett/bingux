#!/usr/bin/env python3
"""Run `bsm --backend headless` as the current user and talk to it over the greeter control socket.

usage: tests/bsm-headless-e2e.py [--bsm PATH] [--keep]

No VM, no root, no display: bsm's headless backend renders into memory, and a
test helper stands in for bsm-helper so no PAM or uid change is needed. The helper
stand-in is the one BsmManager's integration harness exposes; it exists only in
builds with the `test-auth` cargo feature, which no package enables.

The binary is found through --bsm, $BSM_BIN, or by building bsm/core with
`--features test-auth` into /tmp/bsm-target/e2e. The script reads `bsm --help` and
runs only when the binary offers everything this test drives (`--backend headless`,
a runtime directory, a frame dump directory and a test helper). Otherwise it
prints what is missing and exits 77 (skipped); it never fakes a pass.

What it checks when it runs:

- the process is named `bsm` and stays the same pid for the whole run;
- the control socket accepts `hello` and answers with a `snapshot`;
- a second connection is refused while the first is open (one greeter);
- `select_user` changes `selected_uid` in the next snapshot and nothing else
  (no foreground session, mode still `login`);
- `begin_auth` with the stand-in helper yields an `auth_prompt`, a wrong answer an
  `auth_result` with ok false, and neither reveals a session;
- SIGTERM ends the process with status 0 and removes its sockets.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
SKIP = 77
TARGET = Path(os.environ.get("BSM_E2E_TARGET", "/tmp/bsm-target/e2e"))
CARGO = os.environ.get("CARGO") or (str(Path.home() / ".cargo/bin/cargo") if (Path.home() / ".cargo/bin/cargo").exists() else "cargo")

# Option names this test looks for in `bsm --help`, first match wins.
WANTED = {
    "runtime directory": ["--runtime-dir", "--runtime"],
    "frame dump directory": ["--frames-dir", "--dump-frames", "--frame-dump"],
    "test helper": ["--test-helper", "--fake-helper", "--test-auth"],
}


def find_binary(argument):
    if argument:
        return Path(argument)
    if os.environ.get("BSM_BIN"):
        return Path(os.environ["BSM_BIN"])
    manifest = ROOT / "bsm/core/Cargo.toml"
    if not manifest.is_file():
        return None
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    build = subprocess.run(
        [CARGO, "build", "--manifest-path", str(manifest), "--bin", "bsm", "--features", "test-auth"],
        env=env,
        capture_output=True,
        text=True,
    )
    if build.returncode != 0:
        print("building bsm with test-auth failed:\n" + build.stderr[-1500:])
        return None
    return TARGET / "debug/bsm"


def discover(binary):
    """Map each wanted capability to the option name bsm offers, or None."""
    help_text = subprocess.run([str(binary), "--help"], capture_output=True, text=True).stdout
    found = {}
    for capability, names in WANTED.items():
        found[capability] = next((n for n in names if re.search(re.escape(n) + r"\b", help_text)), None)
    found["headless backend"] = "headless" if "headless" in help_text else None
    return found


class Control:
    def __init__(self, path):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(10)
        self.sock.connect(str(path))
        self.buffer = b""

    def send(self, message):
        self.sock.sendall(json.dumps(message).encode() + b"\n")

    def receive(self, timeout=10):
        self.sock.settimeout(timeout)
        while b"\n" not in self.buffer:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise EOFError("control socket closed")
            self.buffer += chunk
        line, _, self.buffer = self.buffer.partition(b"\n")
        return json.loads(line)

    def until(self, kind, timeout=10):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            message = self.receive(max(0.1, deadline - time.monotonic()))
            if message.get("type") == kind:
                return message
        raise TimeoutError(f"no {kind} within {timeout} s")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bsm")
    parser.add_argument("--keep", action="store_true")
    args = parser.parse_args()

    binary = find_binary(args.bsm)
    if binary is None or not binary.is_file():
        print("SKIP: no bsm binary (bsm/core has no Cargo.toml yet, and neither --bsm nor $BSM_BIN is set)")
        return SKIP
    offered = discover(binary)
    missing = [name for name, option in offered.items() if option is None]
    if missing:
        print("SKIP: this bsm does not offer: " + ", ".join(missing))
        print("      (a build with the test-auth feature and the options in docs/bsm/manager.md is needed)")
        return SKIP

    work = Path(tempfile.mkdtemp(prefix="bsm-e2e-", dir="/tmp"))
    results = {}
    process = None
    try:
        runtime = work / "run"
        frames = work / "frames"
        runtime.mkdir()
        frames.mkdir()
        command = [
            str(binary),
            "--backend",
            "headless",
            offered["runtime directory"],
            str(runtime),
            offered["frame dump directory"],
            str(frames),
            offered["test helper"],
        ]
        process = subprocess.Popen(command, stdout=(work / "stdout.log").open("w"), stderr=(work / "stderr.log").open("w"))
        control_path = runtime / "greeter-control.sock"
        deadline = time.monotonic() + 30
        while not control_path.exists() and time.monotonic() < deadline and process.poll() is None:
            time.sleep(0.1)
        results["control socket appears"] = control_path.exists()
        if not results["control socket appears"]:
            print((work / "stderr.log").read_text()[-2000:])
            return finish(results, work, args.keep)
        comm = Path(f"/proc/{process.pid}/comm").read_text().strip()
        results["process name is bsm"] = comm == "bsm"

        first = Control(control_path)
        first.send({"type": "hello", "version": 1})
        snapshot = first.until("snapshot")
        results["hello is answered with a snapshot"] = "revision" in snapshot and "users" in snapshot
        results["greeter starts in login mode"] = snapshot.get("mode", {}).get("mode") == "login"

        try:
            second = Control(control_path)
            second.send({"type": "hello", "version": 1})
            second.sock.settimeout(2)
            reply = second.sock.recv(1)
            results["a second greeter is refused while the first is connected"] = reply == b""
        except (OSError, EOFError):
            results["a second greeter is refused while the first is connected"] = True

        users = snapshot.get("users", [])
        if len(users) >= 2:
            target = users[-1]["uid"]
            first.send({"type": "select_user", "uid": target})
            changed = first.until("snapshot")
            results["select_user changes the selected account"] = changed.get("selected_uid") == target
            results["select_user reveals no session"] = changed.get("mode", {}).get("mode") == "login" and all(
                u.get("availability") != "running_foreground" for u in changed.get("users", [])
            )
        else:
            results["select_user changes the selected account"] = None  # not enough accounts from the stand-in

        if users:
            first.send({"type": "begin_auth", "request_id": 1, "uid": users[0]["uid"], "intent": "login"})
            prompt = first.until("auth_prompt")
            results["begin_auth yields a prompt"] = prompt.get("kind") in ("secret", "visible")
            first.send({"type": "auth_response", "challenge_id": prompt["challenge_id"], "response": "definitely-wrong"})
            outcome = first.until("auth_result")
            results["a wrong answer is refused"] = outcome.get("ok") is False
            first.send({"type": "hello", "version": 1})
            after = first.until("snapshot")
            results["a refused login reveals no session"] = after.get("mode", {}).get("mode") == "login"

        results["the pid did not change"] = process.poll() is None and Path(f"/proc/{process.pid}/comm").read_text().strip() == "bsm"
        first.sock.close()
        process.send_signal(signal.SIGTERM)
        try:
            status = process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            status = -1
        results["SIGTERM ends bsm with status 0"] = status == 0
        results["the control socket is removed"] = not control_path.exists()
        results["frames were dumped"] = any(frames.iterdir())
    finally:
        if process is not None and process.poll() is None:
            process.kill()
    return finish(results, work, args.keep)


def finish(results, work, keep):
    failed = False
    for name, ok in results.items():
        if ok is None:
            print("skip  " + name)
            continue
        print(("ok    " if ok else "FAIL  ") + name)
        failed |= not ok
    if failed or keep:
        print("logs:", work)
    else:
        shutil.rmtree(work, ignore_errors=True)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
