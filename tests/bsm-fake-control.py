#!/usr/bin/env python3
"""A fake bsm greeter control server that speaks the real protocol.

It implements the server side of bsm/proto/src/control.rs closely
enough to run and screenshot the bsm greeter (shell/bingux/greeter) with no
bsm, no PAM and no compositor: a hello handshake, revisioned snapshots,
account selection, PAM-like conversations (a password, a password followed by
a visible one-time code, a rejected password), scripted startup progress that
moves only when told to (or on a short step timer with --auto), power
operations and failures.

    bsm-fake-control.py --socket S --driver D --assets DIR [--mode login|unlock]
                        [--unlock-uid UID] [--auto] [--log FILE]

--assets DIR gets one directory per account with a generated wallpaper and
avatar; run the greeter with BSM_GREETER_CACHE=DIR so it accepts those paths.

Accounts (password `secret` unless noted):
  1000 Alex Morgan   running, locked  (avatar picture)
  1001 Sam Rivera    not signed in; password, then a visible code `123456`
  1002 Robin Chen    not signed in; light colour scheme, 12 hour clock
  1003 Dana Frost    unavailable

The driver socket takes one JSON object per line and answers `ok` or
`error: ...`:
  {"cmd": "snapshot"}                                   push a fresh snapshot
  {"cmd": "select", "uid": 1001}                        server-side selection
  {"cmd": "mode", "mode": "unlock", "uid": 1000}        or {"mode": "login"}
  {"cmd": "reach", "stage": "gnoblin_ready"}            progress milestone
  {"cmd": "count", "stage": "outputs_configured", "done": 1, "total": 2}
  {"cmd": "fail", "stage": "gnoblin_ready", "code": "timeout", "message": "..."}
  {"cmd": "clear_progress"}
  {"cmd": "availability", "uid": 1002, "value": "running_locked"}
  {"cmd": "operation", "phase": "failed", "reason": "inhibited"}
  {"cmd": "lock", "uid": 1000}                          a lock fade, then the unlock screen of that account
  {"cmd": "busy", "seconds": 0.5}                       refuse select_user and begin_auth with busy
  {"cmd": "drop"}                                       close the greeter's connection
  {"cmd": "quit"}
"""

import argparse
import asyncio
import json
import math
import os
from pathlib import Path
import struct
import sys
import zlib

STAGES = [
    "auth_accepted",
    "pam_session",
    "host_connected",
    "gnoblin_ready",
    "outputs_configured",
    "bingux_launched",
    "surfaces_mapped",
    "desktop_ready",
    "ready_to_reveal",
]
COUNTED = {"outputs_configured", "surfaces_mapped", "desktop_ready"}
PASSWORD = "secret"
CODE = "123456"


def png(path, width, height, pixel):
    """Write an RGB PNG; pixel(x, y) returns (r, g, b)."""
    rows = bytearray()
    for y in range(height):
        rows.append(0)
        for x in range(width):
            rows.extend(pixel(x, y))

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n"
    data += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(bytes(rows), 6))
    data += chunk(b"IEND", b"")
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_bytes(data)


def wallpaper(top, bottom, glow):
    def pixel(x, y):
        t = y / 799
        r, g, b = (top[i] * (1 - t) + bottom[i] * t for i in range(3))
        # Two soft lights, so the frosted surfaces have something to blur.
        for cx, cy, radius, colour in glow:
            d = math.hypot(x - cx, y - cy) / radius
            if d < 1:
                k = (1 - d) ** 2 * 0.55
                r, g, b = (c * (1 - k) + colour[i] * k for i, c in enumerate((r, g, b)))
        return int(r), int(g), int(b)

    return pixel


def avatar(inner, outer):
    def pixel(x, y):
        d = math.hypot(x - 63.5, y - 63.5) / 64
        t = min(1, d)
        return tuple(int(inner[i] * (1 - t) + outer[i] * t) for i in range(3))

    return pixel


