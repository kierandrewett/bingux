"""Drives the graphical installer from the guest screen, the way a person would.

The installer is a kiosk on the live session. Nothing here reads its state: the
driver looks at the newest VNC frame, finds text with tesseract, and acts with
the VNC pointer (absolute tablet, see distro_vm.Vm pointer=True) and the
keyboard (Vm.type_text). Positions come from the frame, never from constants,
except for the timezone map which is located from its caption.

Page identity: the card title is the largest text on a page, so the page is
the title word with the largest glyph height among the known page titles.
"""

from __future__ import annotations

from dataclasses import dataclass
import re
import struct
import subprocess
import tempfile
import time

from PIL import Image
import numpy as np

from distro_capture import connect_vnc

PAGE_TITLES = [
    "Welcome to Bingux",
    "Keyboard",
    "Network",
    "Time zone",
    "Computer name",
    "Disk",
    "Storage",
    "Your account",
    "Ready to install",
    "Installing Bingux",
    "Bingux is installed",
]
ERROR_TITLE_MARKER = "could not"  # the error page title; checked as a fallback


@dataclass
class Word:
    text: str
    def __init__(self, path):
        self.conn = connect_vnc(path)
        self.conn.settimeout(5)
        # Pointer coordinates are in the server's current desktop size; the
        # Installer keeps this in step with the newest frame.
        self.size = (1280, 800)

    def _send(self, buttons: int, x: float, y: float) -> None:
        width, height = self.size
        x = max(0, min(width - 1, int(round(x))))
        y = max(0, min(height - 1, int(round(y))))
        self.conn.send(struct.pack(">BBHH", 5, buttons, x, y))
    height: float
    conf: float

    @property
    def cx(self) -> float:
        return (self.x0 + self.x1) / 2

    @property
    def cy(self) -> float:
        return (self.y0 + self.y1) / 2


def _norm(text: str) -> str:
    return re.sub(r"[^a-z0-9]", "", text.lower())


def ocr_words(frame: np.ndarray, scale: int = 2, invert: bool = False) -> list[Word]:
    """Word boxes in frame coordinates. Upscaled, since tesseract is weak on 14 px UI text."""
    image = Image.fromarray(frame).convert("L")
    if invert:
        image = Image.fromarray(255 - np.asarray(image))
    image = image.resize((image.width * scale, image.height * scale), Image.LANCZOS)
    with tempfile.NamedTemporaryFile(suffix=".png") as handle:
        image.save(handle.name)
        out = subprocess.run(
            ["tesseract", handle.name, "stdout", "--psm", "11", "tsv"],
            capture_output=True,
            text=True,
            check=False,
        ).stdout
    words = []
    for row in out.splitlines()[1:]:
        cols = row.split("\t")
        if len(cols) < 12 or not cols[11].strip():
            continue
        conf = float(cols[10])
        if conf < 35:
            continue
        left, top, width, height = (int(v) for v in cols[6:10])
        words.append(
            Word(
                text=cols[11].strip(),
                x0=left / scale,
                y0=top / scale,
                x1=(left + width) / scale,
                y1=(top + height) / scale,
                height=height / scale,
                conf=conf,
            )
        )
    return words


def find_phrase(words: list[Word], phrase: str) -> Word | None:
    """The union box of the first run of consecutive words that spell `phrase`, as one Word."""
    target = [_norm(part) for part in phrase.split() if _norm(part)]
    if not target:
        return None
    tokens = [(index, _norm(word.text)) for index, word in enumerate(words)]
    for start, (index, token) in enumerate(tokens):
        if token != target[0]:
            continue
        run = [words[index]]
        cursor = start + 1
        for part in target[1:]:
            while cursor < len(tokens) and not tokens[cursor][1]:
                cursor += 1
            if cursor >= len(tokens) or tokens[cursor][1] != part:
                break
            run.append(words[tokens[cursor][0]])
            cursor += 1
        else:
            return Word(
                text=phrase,
                x0=min(w.x0 for w in run),
                y0=min(w.y0 for w in run),
                x1=max(w.x1 for w in run),
                y1=max(w.y1 for w in run),
                height=max(w.height for w in run),
                conf=min(w.conf for w in run),
            )
    return None


