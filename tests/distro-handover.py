#!/usr/bin/env python3
"""Frame checks over the recorded handovers of distro install and boot runs.

usage: tests/distro-handover.py RECORDING_DIR [RECORDING_DIR ...]

Each RECORDING_DIR is a run's out_dir (flow.mp4, timeline.json, frame-events.jsonl):
distro/out/install-test-<variant>/live and distro/out/boot-test.

Checks, per recording:

- gaps: no black run longer than BLACK_RUN_LIMIT in frame-events.jsonl, except
  the firmware reset after the installer's restart (not a handover: the
  firmware owns the screen there);
- installer -> reboot: every frame from "restart pressed" to the guest reset is content;
- greeter -> session (installed system): every frame from "greeter shown" to
  "desktop painted" is content; the bar clock box matches between the two stages,
  and the wallpaper corner moves less than WALLPAPER_LIMIT;
- installer -> live desktop (boot run): every frame from "submit try bingux" to
  "live desktop bar" is content;
- session -> lock -> unlock -> logout (boot run): every frame from "lock requested"
  to "lock screen shown", from "unlock submitted" to "desktop after unlock" and
  from "logout requested" to "logout done" is content; the bar clock box matches
  between the desktop before lock and after unlock; the wallpaper corner matches
  within WALLPAPER_LIMIT.

Writes handover.json next to each recording and exits 1 on any defect.
"""

import json
from pathlib import Path
import sys

from distro_capture import (
    WALLPAPER_LIMIT,
    bar_matches,
    black_gaps,
    frame_at,
    handover_defects,
    recording_events,
    wallpaper_jump,
)

# Stage pairs whose bar clock and wallpaper must line up.
INSTALLED_PAIRS = [("greeter shown", "desktop painted")]
BOOT_PAIRS = [("live desktop bar", "desktop after unlock")]


def check_recording(out_dir: Path) -> dict:
    video = out_dir / "flow.mp4"
    events = recording_events(out_dir)
    result = {"recording": str(out_dir), "checks": {}, "defects": [], "measures": {}}
    firmware = []
    if "restart pressed" in events and "greeter shown" in events:
        firmware = [(events["guest reset after restart"], events["greeter shown"])]
        restart_to_reset = (events["restart pressed"], events["guest reset after restart"])
        defects = handover_defects(video, *restart_to_reset)
        result["checks"]["installer -> reboot is content"] = not defects
        result["defects"] += [f"installer->reboot black/placeholder at {t:.1f}s ({k})" for t, k in defects]
        result["firmware_gap"] = firmware[0]

    gaps = black_gaps(out_dir, exclude=firmware)
    result["checks"]["no black gap over 0.5s outside firmware"] = not gaps
    result["defects"] += [f"black gap {a:.2f}-{b:.2f}s" for a, b in gaps]

    if "greeter shown" in events and "desktop painted" in events:
        start, end = events["greeter shown"], events["desktop painted"]
        defects = handover_defects(video, start, end)
        result["checks"]["greeter -> session is content"] = not defects
        result["defects"] += [f"greeter->session black/placeholder at {t:.1f}s ({k})" for t, k in defects]
        result["checks"]["greeter -> session bar matches"] = pair_bar(video, events, INSTALLED_PAIRS[0], result)
        result["checks"]["greeter -> session wallpaper continuous"] = pair_wallpaper(
            video, events, INSTALLED_PAIRS[0], result
        )

    if "live desktop bar" in events:
        windows = [
            ("installer -> live desktop", "submit try bingux", "live desktop bar"),
            ("session -> lock", "lock requested", "lock screen shown"),
            ("lock -> unlock", "unlock submitted", "desktop after unlock"),
            ("unlock -> logout", "logout requested", "logout done"),
        ]
        for label, a, b in windows:
            if a not in events or b not in events:
                result["checks"][f"{label} is content"] = False
                result["defects"].append(f"{label}: missing timeline event {a!r} or {b!r}")
                continue
            defects = handover_defects(video, events[a], events[b])
            result["checks"][f"{label} is content"] = not defects
            result["defects"] += [f"{label} black/placeholder at {t:.1f}s ({k})" for t, k in defects]
        result["checks"]["session bar matches before lock and after unlock"] = pair_bar(
            video, events, BOOT_PAIRS[0], result
        )
        result["checks"]["session wallpaper continuous across lock"] = pair_wallpaper(
            video, events, BOOT_PAIRS[0], result
        )

    result["ok"] = bool(result["checks"]) and all(result["checks"].values())
    (out_dir / "handover.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def pair_bar(video, events, pair, result):
    a, b = pair
    if a not in events or b not in events:
        result["defects"].append(f"missing stage {a!r} or {b!r} for bar check")
        return False
    first = frame_at(video, events[a])
    second = frame_at(video, events[b])
    same = bar_matches(first, second)
    result["measures"][f"bar {a} vs {b}"] = same
    if not same:
        result["defects"].append(f"bar clock moved between {a!r} ({events[a]:.1f}s) and {b!r} ({events[b]:.1f}s)")
    return same


def pair_wallpaper(video, events, pair, result):
    a, b = pair
    first = frame_at(video, events[a])
    second = frame_at(video, events[b])
    jump = wallpaper_jump(first, second)
    result["measures"][f"wallpaper jump {a} vs {b}"] = round(jump, 2)
    if jump >= WALLPAPER_LIMIT:
        result["defects"].append(f"wallpaper jump {jump:.1f} between {a!r} and {b!r}")
        return False
    return True


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    ok = True
    for arg in argv:
        result = check_recording(Path(arg))
        print(f"{arg}: {'PASS' if result['ok'] else 'FAIL'}")
        for name, passed in result["checks"].items():
            print(("  ok   " if passed else "  FAIL ") + name)
        for defect in result["defects"]:
            print("  defect: " + defect)
        for name, value in result["measures"].items():
            print(f"  measure: {name} = {value}")
        ok = ok and result["ok"]
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
