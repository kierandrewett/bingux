#!/usr/bin/env python3
"""Control the running Bingux shell through its public Quickshell IPC."""
import argparse
import json
import os
import subprocess
import sys
import time


def parser():
    result = argparse.ArgumentParser(prog="binguxctl", description=__doc__)
    result.add_argument("--quickshell", default=os.environ.get("BINGUX_QUICKSHELL", "qs"), help="Quickshell executable")
    selection = result.add_mutually_exclusive_group()
    selection.add_argument("--path", help="Running shell configuration path")
    selection.add_argument("--config", help="Running shell configuration name (default: bingux)")
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("status", help="Print shell state as JSON")
    commands.add_parser("reload", help="Reload the shell configuration")
    commands.add_parser("list", help="List all public IPC targets and methods")
    capture = commands.add_parser("capture", help="Show, trigger or stop capture")
    capture.add_argument("action", nargs="?", default="open", choices=["open", "toggle", "take", "stop", "cancel", "status"])
    capture.add_argument("--mode", choices=["screenshot", "recording"], default="")
    capture.add_argument("--target", choices=["region", "screen", "window"], default="")
    search = commands.add_parser("search", help="Control search or set its query")
    search.add_argument("action", nargs="?", default="open", choices=["open", "close", "toggle", "status", "query"])
    search.add_argument("text", nargs="*")
    for name in ("calendar", "controls", "metrics", "keyboard", "notifications"):
        command = commands.add_parser(name, help=f"Control {name}")
        actions = ["open", "close", "toggle", "status"]
        if name == "keyboard": actions += ["next", "previous"]
        command.add_argument("action", nargs="?", default="toggle", choices=actions)
    emoji = commands.add_parser("emoji", help="Control the emoji picker")
    emoji.add_argument("action", nargs="?", default="open", choices=["open", "close", "status"])
    dnd = commands.add_parser("dnd", help="Set Do Not Disturb")
    dnd.add_argument("action", nargs="?", default="status", choices=["on", "off", "toggle", "status"])
    sidebar = commands.add_parser("sidebar", help="Control the sidebar")
    sidebar.add_argument("action", nargs="?", default="toggle", choices=["open", "close", "toggle", "popout", "dock", "select", "edge", "status"])
    sidebar.add_argument("value", nargs="?")
    for name, actions in (("privacy", ["status"]), ("switcher", ["status", "close"])):
        command = commands.add_parser(name, help=f"Inspect {name}")
        command.add_argument("action", nargs="?", default="status", choices=actions)
    ipc = commands.add_parser("ipc", help="Call any public IPC method directly")
    ipc.add_argument("target")
    ipc.add_argument("method")
    ipc.add_argument("arguments", nargs=argparse.REMAINDER)
    return result


def invocation(args, cli):
    command, action = args.command, getattr(args, "action", None)
    if command == "list": return ["show"]
    if command in ("status", "reload"): call = ["shell", command]
    elif command == "capture":
        if action != "open" and (args.mode or args.target): cli.error("--mode and --target require capture open")
        call = ["capture", "show", args.mode, args.target] if action == "open" else ["capture", {"toggle": "open"}.get(action, action)]
    elif command == "search" and action == "query":
        if not args.text: cli.error("search query requires text")
        call = ["search", "query", " ".join(args.text)]
    elif command == "search" and args.text: cli.error("query text requires search query")
    elif command == "keyboard" and action in ("next", "previous"):
        call = ["shell", "keyboardNext" if action == "next" else "keyboardPrevious"]
    elif command == "dnd": call = ["shell", "dnd", action]
    elif command in ("search", "calendar", "controls", "metrics", "keyboard", "notifications"):
        call = ["shell", "panel", command, action]
    elif command == "sidebar":
        allowed = {"edge": ["left", "top", "right"], "select": ["terminal", "notes", "monitor", "calendar", "media", "tasks"]}
        if action in allowed:
            if args.value not in allowed[action]: cli.error(f"sidebar {action} requires one of: {', '.join(allowed[action])}")
        elif args.value is not None: cli.error(f"sidebar {action} takes no argument")
        call = ["sidebar", "hide" if action == "close" else action] + ([args.value] if args.value else [])
    elif command == "ipc": call = [args.target, args.method, *args.arguments]
    else: call = [command, action]
    return ["call", "--", *call]


def execute(command, run=subprocess.run, sleep=time.sleep):
    for attempt in range(12):
        result = run(command, capture_output=True, text=True, timeout=10)
        output = (result.stdout + result.stderr).strip()
        if output == "Not ready to accept queries yet." and attempt < 11:
            sleep(.1)
            continue
        # Never repeat an uncertain result: a toggle or capture may have executed.
        if result.returncode or output.startswith("No running instances") or output in ("Not ready to accept queries yet.", "Function not found.", "Target not found."):
            raise RuntimeError(output or f"Quickshell exited with status {result.returncode}")
        try:
            response = json.loads(result.stdout)
        except json.JSONDecodeError:
            response = None
        if isinstance(response, dict) and response.get("ok") is False:
            raise RuntimeError(response.get("error", "Shell rejected the command"))
        if result.stdout: print(result.stdout, end="" if result.stdout.endswith("\n") else "\n")
        if result.stderr: print(result.stderr, file=sys.stderr, end="")
        return 0
    raise RuntimeError("Shell is still reloading")


def main(argv=None):
    cli = parser()
    args = cli.parse_args(argv)
    call = invocation(args, cli)
    path = args.path or (None if args.config else os.environ.get("BINGUX_CONFIG_PATH"))
    selection = ["--path", path] if path else ["--config", args.config or os.environ.get("BINGUX_CONFIG_NAME", "bingux")]
    try:
        return execute([args.quickshell, "ipc", *selection, *call])
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as error:
        print(f"binguxctl: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
