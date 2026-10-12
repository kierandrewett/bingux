#!/usr/bin/env python3
"""Verify cold Qt 6.10.3 qmlprofiler XML without operating a desktop.

Exit 0: verified absence; 1: forbidden activity; 2: invalid/inadequate trace.
The optional SceneGraph cutoff includes its timestamp, not just earlier ranges.
"""

import argparse
from collections import Counter
from dataclasses import dataclass
import json
from pathlib import Path
import sys
from urllib.parse import unquote, urlsplit
import xml.etree.ElementTree as ET

DEFAULT_FORBIDDEN = (
    "SystemMetricsPopup.qml",
    "CalendarPopup.qml",
    "NotificationHistoryPopup.qml",
)
COLD_TYPES = ("Compiling", "Creating")


class TraceError(ValueError):
    """The file cannot establish the requested cold-trace condition."""


def nonnegative(value, label):
    try:
        number = int(value)
    except (ValueError, TypeError) as exc:
        raise TraceError(f"{label}: expected an integer, got {value!r}") from exc
    if number < 0:
        raise TraceError(f"{label}: negative value {number}")
    return number


def source_name(value):
    """Exact source basename, including URL percent decoding; no substring match."""
    if not value:
        return ""
    try:
        path = unquote(urlsplit(value.strip()).path).replace("\\", "/")
    except ValueError as exc:
        raise TraceError(f"invalid source URL: {value!r}") from exc
    name = path.rsplit("/", 1)[-1]
    return name if name.endswith((".qml", ".js")) else ""


@dataclass(frozen=True)
class Event:
    kind: str
    sources: frozenset


@dataclass(frozen=True)
class Range:
    index: int
    start: int
    duration: int


def read_trace(path):
    try:
        raw = Path(path).read_bytes()
        if b"<!DOCTYPE" in raw or b"<!ENTITY" in raw:
            raise TraceError("DTD/entity declarations are not part of the supported trace schema")
        root = ET.fromstring(raw)
    except (OSError, ET.ParseError) as exc:
        raise TraceError(f"cannot read complete trace XML: {exc}") from exc
    if root.tag != "trace" or root.get("version") != "1.02":
        raise TraceError("expected Qt trace root <trace version='1.02'>")
    start = nonnegative(root.get("traceStart"), "traceStart")
    end = nonnegative(root.get("traceEnd"), "traceEnd")
    if end <= start:
        raise TraceError("traceEnd must be greater than traceStart")
    sections = {}
    for name in ("eventData", "profilerDataModel"):
        matches = root.findall(name)
        if len(matches) != 1 or not len(matches[0]):
            raise TraceError(f"expected exactly one nonempty {name} section")
        sections[name] = matches[0]
    if sections["eventData"].get("totalTime") is not None:
        nonnegative(sections["eventData"].get("totalTime"), "eventData.totalTime")
    events = {}
    for node in sections["eventData"]:
        if node.tag != "event":
            raise TraceError(f"unexpected eventData child {node.tag!r}")
        index = nonnegative(node.get("index"), "event.index")
        if index in events:
            raise TraceError(f"duplicate event definition {index}")
        for field in ("type", "filename", "details"):
            if len(node.findall(field)) > 1:
                raise TraceError(f"duplicate {field} in event {index}")
        kind = (node.findtext("type") or "").strip()
        if not kind:
            raise TraceError(f"event {index} has no type")
        sources = frozenset(
            filter(None, (source_name(node.findtext("filename")), source_name(node.findtext("details"))))
        )
        events[index] = Event(kind, sources)
    ranges = []
    for node in sections["profilerDataModel"]:
        if node.tag != "range":
            raise TraceError(f"unexpected profilerDataModel child {node.tag!r}")
        index = nonnegative(node.get("eventIndex"), "range.eventIndex")
        if index not in events:
            raise TraceError(f"range references undefined event {index}")
        timestamp = nonnegative(node.get("startTime"), f"range {index}.startTime")
        value = node.get("duration")
        if value is None and events[index].kind in COLD_TYPES:
            raise TraceError(f"{events[index].kind} range {index} has no duration")
        duration = nonnegative(value, f"range {index}.duration") if value is not None else 0
        ranges.append(Range(index, timestamp, duration))
    return start, end, events, ranges