def text_on_screen(frame: np.ndarray, phrase: str) -> Word | None:
    """Look for `phrase` on a normal and on an inverted copy of the frame (light and dark text)."""
    found = find_phrase(ocr_words(frame), phrase)
    if found is None:
        found = find_phrase(ocr_words(frame, invert=True), phrase)
    return found


def page_title(frame: np.ndarray) -> str | None:
    """The page title on screen: the matching title with the largest glyphs, or None."""
    words = ocr_words(frame)
    best, best_height = None, 0.0
    for title in PAGE_TITLES:
        box = find_phrase(words, title)
        if box is not None and box.height > best_height:
            best, best_height = title, box.height
    return best


class Pointer:
    """Absolute VNC pointer: a second RFB client on the VM's VNC socket (shared session).

    `frame` returns the newest framebuffer, so clamping follows the live screen
    size (the size at connect time is stale once the installer changes mode).
    """

    def __init__(self, path, frame):
        self.conn = connect_vnc(path)
        self.conn.settimeout(5)
        self.frame = frame

    def _send(self, buttons: int, x: float, y: float) -> None:
        current = self.frame()
        height, width = current.shape[:2] if current is not None else self.conn.size[::-1]
        x = max(0, min(width - 1, int(round(x))))
        y = max(0, min(height - 1, int(round(y))))
        self.conn.send(struct.pack(">BBHH", 5, buttons, x, y))

    def move(self, x: float, y: float) -> None:
        self._send(0, x, y)
        time.sleep(0.05)

    def click(self, x: float, y: float) -> None:
        self.move(x, y)
        self._send(1, x, y)
        time.sleep(0.08)
        self._send(0, x, y)
        time.sleep(0.15)

    def close(self) -> None:
        self.conn.close()


class Installer:
    """Page-level actions on the installer kiosk. `vm` supplies frames and typing."""

    def __init__(self, vm, pointer: Pointer, log=print):
        self.vm = vm
        self.pointer = pointer
        self.log = log

    def frame(self):
        return self.vm.frame()

    def wait_text(self, phrase: str, timeout: float = 60, poll: float = 1.0) -> Word | None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            frame = self.frame()
            if frame is not None:
                box = text_on_screen(frame, phrase)
                if box is not None:
                    return box
            time.sleep(poll)
        return None

    def wait_page(self, title: str, timeout: float = 120) -> bool:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            frame = self.frame()
            if frame is not None and page_title(frame) == title:
                return True
            time.sleep(1.0)
        return False

    def current_page(self) -> str | None:
        frame = self.frame()
        return page_title(frame) if frame is not None else None

    def click_text(self, phrase: str, timeout: float = 30, dx: float = 0, dy: float = 0) -> bool:
        box = self.wait_text(phrase, timeout)
        if box is None:
            self.log(f"  no text '{phrase}' on screen")
            return False
        self.pointer.click(box.cx + dx, box.cy + dy)
        return True

    def click_field(self, caption: str, timeout: float = 30) -> bool:
        """Focus the input box under `caption`. The caption sits 4 px above a 36 px field."""
        box = self.wait_text(caption, timeout)
        if box is None:
            self.log(f"  no field caption '{caption}' on screen")
            return False
        self.pointer.click(box.x0 + 60, box.y1 + 4 + 18 + 4)
        return True

    def type_into(self, caption: str, text: str, clear: bool = False) -> bool:
        if not self.click_field(caption):
            return False
        if clear:
            self.vm.monitor("sendkey ctrl-a")
            self.vm.key("backspace")
        self.vm.type_text(text)
        return True

    def continue_page(self) -> None:
        self.click_text("Continue", timeout=20)
