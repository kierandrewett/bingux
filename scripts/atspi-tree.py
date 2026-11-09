#!/usr/bin/env python3
"""Dump what a screen reader sees of a running app: its AT-SPI tree.

bingux-ui builds an AccessKit tree; Freya's winit backend hands it to
`accesskit_unix`, which serves it on the AT-SPI bus when assistive technology
is enabled (`org.a11y.Status.IsEnabled`). This script reads that tree the way
Orca does, through `gi.repository.Atspi`, and prints one line per object:

    push button "Add Device" [focusable, enabled, sensitive] actions=click,focus

with the description, value, relations (labelled-by, controlled-by, ...) and
the states that matter (checked, expanded, selected, busy, focused). It ends
with a summary and a list of findings: an interactive object with no name, a
slider or progress bar with no value, a focusable object a person cannot see.

Run it inside a session that has the accessibility bus. `scripts/a11y-run.sh`
starts one (dbus, at-spi-bus-launcher, IsEnabled=true, a headless sway) and
runs this after the app has drawn:

    scripts/a11y-run.sh --app bingux-settings --out tree.txt -- /path/to/bingux-settings
    python3 scripts/atspi-tree.py --app bingux-settings --out tree.txt

Needs `gir1.2-atspi-2.0` and `python3-gi` (Debian, Ubuntu) or `at-spi2-core`
and `python3-gobject` (Fedora). Exit status 1 when the app never appeared, 2
with `--strict` when there are findings.
"""

from __future__ import annotations

import argparse
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

# Roles a person operates: each needs a name.
INTERACTIVE = {
    "push button",
    "toggle button",
    "check box",
    "radio button",
    "combo box",
    "menu item",
    "check menu item",
    "radio menu item",
    "page tab",
    "slider",
    "spin button",
    "entry",
    "password text",
    "link",
    "list item",
    "tree item",
    "switch",
}
# Roles that carry a number: each needs a value.
VALUED = {"slider", "spin button", "progress bar", "scroll bar", "level bar"}
# States worth printing; the rest (showing, visible, ...) is noise on every line.
STATES = [
    "focusable",
    "focused",
    "checkable",
    "checked",
    "indeterminate",
    "pressed",
    "expandable",
    "expanded",
    "collapsed",
    "selectable",
    "selected",
    "busy",
    "modal",
    "editable",
    "read_only",
    "required",
    "invalid_entry",
    "multiselectable",
    "has_popup",
]


def state_names(acc) -> list[str]:
    try:
        states = acc.get_state_set()
    except Exception:  # noqa: BLE001 - a vanished object
        return []
    out = []
    for name in STATES:
        state = getattr(Atspi.StateType, name.upper(), None)
        if state is not None and states.contains(state):
            out.append(name)
    if not states.contains(Atspi.StateType.ENABLED) and not states.contains(Atspi.StateType.SENSITIVE):
        out.append("disabled")
    return out


def value_of(acc) -> str | None:
    try:
        value = acc.get_value_iface()
    except Exception:  # noqa: BLE001
        return None
    if value is None:
        return None
    try:
        current = value.get_current_value()
        text = value.get_text() if hasattr(value, "get_text") else ""
        low, high = value.get_minimum_value(), value.get_maximum_value()
    except Exception:  # noqa: BLE001
        return None
    shown = f"{current:g}"
    if text:
        shown += f" ({text})"
    return f"{shown} in {low:g}..{high:g}"


def text_of(acc) -> str | None:
    try:
        text = acc.get_text_iface()
    except Exception:  # noqa: BLE001
        return None
    if text is None:
        return None
    try:
        return text.get_text(0, min(text.get_character_count(), 80))
    except Exception:  # noqa: BLE001
        return None


def relations_of(acc) -> list[str]:
    out = []
    try:
        relations = acc.get_relation_set()
    except Exception:  # noqa: BLE001
        return out
    for relation in relations or []:
        kind = relation.get_relation_type().value_nick
        targets = []
        for index in range(relation.get_n_targets()):
            target = relation.get_target(index)
            targets.append(f'{target.get_role_name()} "{target.get_name()}"')
        out.append(f"{kind}->{', '.join(targets)}")
    return out


def actions_of(acc) -> list[str]:
    try:
        action = acc.get_action_iface()
    except Exception:  # noqa: BLE001
        return []
    if action is None:
        return []
    return [action.get_action_name(index) for index in range(action.get_n_actions())]