def verify_trace(path, forbidden=DEFAULT_FORBIDDEN, until_first_scenegraph=False, require_source="shell.qml"):
    names = set(forbidden)
    if not names or any(source_name(name) != name for name in names):
        raise TraceError("forbidden sources must be nonempty exact .qml/.js basenames")
    if source_name(require_source) != require_source or not require_source:
        raise TraceError("required source must be an exact .qml/.js basename")
    start, trace_end, events, ranges = read_trace(path)
    cutoff = trace_end
    if until_first_scenegraph:
        markers = [r.start for r in ranges if events[r.index].kind == "SceneGraph" and start <= r.start <= trace_end]
        if not markers:
            raise TraceError("no referenced SceneGraph marker inside declared trace bounds")
        cutoff = min(markers)
    counts = Counter()
    entry_types = set()
    violations = {}
    for record in ranges:
        event = events[record.index]
        # Include a range crossing traceStart or the cutoff; do not count later use.
        if (
            event.kind not in COLD_TYPES
            or record.start > cutoff
            or record.start + record.duration < start
            or not event.sources
        ):
            continue
        counts[event.kind] += 1
        if require_source in event.sources:
            entry_types.add(event.kind)
        for name in event.sources & names:
            key = (name, event.kind)
            group = violations.setdefault(
                key,
                {
                    "filename": name,
                    "type": event.kind,
                    "rangeCount": 0,
                    "firstStartTime": record.start,
                    "eventIndexes": set(),
                },
            )
            group["rangeCount"] += 1
            group["firstStartTime"] = min(group["firstStartTime"], record.start)
            group["eventIndexes"].add(record.index)
    missing = [kind for kind in COLD_TYPES if not counts[kind] or kind not in entry_types]
    if missing:
        raise TraceError(
            f"no qualifying cold {', '.join(missing)} coverage for {require_source!r} inside selected interval"
        )
    result_groups = []
    for key in sorted(violations):
        group = violations[key]
        group["eventIndexes"] = sorted(group["eventIndexes"])
        result_groups.append(group)
    return {
        "ok": not result_groups,
        "status": "pass" if not result_groups else "forbidden",
        "trace": str(Path(path)),
        "schema": "1.02",
        "timeUnit": "ns",
        "window": {
            "startTime": start,
            "endTime": cutoff,
            "boundary": "first-SceneGraph-inclusive" if until_first_scenegraph else "traceEnd-inclusive",
        },
        "definitions": len(events),
        "ranges": len(ranges),
        "qualifyingRanges": {kind: counts[kind] for kind in COLD_TYPES},
        "requiredSource": require_source,
        "forbidden": sorted(names),
        "violations": result_groups,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument(
        "--forbid",
        action="append",
        metavar="BASENAME",
        help="exact forbidden .qml/.js basename; repeatable; replaces the three defaults",
    )
    parser.add_argument(
        "--until-first-scenegraph",
        action="store_true",
        help="check through the earliest referenced SceneGraph timestamp, inclusive",
    )
    parser.add_argument(
        "--require-source",
        default="shell.qml",
        metavar="BASENAME",
        help="require this entry source in both cold Compiling and Creating ranges",
    )
    args = parser.parse_args(argv)
    try:
        result = verify_trace(
            args.trace, args.forbid or DEFAULT_FORBIDDEN, args.until_first_scenegraph, args.require_source
        )
    except TraceError as exc:
        print(
            json.dumps({"ok": False, "status": "invalid", "trace": str(args.trace), "error": str(exc)}, sort_keys=True)
        )
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
