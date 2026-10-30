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
  "desktop painted" is content; the bar clock box matches between the two stages.
  The greeter shows the blurred wallpaper twin under a scrim, and the session
  paints the same twin first, then fades to the sharp wallpaper. The gated
  wallpaper check compares the greeter with the session's first painted frame
  (session_first_frame); the greeter-to-desktop jump after the fade is reported only;
- installer -> live desktop (boot run): every frame from "submit try bingux" to
  "live desktop bar" is content;
- session -> lock -> unlock -> logout (boot run): every frame from "lock requested"
  to "lock screen shown", from "unlock submitted" to "desktop after unlock" and
  from "logout requested" to "logout done" is content; the bar clock box matches
  between the desktop before lock and after unlock; the wallpaper corner matches
  within WALLPAPER_LIMIT.

The black-gap check lists black runs outside the firmware reset; the installer's
shutdown after Restart is reported as a black run too, since the screen is blank
before the firmware takes over.

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
    session_first_frame,
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
        result["checks"]["greeter -> session wallpaper matches"] = pair_session_wallpaper(
            video, out_dir, events, result
        )
        measure_wallpaper(video, events, INSTALLED_PAIRS[0], result, note=" (after the sharp fade-in, not gated)")

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


def measure_wallpaper(video, events, pair, result, note=""):
    a, b = pair
    jump = wallpaper_jump(frame_at(video, events[a]), frame_at(video, events[b]))
    result["measures"][f"wallpaper jump {a} vs {b}{note}"] = round(jump, 2)
    return jump


def pair_wallpaper(video, events, pair, result):
    """Gated: the wallpaper corner must not change between the two stages."""
    jump = measure_wallpaper(video, events, pair, result)
    if jump >= WALLPAPER_LIMIT:
        result["defects"].append(f"wallpaper jump {jump:.1f} between {pair[0]!r} and {pair[1]!r}")
        return False
    return True


def pair_session_wallpaper(video, out_dir, events, result):
    """Gated: the greeter's wallpaper matches the session's first painted frame within WALLPAPER_LIMIT."""
    start, end = events["greeter shown"], events["desktop painted"]
    first = session_first_frame(out_dir, start, end)
    if first is None:
        result["defects"].append("no session first frame: no black update between 'greeter shown' and 'desktop painted'")
        return False
    jump = wallpaper_jump(frame_at(video, start), frame_at(video, first))
    result["measures"]["wallpaper jump greeter shown vs session first frame"] = round(jump, 2)
    if jump >= WALLPAPER_LIMIT:
        result["defects"].append(f"wallpaper jump {jump:.1f} between 'greeter shown' and the session's first frame at {first:.1f}s")
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