def extents_of(acc) -> tuple[int, int, int, int] | None:
    try:
        component = acc.get_component_iface()
        if component is None:
            return None
        rect = component.get_extents(Atspi.CoordType.WINDOW)
        return rect.x, rect.y, rect.width, rect.height
    except Exception:  # noqa: BLE001
        return None


class Dump:
    def __init__(self, keep_containers: bool) -> None:
        self.lines: list[str] = []
        self.findings: list[str] = []
        self.roles: dict[str, int] = {}
        self.keep_containers = keep_containers

    def walk(self, acc, depth: int, path: str) -> None:
        try:
            role = acc.get_role_name()
            name = acc.get_name() or ""
        except Exception:  # noqa: BLE001
            return
        description = acc.get_description() or ""
        states = state_names(acc)
        value = value_of(acc) if role in VALUED or "slider" in role else None
        relations = relations_of(acc)
        actions = actions_of(acc)
        count = acc.get_child_count()
        self.roles[role] = self.roles.get(role, 0) + 1
        # A nameless container with one child adds a line and says nothing: fold it.
        plain = role in ("panel", "filler", "unknown") and not name and not description and not states
        here = f"{path}/{role}"
        if not plain or self.keep_containers:
            parts = [f"{'  ' * depth}{role}"]
            if name:
                parts.append(f'"{name}"')
            if description:
                parts.append(f'desc="{description}"')
            if states:
                parts.append(f"[{', '.join(states)}]")
            if value is not None:
                parts.append(f"value={value}")
            if role in ("entry", "text", "password text"):
                text = text_of(acc)
                if text:
                    parts.append(f'text="{text}"')
            if relations:
                parts.append("rel=" + "; ".join(relations))
            if actions:
                parts.append("actions=" + ",".join(actions))
            self.lines.append(" ".join(parts))
            depth += 1
        if role in INTERACTIVE and not name.strip():
            self.findings.append(f"unnamed {role} at {here} {extents_of(acc)}")
        if role in VALUED and value is None:
            self.findings.append(f'{role} "{name}" has no value')
        if "focusable" in states:
            extents = extents_of(acc)
            if extents is not None and (extents[2] <= 0 or extents[3] <= 0):
                self.findings.append(f'focusable {role} "{name}" has no size')
        for index in range(min(count, 2000)):
            try:
                child = acc.get_child_at_index(index)
            except Exception:  # noqa: BLE001
                continue
            if child is not None:
                self.walk(child, depth, here)


def find_app(name: str, timeout: float):
    deadline = time.monotonic() + timeout
    seen: list[str] = []
    while time.monotonic() < deadline:
        desktop = Atspi.get_desktop(0)
        seen = []
        for index in range(desktop.get_child_count()):
            app = desktop.get_child_at_index(index)
            if app is None:
                continue
            app_name = app.get_name() or ""
            seen.append(app_name)
            if name.lower() in app_name.lower() and app.get_child_count() > 0:
                return app, seen
            # accesskit names the application after the toolkit when the app sets no name:
            # match on the window title too.
            for child_index in range(app.get_child_count()):
                window = app.get_child_at_index(child_index)
                if window is not None and name.lower() in (window.get_name() or "").lower():
                    return app, seen
        time.sleep(0.5)
    return None, seen


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--app", required=True, help="application or window name to find (substring)")
    parser.add_argument("--out", help="write the dump here as well as to stdout")
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--containers", action="store_true", help="keep nameless panels")
    parser.add_argument("--strict", action="store_true", help="exit 2 when there are findings")
    args = parser.parse_args()

    Atspi.init()
    app, seen = find_app(args.app, args.timeout)
    if app is None:
        print(f"atspi-tree: no application matching {args.app!r}; the bus lists {seen}", file=sys.stderr)
        return 1
    dump = Dump(args.containers)
    dump.lines.append(f'application "{app.get_name()}" toolkit={app.get_toolkit_name()!r}')
    for index in range(app.get_child_count()):
        child = app.get_child_at_index(index)
        if child is not None:
            dump.walk(child, 1, "")
    summary = [
        "",
        f"objects: {sum(dump.roles.values())}",
        "roles: " + ", ".join(f"{role} {count}" for role, count in sorted(dump.roles.items(), key=lambda kv: -kv[1])),
        f"findings: {len(dump.findings)}",
        *[f"  {finding}" for finding in dump.findings],
    ]
    text = "\n".join(dump.lines + summary) + "\n"
    sys.stdout.write(text)
    if args.out:
        with open(args.out, "w", encoding="utf-8") as handle:
            handle.write(text)
    return 2 if args.strict and dump.findings else 0


if __name__ == "__main__":
    sys.exit(main())