def make_assets(root):
    root = Path(root)
    png(root / "1000/wallpaper.png", 1280, 800, wallpaper((31, 59, 115), (194, 90, 124), [(300, 250, 420, (255, 190, 120)), (1000, 600, 380, (90, 140, 255))]))
    png(root / "1000/avatar.png", 128, 128, avatar((255, 214, 150), (190, 90, 60)))
    png(root / "1001/wallpaper.png", 1280, 800, wallpaper((20, 60, 50), (210, 130, 50), [(900, 200, 450, (255, 230, 150)), (200, 650, 400, (60, 200, 150))]))
    png(root / "1002/wallpaper.png", 1280, 800, wallpaper((70, 40, 110), (230, 150, 190), [(640, 300, 500, (255, 255, 255))]))


def account(uid, name, display, availability, accent, scheme="dark", clock="24h", session=None, wallpaper_rev=1, picture=True, root=""):
    base = f"{root}/{uid}"
    return {
        "uid": uid,
        "user_name": name,
        "display_name": display,
        "avatar_path": f"{base}/avatar.png" if uid == 1000 and picture else None,
        "avatar_rev": 1,
        "wallpaper_path": f"{base}/wallpaper.png" if uid in (1000, 1001, 1002) and picture else None,
        "wallpaper_blur_path": None,
        "wallpaper_rev": wallpaper_rev,
        "show_personal_images": picture,
        "appearance": {"color_scheme": scheme, "accent": accent, "clock_format": clock, "reduce_motion": False},
        "availability": availability,
        "session": session,
    }


