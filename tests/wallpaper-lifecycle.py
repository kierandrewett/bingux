#!/usr/bin/env python3
"""Check real wallpaper decoding and crossfades in a private Wayland session."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from PIL import Image, ImageDraw


REPO = Path(__file__).resolve().parents[1]
if not os.environ.get("WAYLAND_DISPLAY", "").startswith(("gnoblin-devkit-", "gnoblin-gs-")):
    raise SystemExit("Run in a private Gnoblin devkit/test session")
with tempfile.TemporaryDirectory(prefix="bingux-wallpaper-") as temporary:
    fixture = Path(temporary)
    results = Path(os.environ.get("BINGUX_WALLPAPER_RESULTS", fixture / "results"))
    results.mkdir(parents=True, exist_ok=True)
    images = {}
    for name, size in (("landscape", (7680, 4320)), ("portrait", (4320, 7680))):
        variable = "BINGUX_WALLPAPER_" + name.upper()
        if os.environ.get(variable):
            images[variable] = os.environ[variable]
            continue
        image = Image.new("RGB", size)
        draw = ImageDraw.Draw(image)
        width, height = size
        for index, color in enumerate(("#e94e4e", "#4ed56a", "#4e79e9")):
            draw.rectangle((index * width // 3, 0, (index + 1) * width // 3, height), fill=color)
        draw.rectangle((width * 0.4, height * 0.4, width * 0.6, height * 0.6), fill="#fafafa")
        draw.line((0, height // 2, width, height // 2), fill="black", width=8)
        path = fixture / (name + ".jpg")
        image.save(path, quality=95)
        image.close()
        images[variable] = path.as_uri()
    source = Path(os.environ.get("BINGUX_WALLPAPER_SOURCE", REPO / "shell/bingux/Wallpaper.qml")).read_text()
    if "import QtQuick.Window" not in source:
        source = source.replace("import QtQuick\n", "import QtQuick\nimport QtQuick.Window\n", 1)
    # Observe the window's scene tree through an alias; it is not a QObject
    # descendant of the Variants container.
    source = source.replace("Variants {", "Variants {\n    id: probeRoot\n    property var testPanels: []", 1)
    source = source.replace(
        "id: window",
        "id: window\n        property alias testStage: stage\n        Component.onCompleted: probeRoot.testPanels.push(this)",
        1,
    )
    source = source.replace(
        "id: stage", "id: stage\n            readonly property real testDpr: Screen.devicePixelRatio", 1
    )
    (fixture / "Wallpaper.qml").write_text(source)
    (fixture / "qmldir").write_text("singleton BinguxPreferences 1.0 BinguxPreferences.qml\n")
    (fixture / "BinguxPreferences.qml").write_text(
        'pragma Singleton\nimport QtQuick\nQtObject { property url wallpaperUrl: "" }\n'
    )
    shutil.copy2(REPO / "tests/wallpaper-lifecycle.qml", fixture / "shell.qml")
    environment = os.environ | {
        **images,
        "BINGUX_WALLPAPER_RESULTS": str(results),
        "QT_QPA_PLATFORM": "wayland",
        "QT_SCALE_FACTOR": os.environ.get("BINGUX_WALLPAPER_SCALE", "1"),
        "QT_LOGGING_RULES": "qt.quick.image.debug=true",
    }
    samples = []
    with (results / "runtime.log").open("w") as log:
        process = subprocess.Popen(
            [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(fixture)], env=environment, stdout=log, stderr=log
        )
        try:
            deadline = time.monotonic() + 20
            while process.poll() is None and time.monotonic() < deadline:
                try:
                    values = dict(
                        line.split(":", 1)
                        for line in Path(f"/proc/{process.pid}/smaps_rollup").read_text().splitlines()
                        if ":" in line
                    )
                    samples.append(int(values["Pss"].split()[0]))
                except (FileNotFoundError, ProcessLookupError):
                    pass
                time.sleep(0.1)
            process.wait(timeout=1)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)
    output = (results / "runtime.log").read_text()
    print(output)
    print(f"Peak wallpaper process PSS: {max(samples, default=0) / 1024:.2f} MiB")
    if (
        process.returncode
        or "WALLPAPER_LIFECYCLE_PASSED" not in output
        or "WALLPAPER_INTERRUPTION_PASSED" not in output
        or "FAIL!" in output
    ):
        raise SystemExit(1)
    for name in ("landscape", "portrait"):
        if os.environ.get("BINGUX_WALLPAPER_" + name.upper()):
            continue
        with Image.open(results / (name + ".png")) as image:
            rgb = image.convert("RGB")
            for x, y, expected in (
                (0.1, 0.25, (233, 78, 78)),
                (0.9, 0.25, (78, 121, 233)),
                (0.5, 0.45, (250, 250, 250)),
            ):
                actual = rgb.getpixel((int(x * rgb.width), int(y * rgb.height)))
                assert max(abs(a - b) for a, b in zip(actual, expected)) < 12, (name, actual, expected)
    for shot in results.glob("interrupted-*.png"):
        with Image.open(shot) as image:
            alpha = image.convert("RGBA").getchannel("A")
            assert alpha.getextrema() == (255, 255), (shot.name, alpha.getextrema())
    print("PASS: display-sized decode, physical pixel resolution, crossfade, cropping and empty wallpaper")
