#!/usr/bin/env python3
"""Exercise the editor's real wallpaper Image block offscreen."""

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import textwrap

from PIL import Image


REPO = Path(__file__).resolve().parents[1]


def editor_image(source: str) -> str:
    canvas = source.index("id: canvas")
    match = re.search(r"\n        Image \{(?P<body>.*?)\n        \}\n        Rectangle \{", source[canvas:], re.DOTALL)
    if not match:
        raise SystemExit("editor wallpaper Image block was not found")
    body = textwrap.dedent(match.group("body")).strip()
    # The pre-fix block had no id. Add one only to let the fixture observe the
    # same Image instance; its loading and decoding bindings are unchanged.
    if "id: editorWallpaper" not in body:
        body = "id: editorWallpaper\n" + body
    return textwrap.indent(body, "        ")


def qml(image_block: str, landscape: str, portrait: str, expected_dpr: int) -> str:
    return f'''import QtQuick
import QtQuick.Window
import QtTest

Item {{
    id: root
    property url wallpaper: ""
    property url landscape: "{landscape}"
    property url portrait: "{portrait}"
    property real expectedDpr: {expected_dpr}

    Item {{
        id: canvas
        width: 0
        height: 0
        Image {{
{image_block}
        }}
    }}

    TestCase {{
        name: "EditorWallpaperLifecycle"
        when: true

        function checkDecoded(label, expectedWidth, expectedHeight) {{
            tryCompare(editorWallpaper, "status", Image.Ready, 5000);
            const physicalWidth = Math.ceil(canvas.width * Screen.devicePixelRatio);
            const physicalHeight = Math.ceil(canvas.height * Screen.devicePixelRatio);
            compare(editorWallpaper.implicitWidth, expectedWidth);
            compare(editorWallpaper.implicitHeight, expectedHeight);
            compare(editorWallpaper.sourceSize.width, physicalWidth);
            compare(editorWallpaper.sourceSize.height, physicalHeight);
            console.info("EDITOR_WALLPAPER_DECODE", label, Screen.devicePixelRatio, editorWallpaper.sourceSize, editorWallpaper.implicitWidth, editorWallpaper.implicitHeight);
        }}

        function test_decode_is_bounded() {{
            compare(Screen.devicePixelRatio, root.expectedDpr);
            canvas.width = 640;
            canvas.height = 360;
            root.wallpaper = root.landscape;
            checkDecoded("landscape", 640 * root.expectedDpr, 360 * root.expectedDpr);

            root.wallpaper = root.portrait;
            checkDecoded("portrait", 640 * root.expectedDpr, Math.ceil(640 * root.expectedDpr * 16 / 9));
        }}

        function test_zero_geometry_keeps_source_null() {{
            canvas.width = 0;
            canvas.height = 0;
            root.wallpaper = root.landscape;
            wait(100);
            compare(editorWallpaper.source.toString(), "");
            compare(editorWallpaper.status, Image.Null);
        }}

        function cleanupTestCase() {{
            Qt.quit();
        }}
    }}
}}
'''


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", default="worktree", help="worktree or a git revision")
    args = parser.parse_args()
    if not shutil.which("qmltestrunner"):
        raise SystemExit("qmltestrunner is required")
    if args.source == "worktree":
        source = (REPO / "shell/bingux/DesktopCustomise.qml").read_text()
    else:
        source = subprocess.run(
            ["git", "show", f"{args.source}:shell/bingux/DesktopCustomise.qml"],
            cwd=REPO,
            check=True,
            capture_output=True,
            text=True,
        ).stdout

    with tempfile.TemporaryDirectory(prefix="bingux-editor-wallpaper-") as temporary:
        fixture = Path(temporary)
        landscape = fixture / "landscape.png"
        portrait = fixture / "portrait.png"
        Image.new("RGB", (7680, 4320), "#e94e4e").save(landscape)
        Image.new("RGB", (4320, 7680), "#4e79e9").save(portrait)
        image_block = editor_image(source)

        for scale in (1, 2):
            test_file = fixture / f"tst_editor_wallpaper_{scale}.qml"
            test_file.write_text(qml(image_block, landscape.as_uri(), portrait.as_uri(), scale))
            environment = os.environ | {
                "QT_QPA_PLATFORM": "offscreen",
                "QT_SCALE_FACTOR": str(scale),
            }
            result = subprocess.run(
                ["qmltestrunner", "-input", str(test_file)],
                env=environment,
                text=True,
                capture_output=True,
            )
            print(result.stdout, end="")
            print(result.stderr, end="")
            if result.returncode:
                raise SystemExit(result.returncode)

    print("PASS: editor wallpaper source is geometry-gated and decoded at physical canvas size")


if __name__ == "__main__":
    main()