class Server:
    def __init__(self, args):
        self.args = args
        self.assets = os.path.abspath(args.assets)
        self.revision = 0
        self.mode = {"mode": "unlock", "session": "s0000000a", "uid": args.unlock_uid} if args.mode == "unlock" else {"mode": "login"}
        self.selected = args.unlock_uid if args.mode == "unlock" else 1000
        pictures = not args.no_pictures
        self.users = {
            1000: account(1000, "alex", "Alex Morgan", "running_locked", "#3584e4", session="s0000000a", picture=pictures, root=self.assets),
            1001: account(1001, "sam", "Sam Rivera", "not_signed_in", "#e66100", clock="12h", picture=pictures, root=self.assets),
            1002: account(1002, "robin", "Robin Chen", "not_signed_in", "#26a269", scheme="light", clock="12h", picture=pictures, root=self.assets),
            1003: account(1003, "dana", "Dana Frost", "unavailable", "#c64600", root=self.assets),
        }
        if args.mode == "login":
            # A cold boot: nobody is signed in yet.
            self.users[1000].update(availability="not_signed_in", session=None)
        else:
            self.users[1002].update(availability="running_locked", session="s0000000c")
        if args.selected is not None:
            self.selected = args.selected
        self.progress = None
        self.generation = 1000
        self.writer = None
        self.challenge = None  # {id, uid, intent, step}
        self.next_challenge = 100
        self.next_operation = 500
        self.operation = None
        # The operation the real manager keeps in `authorising` for as long as a
        # sign-in conversation waits for its answer: (id, kind) or None.
        self.conversation_op = None
        self.log_file = open(args.log, "a", buffering=1) if args.log else None
        self.deny_power = args.deny_power
        self.busy_until = 0.0

    # -- output -----------------------------------------------------------
    def log(self, kind, **fields):
        if self.log_file:
            self.log_file.write(json.dumps({"recv": kind, **fields}) + "\n")

    def snapshot(self):
        return {
            "type": "snapshot",
            "revision": self.revision,
            "mode": self.mode,
            "selected_uid": self.selected,
            "users": list(self.users.values()),
            "progress": self.progress,
            "reduce_motion": False,
            "hostname": "bingux",
        }

    def send(self, message):
        if self.writer is None:
            return
        self.writer.write((json.dumps(message) + "\n").encode())

    def push_snapshot(self):
        self.revision += 1
        self.send(self.snapshot())

    # -- progress ---------------------------------------------------------
    def new_progress(self, uid):
        self.generation += 1
        self.progress = {
            "session": "s0000000b",
            "generation": self.generation,
            "revision": 1,
            "reached": [],
            "counted_stage": None,
            "done": 0,
            "total": 0,
            "failure": None,
        }

    def pending(self):
        for stage in STAGES:
            if stage not in self.progress["reached"]:
                return stage
        return None

    def reach(self, stage):
        p = self.progress
        if p is None or p["failure"] is not None or self.pending() != stage:
            return False
        p["reached"].append(stage)
        p.update(counted_stage=None, done=0, total=0, revision=p["revision"] + 1)
        self.send({"type": "progress", **p})
        return True

    def count(self, stage, done, total):
        p = self.progress
        if p is None or p["failure"] is not None or self.pending() != stage or stage not in COUNTED or done > total:
            return False
        p.update(counted_stage=stage, done=done, total=total, revision=p["revision"] + 1)
        self.send({"type": "progress", **p})
        return True

    def fail(self, stage, code, message):
        p = self.progress
        if p is None:
            return False
        p["failure"] = {"stage": stage, "code": code, "message": message}
        p["revision"] += 1
        self.send({"type": "progress", **p})
        return True

    async def auto_progress(self):
        """--auto: walk the milestones with short pauses, like a real launch."""
        steps = [
            ("reach", "auth_accepted"),
            ("reach", "pam_session"),
            ("reach", "host_connected"),
            ("reach", "gnoblin_ready"),
            ("count", "outputs_configured", 0, 2),
            ("count", "outputs_configured", 1, 2),
            ("reach", "outputs_configured"),
            ("reach", "bingux_launched"),
            ("count", "surfaces_mapped", 1, 3),
            ("count", "surfaces_mapped", 2, 3),
            ("reach", "surfaces_mapped"),
            ("count", "desktop_ready", 1, 2),
            ("reach", "desktop_ready"),
            ("reach", "ready_to_reveal"),
        ]
        for step in steps:
            await asyncio.sleep(0.9)
            if step[0] == "reach":
                self.reach(step[1])
            else:
                self.count(*step[1:])
            await self.writer.drain()

    # -- conversation -----------------------------------------------------
    def prompt(self, request_id, kind, text):
        self.send(
            {
                "type": "auth_prompt",
                "request_id": request_id,
                "challenge_id": self.challenge["id"],
                "kind": kind,
                "text": text,
            }
        )

    def result(self, ok, code=None, message=""):
        self.send({"type": "auth_result", "challenge_id": self.challenge["id"], "ok": ok, "code": code, "message": message})
        self.challenge = None

    def end_conversation_op(self, phase):
        if self.conversation_op is not None:
            self.operation, kind_name = self.conversation_op
            self.conversation_op = None
            self.operation_event(kind_name, phase, "cancelled" if phase == "cancelled" else None)

    def operation_event(self, kind, phase, reason=None):
        self.send({"type": "operation", "operation_id": self.operation, "kind": kind, "phase": phase, "reason": reason})

    async def handle(self, message):
        kind = message.get("type")
        if kind == "hello":
            self.log("hello", version=message.get("version"))
            if message.get("version") != 1:
                self.send({"type": "error", "request_id": None, "code": "not_supported", "message": "version"})
                return
            self.send(self.snapshot())
        elif kind in ("select_user", "begin_auth") and asyncio.get_running_loop().time() < self.busy_until:
            # A lock or switch fade is running: bsm refuses with busy.
            self.log("busy_" + kind)
            self.send({"type": "error", "request_id": message.get("request_id"), "code": "busy", "message": "a transition is running"})
        elif kind == "select_user":
            uid = message.get("uid")
            self.log("select_user", uid=uid)
            if uid not in self.users:
                self.send({"type": "error", "request_id": None, "code": "invalid_account", "message": "no such account"})
                return
            self.selected = uid
            self.challenge = None
            self.end_conversation_op("cancelled")
            self.push_snapshot()
        elif kind == "begin_auth":
            uid = message.get("uid")
            self.log("begin_auth", uid=uid, intent=message.get("intent"))
            if uid not in self.users:
                self.send({"type": "error", "request_id": message.get("request_id"), "code": "invalid_account", "message": "no such account"})
                return
            if uid != self.selected:
                # Like the real manager: begin_auth must name the selected account.
                self.send({"type": "error", "request_id": message.get("request_id"), "code": "stale_generation", "message": "not the selected account"})
                return
            if self.progress is not None and self.progress["failure"] is not None:
                # Signing in again after a failed launch starts from a clean slate.
                self.progress = None
                self.push_snapshot()
            self.challenge = {"id": self.next_challenge, "uid": uid, "intent": message.get("intent"), "step": "password", "request": message.get("request_id")}
            self.next_challenge += 1
            if self.conversation_op is None:
                # BeginAuth opens an operation that stays `authorising` until the
                # conversation ends: the greeter must not read it as a running fade.
                kind_name = {"unlock": "unlock_slide_up", "switch": "switch_user"}.get(message.get("intent"), "new_login_fade")
                self.operation = self.next_operation
                self.next_operation += 1
                self.conversation_op = (self.operation, kind_name)
                self.operation_event(kind_name, "requested")
                self.operation_event(kind_name, "authorising")
            self.prompt(message.get("request_id"), "secret", "Password: ")
        elif kind == "auth_response":
            # The response is a secret: only its length is ever logged.
            self.log("auth_response", challenge_id=message.get("challenge_id"), length=len(message.get("response", "")))
            c = self.challenge
            if c is None or message.get("challenge_id") != c["id"]:
                return
            await asyncio.sleep(0.15)
            response = message.get("response")
            if c["step"] == "password":
                if response != PASSWORD:
                    code, text = "pam_failure", "Authentication failed"
                    self.result(False, code, text)
                    return
                if c["uid"] == 1001:
                    c["step"] = "code"
                    self.prompt(None, "info", "A code was sent to your phone")
                    self.prompt(None, "visible", "One-time code: ")
                    return
            elif c["step"] == "code" and response != CODE:
                self.result(False, "pam_failure", "That code is not right")
                return
            self.accept(c)
        elif kind == "cancel_auth":
            self.log("cancel_auth", challenge_id=message.get("challenge_id"))
            if self.challenge and self.challenge["id"] == message.get("challenge_id"):
                self.challenge = None
                self.end_conversation_op("cancelled")
        elif kind == "power":
            self.log("power", action=message.get("action"))
            self.operation = self.next_operation
            self.next_operation += 1
            kind_name = "suspend" if message.get("action") == "suspend" else "power_fade"
            self.operation_event(kind_name, "requested")
            await asyncio.sleep(0.2)
            self.operation_event(kind_name, "authorising")
            await asyncio.sleep(0.2)
            if self.deny_power:
                self.operation_event(kind_name, "failed", "unauthorised")
                return
            self.operation_event(kind_name, "preparing")
        elif kind == "cancel_operation":
            self.log("cancel_operation", operation_id=message.get("operation_id"))
            if self.operation is not None:
                self.operation_event("power_fade", "cancelled")
        else:
            self.log("unknown", type=kind)

    def accept(self, c):
        user = self.users[c["uid"]]
        self.result(True)
        if self.conversation_op is not None:
            self.operation, kind_name = self.conversation_op
            self.conversation_op = None
            self.operation_event(kind_name, "preparing")
        if user["availability"] in ("not_signed_in", "unavailable") and c["intent"] != "unlock":
            self.new_progress(c["uid"])
            user["availability"] = "starting"
            self.push_snapshot()
            if self.args.auto:
                asyncio.ensure_future(self.auto_progress())
        elif self.operation is None:
            self.operation = self.next_operation
            self.next_operation += 1
            kind = "unlock_slide_up" if c["intent"] == "unlock" else "switch_user"
            self.operation_event(kind, "requested")
            self.operation_event(kind, "preparing")

    # -- connections ------------------------------------------------------
    async def greeter(self, reader, writer):
        if self.writer is not None:
            writer.close()
            return
        self.writer = writer
        self.challenge = None
        try:
            while True:
                line = await reader.readline()
                if not line:
                    break
                try:
                    message = json.loads(line)
                except ValueError:
                    self.log("malformed")
                    continue
                await self.handle(message)
                await writer.drain()
        finally:
            self.writer = None
            writer.close()

    async def driver(self, reader, writer):
        while True:
            line = await reader.readline()
            if not line:
                break
            try:
                reply = await self.command(json.loads(line))
            except Exception as error:  # noqa: BLE001 - the driver is a test tool
                reply = f"error: {error}"
            writer.write((reply + "\n").encode())
            await writer.drain()
        writer.close()

    async def command(self, c):
        name = c["cmd"]
        if name == "snapshot":
            self.push_snapshot()
        elif name == "select":
            # Like the real manager: an external selection first cancels the
            # open conversation (result and operation events), and the snapshot
            # that names the new account follows a moment later.
            if self.challenge is not None:
                self.result(False, "cancelled", "cancelled")
                self.end_conversation_op("cancelled")
                await self.writer.drain()
                await asyncio.sleep(0.03)
            self.selected = c["uid"]
            self.push_snapshot()
        elif name == "mode":
            self.mode = {"mode": "unlock", "session": "s0000000a", "uid": c["uid"]} if c["mode"] == "unlock" else {"mode": "login"}
            if c["mode"] == "unlock":
                self.selected = c["uid"]
            self.push_snapshot()
        elif name == "reach":
            if not self.reach(c["stage"]):
                return "error: stage refused"
        elif name == "count":
            if not self.count(c["stage"], c["done"], c["total"]):
                return "error: count refused"
        elif name == "fail":
            self.fail(c["stage"], c.get("code", "timeout"), c.get("message", ""))
            # The launch is over; the account is not signed in again.
            self.users[self.selected]["availability"] = "not_signed_in"
            self.push_snapshot()
        elif name == "clear_progress":
            self.progress = None
            self.push_snapshot()
        elif name == "availability":
            self.users[c["uid"]]["availability"] = c["value"]
            self.push_snapshot()
        elif name == "operation":
            self.operation_event("power_fade", c["phase"], c.get("reason"))
        elif name == "lock":
            # A lock: bsm refuses select_user and begin_auth with busy while the
            # fade runs, ends any open conversation, then publishes unlock mode.
            self.busy_until = asyncio.get_running_loop().time() + 0.6
            self.end_conversation_op("cancelled")
            self.challenge = None
            self.operation = self.next_operation
            self.next_operation += 1
            for phase in ("requested", "authorising", "preparing", "animating"):
                self.operation_event("lock", phase)
                await self.writer.drain()
                await asyncio.sleep(0.1)
            self.mode = {"mode": "unlock", "session": "s0000000a", "uid": c["uid"]}
            self.users[c["uid"]].update(availability="running_locked", session="s0000000a")
            self.selected = c["uid"]
            self.push_snapshot()
            await asyncio.sleep(0.2)
            self.operation_event("lock", "completed")
        elif name == "busy":
            self.busy_until = asyncio.get_running_loop().time() + c["seconds"]
        elif name == "drop":
            if self.writer:
                self.writer.close()
        elif name == "quit":
            asyncio.get_running_loop().call_later(0.1, os._exit, 0)
        else:
            return "error: unknown command"
        if self.writer:
            await self.writer.drain()
        return "ok"


async def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--socket", required=True)
    parser.add_argument("--driver")
    parser.add_argument("--assets", required=True)
    parser.add_argument("--mode", choices=["login", "unlock"], default="login")
    parser.add_argument("--unlock-uid", type=int, default=1000)
    parser.add_argument("--selected", type=int, help="account shown first (default: the unlock owner or 1000)")
    parser.add_argument("--no-pictures", action="store_true", help="every account opts out of personal images")
    parser.add_argument("--auto", action="store_true", help="advance startup progress on a short step timer")
    parser.add_argument("--deny-power", action="store_true", help="refuse power requests as unauthorised")
    parser.add_argument("--log", help="append received message types (never secrets) as JSON lines")
    args = parser.parse_args()

    make_assets(args.assets)
    server = Server(args)
    for path in (args.socket, args.driver):
        if path and os.path.exists(path):
            os.unlink(path)
    listeners = [await asyncio.start_unix_server(server.greeter, path=args.socket)]
    if args.driver:
        listeners.append(await asyncio.start_unix_server(server.driver, path=args.driver))
    print("READY", flush=True)
    await asyncio.Event().wait()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        pass
