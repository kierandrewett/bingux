"""Screenshots and the comparisons tests make on them.

Needs Pillow and numpy (the existing probes in scripts/ need them too).
"""

from __future__ import annotations

import csv
import io
import shutil
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image

Box = tuple[int, int, int, int]  # left, top, right, bottom; right and bottom are exclusive


@dataclass
class Screenshot:
    """A captured stage. `path` is where it was kept, if it was."""

    path: Path
    pixels: np.ndarray  # height x width x 3, uint8

    @classmethod
    def open(cls, path: Path) -> Screenshot:
        with Image.open(path) as image:
            return cls(path, np.asarray(image.convert("RGB")).copy())

    @property
    def size(self) -> tuple[int, int]:
        return self.pixels.shape[1], self.pixels.shape[0]

    def region(self, box: Box) -> np.ndarray:
        left, top, right, bottom = box
        return self.pixels[top:bottom, left:right]

    def distinct_colours(self, box: Box | None = None) -> int:
        """How many different colours a region holds. A black frame has one."""
        region = self.pixels if box is None else self.region(box)
        return len(np.unique(region.reshape(-1, 3), axis=0))

    def is_blank(self, box: Box | None = None, spread: int = 2) -> bool:
        """True when every channel of the region varies by at most `spread` levels."""
        region = self.pixels if box is None else self.region(box)
        return int((region.max(axis=(0, 1)) - region.min(axis=(0, 1))).max()) <= spread

    def mean_colour(self, box: Box | None = None) -> tuple[int, int, int]:
        region = self.pixels if box is None else self.region(box)
        r, g, b = region.reshape(-1, 3).mean(axis=0)
        return int(r), int(g), int(b)

    def changed_fraction(self, other: Screenshot, box: Box | None = None, tolerance: int = 8) -> float:
        """The fraction of pixels of `box` whose largest channel difference exceeds `tolerance`."""
        a = self.pixels if box is None else self.region(box)
        b = other.pixels if box is None else other.region(box)
        if a.shape != b.shape:
            raise ValueError(f"screenshots differ in size: {a.shape} against {b.shape}")
        difference = np.abs(a.astype(np.int16) - b.astype(np.int16)).max(axis=2)
        return float((difference > tolerance).mean())

    def save_crop(self, box: Box, path: Path) -> Path:
        left, top, right, bottom = box
        Image.fromarray(self.pixels[top:bottom, left:right]).save(path)
        return path

    def find_colour(self, rgb: tuple[int, int, int], tolerance: int = 6, box: Box | None = None) -> Box | None:
        """The bounding box of pixels near `rgb`, or None."""
        region = self.pixels if box is None else self.region(box)
        close = (np.abs(region.astype(np.int16) - np.array(rgb, dtype=np.int16)).max(axis=2)) <= tolerance
        ys, xs = np.nonzero(close)
        if not len(xs):
            return None
        offset_x, offset_y = (box[0], box[1]) if box else (0, 0)
        return (
            int(xs.min()) + offset_x,
            int(ys.min()) + offset_y,
            int(xs.max()) + offset_x + 1,
            int(ys.max()) + offset_y + 1,
        )

    # -- reading text --------------------------------------------------------

    def words(self, box: Box | None = None, scale: int = 2, min_confidence: float = 40.0) -> list[Word]:
        """The words Tesseract reads in a region, with their boxes in screenshot coordinates.

        The region is enlarged first: Tesseract reads 11 px interface text poorly at 1x.
        """
        tesseract = shutil.which("tesseract")
        if tesseract is None:
            raise OcrUnavailable("tesseract is not installed")
        left, top, right, bottom = box or (0, 0, *self.size)
        crop = Image.fromarray(self.pixels[top:bottom, left:right])
        crop = crop.resize((crop.width * scale, crop.height * scale), Image.LANCZOS)
        with tempfile.TemporaryDirectory(prefix="bxocr-") as directory:
            source = Path(directory) / "crop.png"
            crop.save(source)
            done = subprocess.run(
                [tesseract, str(source), "stdout", "--psm", "11", "tsv"], capture_output=True, text=True, check=False
            )
        found = []
        for row in csv.DictReader(io.StringIO(done.stdout), delimiter="\t", quoting=csv.QUOTE_NONE):
            text = (row.get("text") or "").strip()
            if not text or float(row.get("conf") or -1) < min_confidence:
                continue
            x, y, w, h = (int(row[key]) // scale for key in ("left", "top", "width", "height"))
            found.append(Word(text, (left + x, top + y, left + x + w, top + y + h)))
        return found

    def text(self, box: Box | None = None, scale: int = 2) -> str:
        return " ".join(word.text for word in self.words(box, scale))

    def find_text(self, phrase: str, box: Box | None = None, scale: int = 2) -> Box | None:
        """The box of the first run of words that spells `phrase` (case-insensitive), or None."""
        wanted = phrase.lower().split()
        found = self.words(box, scale)
        for start in range(len(found)):
            if [w.text.lower().strip(".,:;") for w in found[start : start + len(wanted)]] == [
                t.strip(".,:;") for t in wanted
            ]:
                run = found[start : start + len(wanted)]
                return (
                    min(w.box[0] for w in run),
                    min(w.box[1] for w in run),
                    max(w.box[2] for w in run),
                    max(w.box[3] for w in run),
                )
        return None


class OcrUnavailable(RuntimeError):
    pass


def ocr_available() -> bool:
    return shutil.which("tesseract") is not None


@dataclass(frozen=True)
class Word:
    text: str
    box: Box

    @property
    def centre(self) -> tuple[int, int]:
        return (self.box[0] + self.box[2]) // 2, (self.box[1] + self.box[3]) // 2


def centre(box: Box) -> tuple[int, int]:
    return (box[0] + box[2]) // 2, (box[1] + box[3]) // 2
