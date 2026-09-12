#!/usr/bin/env python3
"""Render real app badges and verify transparent clearance, including wide counts."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
from PIL import Image

repo = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="bingux-cutouts-") as directory:
    output = Path(directory)
    Image.new("RGBA", (96, 96), (255, 0, 255, 255)).save(output / "icon.png")
    qml = output / "shell.qml"
    qml.write_text(
        Path(__file__).with_suffix(".qml").read_text().replace("../shell/bingux", (repo / "shell/bingux").as_uri())
    )
    environment = os.environ | {
        "BINGUX_COMPOSITOR": "portable",
        "BINGUX_TEST_ICON": (output / "icon.png").as_uri(),
        "BINGUX_TEST_OUTPUT": directory,
    }
    result = subprocess.run(
        [os.environ.get("QS_TEST_BIN", "qs"), "-p", str(qml)],
        env=environment,
        capture_output=True,
        text=True,
        timeout=30,
    )
    log = result.stdout + result.stderr
    records = [json.loads(line.split("CUTOUT_SAMPLE ", 1)[1]) for line in log.splitlines() if "CUTOUT_SAMPLE " in line]
    assert result.returncode == 0 and len(records) == 4, log
    for record in records:
        image = Image.open(record["path"]).convert("RGBA")
        assert image.getpixel((60, 60)) == (255, 0, 255, 255), "Mask must preserve the icon away from badges"
        for sample in record["samples"]:
            x = int(sample["x"])
            alpha = image.getpixel((x, int(sample["gapY"])))[3]
            if sample["opacity"] > 0.99:
                assert alpha < 20, (record, "Badge clearance must be transparent")
            else:
                assert 0 < alpha < 245, (record, alpha, "Fading badges must gradually restore the icon")
            assert image.getpixel((x, int(sample["iconY"])))[3] > 240, (
                record,
                "Cutout must not remove neighbouring icon pixels",
            )
        if record["phase"] == 3:
            assert image.getpixel((116, 50))[3] == 255, "Hiding badges must restore icon pixels"
    assert [len(record["samples"]) for record in records] == [1, 2, 2, 0]
    print("PASS: transparent badge clearance, wide counts, audio and external badges, fades, and removal")
