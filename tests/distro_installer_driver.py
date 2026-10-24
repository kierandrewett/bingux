"""Drives the graphical installer from the guest screen, the way a person would.

The installer is a kiosk on the live session. Nothing here reads its state: the
driver looks at the newest VNC frame, finds text with tesseract, and acts with
the VNC pointer (absolute tablet, see distro_vm.Vm pointer=True) and the
keyboard (Vm.type_text). Positions come from the frame, never from constants,
except the time zone map, which is placed from its label and the card geometry
(the card is centred on the screen).

Page identity: the card title is the largest text on a page, so the page is the
title with the largest glyph height among the known page titles.

Text is matched as a phrase. A reading that stands alone on its line (a caption,
title or button) and reads exactly as written beats the same words inside a
sentence, so the button "Continue" is never mistaken for "can continue" in body
text, and the field caption "Passphrase" never for the note above it.
"""

from __future__ import annotations

from dataclasses import dataclass
import io
import re
import struct
import subprocess
import time

from PIL import Image, ImageOps
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
SWITCH_WIDTH = 36  # ControlSwitch track width (shell/bingux/ControlSwitch.qml)
THUMB_WIDTH = 16  # ControlSwitch thumb; it sits THUMB_INSET from the track end
THUMB_INSET = 2
THUMB_BRIGHT = 200  # the thumb is near white in both states; nothing right of a switch is that bright
FIELD_DROP = 26  # caption glyph bottom to the middle of the 36 px field below it
FIELD_INSET = 60  # from a caption's left edge into the field


@dataclass
class Word:
    text: str
    x0: float
    y0: float
    x1: float
    y1: float
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


def _plain(text: str) -> str:
    """Letters and digits, case kept: the exact-reading test."""
    return re.sub(r"[^A-Za-z0-9]", "", text)


def _tsv_words(image: Image.Image, scale: int, psm: str, x_offset: float = 0, y_offset: float = 0) -> list[Word]:
    """Run tesseract on an already-upscaled image; boxes come back in frame coordinates. The offsets place
    the image origin in the frame; a margin added to the image is removed by a negative offset. The PNG goes
    to tesseract on stdin: a temp file would land in /tmp, whose per-user quota OCR can exhaust."""
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    out = subprocess.run(
        ["tesseract", "stdin", "stdout", "--psm", psm, "tsv"],
        input=buffer.getvalue(),
        capture_output=True,
        check=False,
    ).stdout.decode(errors="replace")
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
                x0=left / scale + x_offset,
                y0=top / scale + y_offset,
                x1=(left + width) / scale + x_offset,
                y1=(top + height) / scale + y_offset,
                height=height / scale,
                conf=conf,
            )
        )
    return words


def ocr_words(frame: np.ndarray, scale: int = 2, invert: bool = False, binarize: bool = False) -> list[Word]:
    """Word boxes in frame coordinates. Upscaled, since tesseract is weak on 14 px UI text."""
    image = Image.fromarray(frame).convert("L")
    if invert:
        image = Image.fromarray(255 - np.asarray(image))
    if binarize:
        image = Image.fromarray(np.where(np.asarray(image) > 100, 0, 255).astype("uint8"))
    image = image.resize((image.width * scale, image.height * scale), Image.LANCZOS)
    return _tsv_words(image, scale, "11")


def ocr_bands(frame: np.ndarray, threshold: int, height: int = 60, stride: int = 30, scale: int = 4, pad: int = 8) -> list[Word]:
    """Thresholded bands read one line at a time. Page-level OCR misses light labels on dark buttons
    (Continue). A 60 px band, LANCZOS-upscaled with a white margin and read with psm 7, finds them.
    Light text (about 248) on a button fill (about 64) splits cleanly at 156; the grey button border
    (about 149) then stays white, where a cut-off of 100 turned it black and hid the label.
    Bands overlap, so a line that straddles one band sits whole in the next. About 5 s per frame."""
    gray = Image.fromarray(frame).convert("L")
    words: list[Word] = []
    for top in range(0, gray.height - height + 1, stride):
        band = gray.crop((0, top, gray.width, top + height))
        binary = Image.fromarray(np.where(np.asarray(band) > threshold, 0, 255).astype("uint8"))
        image = binary.resize((band.width * scale, band.height * scale), Image.LANCZOS)
        image = ImageOps.expand(image, border=pad * scale, fill=255)
        words += _tsv_words(image, scale, "7", x_offset=-pad, y_offset=top - pad)
    return words


def _passes(frame: np.ndarray):
    """Word lists from the normal, inverted and binarized frame, one OCR run each."""
    yield ocr_words(frame)
    yield ocr_words(frame, invert=True)
    yield ocr_words(frame, binarize=True)


def _sources(frame: np.ndarray):
    """As _passes, then the thresholded bands at three cut-offs, tried in turn: 156 for light text on a dark
    fill, 200 for white text on a light fill (the Continue fill of the installed build before the darker
    wide-card accent; 230 reads it as "Sonia"), then 100 as the slow last resort."""
    yield from _passes(frame)
    yield ocr_bands(frame, 156)
    yield ocr_bands(frame, 200)
    yield ocr_bands(frame, 100)


@dataclass
class Match:
    box: Word
    exact: bool  # the words read exactly as written, case included
    alone: bool  # nothing else sits beside them on their line

    @property
    def rank(self) -> tuple[bool, bool, float]:
        return (self.alone, self.exact, self.box.y1)


def _beside(word: Word, box: Word) -> bool:
    """`word` shares the line with `box` and is close enough to read as the same sentence. A glyph with no
    letter or digit (the back chevron beside a page title) is not text and never counts."""
    if not _norm(word.text) or abs(word.cy - box.cy) >= box.height / 2:
        return False
    gap = max(word.x0 - box.x1, box.x0 - word.x1)
    return gap < 2 * box.height


def find_matches(words: list[Word], phrase: str) -> list[Match]:
    """Every run of consecutive words that spell `phrase`, case and punctuation ignored for the match."""
    target = [part for part in phrase.split() if _norm(part)]
    if not target:
        return []
    tokens = [_norm(word.text) for word in words]
    matches: list[Match] = []
    for start, token in enumerate(tokens):
        if token != _norm(target[0]):
            continue
        run = [start]
        cursor = start + 1
        for part in target[1:]:
            while cursor < len(tokens) and not tokens[cursor]:
                cursor += 1
            if cursor >= len(tokens) or tokens[cursor] != _norm(part):
                break
            run.append(cursor)
            cursor += 1
        else:
            members = [words[i] for i in run]
            box = Word(
                text=phrase,
                x0=min(w.x0 for w in members),
                y0=min(w.y0 for w in members),
                x1=max(w.x1 for w in members),
                y1=max(w.y1 for w in members),
                height=max(w.height for w in members),
                conf=min(w.conf for w in members),
            )
            alone = not any(i not in run and _beside(word, box) for i, word in enumerate(words))
            exact = [_plain(w.text) for w in members] == [_plain(part) for part in target]
            matches.append(Match(box, exact, alone))
    return matches


def best_match(matches: list[Match]) -> Match | None:
    return max(matches, key=lambda match: match.rank, default=None)


def text_on_screen(frame: np.ndarray, phrase: str) -> Word | None:
    """Where `phrase` is on screen, or None. Returns early on a clean reading: exact, and either
    alone on its line or a phrase of several words. Otherwise the best reading across all sources."""
    multi = len(phrase.split()) > 1
    best: Match | None = None
    for words in _sources(frame):
        found = best_match(find_matches(words, phrase))
        if found is not None and (best is None or found.rank > best.rank):
            best = found
        if best is not None and best.exact and (best.alone or multi):
            break
    return best.box if best is not None else None


def _title_in(words: list[Word]) -> str | None:
    """The known page title among `words`: one that reads exactly as written and stands alone on its
    line, so body text that merely contains a title word never counts. The tallest such title wins."""
    best, best_height = None, 0.0
    for title in PAGE_TITLES:
        found = best_match(find_matches(words, title))
        if found is None or not (found.exact and found.alone):
            continue
        if found.box.height > best_height:
            best, best_height = title, found.box.height
    return best


def page_title(frame: np.ndarray) -> str | None:
    """The page title on screen (one of PAGE_TITLES), or None. The first OCR pass that finds one decides."""
    for words in _passes(frame):
        title = _title_in(words)
        if title is not None:
            return title
    return None


def error_page_up(frame: np.ndarray) -> bool:
    """Whether the installer's error page is up. Its title is one of the Schema.errorTitle messages rather
    than a page title, so call this only when page_title finds nothing. Its primary button reads Retry,
    and no other installer page has that word."""
    return text_on_screen(frame, "Retry") is not None


def headline(frame: np.ndarray) -> str:
    """The tallest line of text on screen, for messages (an error page's title), or an empty string."""
    words = ocr_words(frame)
    if not words:
        return ""
    tallest = max(w.height for w in words)
    return " ".join(w.text for w in words if w.height >= 0.9 * tallest)


def screen_name(frame: np.ndarray | None) -> str:
    """What the installer shows, for messages: the page title, else the headline."""
    if frame is None:
        return "no picture"
    return page_title(frame) or headline(frame) or "nothing readable"


def zone_on_label(frame: np.ndarray, zone_names: list[str]) -> str | None:
    """The zone the 'Time zone:' line names, or None. Letters only are compared, so OCR spacing and
    punctuation do not matter; the longest name found wins (Etc/GMT+1 over Etc/GMT)."""
    for words in _passes(frame):
        labels = [w for w in words if w.text.lower().startswith("zone")]
        if not labels:
            continue
        label = max(labels, key=lambda w: w.y0)
        line = sorted((w for w in words if abs(w.cy - label.cy) < label.height / 2), key=lambda w: w.x0)
        text = _norm("".join(w.text for w in line))
        found = [name for name in zone_names if _norm(name) in text]
        if found:
            return max(found, key=len)
    return None


def switch_x(frame: np.ndarray, y: float) -> float:
    """x of the switch centre on row `y`, found from its thumb. The track's colour is not used: an off track is
    nearly the card fill, and the card fill shifts with the wallpaper. The thumb is the rightmost near-white
    column in the row band. Off, the thumb is at the track's left end, so the track centre is
    SWITCH_WIDTH / 2 - (THUMB_INSET + THUMB_WIDTH / 2) to its right; on, it is the same distance to its left.
    A blue track left of the thumb means on."""
    row = int(round(y))
    band = frame[max(0, row - 4): row + 5].astype(int)
    columns = np.flatnonzero(np.any(band.min(axis=2) >= THUMB_BRIGHT, axis=0))
    if columns.size == 0:
        raise RuntimeError(f"no switch thumb on row {row}")
    right = int(columns.max())
    thumb = right - (THUMB_WIDTH - 1) / 2
    pixel = frame[row, right - 25].astype(int)
    on = int(pixel.max() - pixel.min()) > 25
    offset = SWITCH_WIDTH / 2 - (THUMB_INSET + THUMB_WIDTH / 2)
    return thumb - offset if on else thumb + offset


MAP_OCEAN = (30, 31, 34)  # the rendered sea of the time zone map: black at 30% over the card (shell/bingux InstallerTimezonePage.qml)


def map_rect(frame: np.ndarray) -> tuple[int, int, int, int]:
    """The time zone map as (left, top, right, bottom) in screen pixels: the bounding box of its sea colour,
    which covers every map pixel that is neither land nor a city dot. The list and the label never use it."""
    ys, xs = np.nonzero(np.all(np.abs(frame.astype(int) - MAP_OCEAN) <= 3, axis=2))
    if xs.size == 0:
        raise RuntimeError("time zone map not on screen")
    return int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1


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

    def current_page(self) -> str:
        return screen_name(self.frame())

    def click_text(self, phrase: str, timeout: float = 30, dx: float = 0, dy: float = 0) -> bool:
        box = self.wait_text(phrase, timeout)
        if box is None:
            self.log(f"  no text '{phrase}' on screen")
            return False
        self.pointer.click(box.cx + dx, box.cy + dy)
        return True

    def click_switch(self, label: str, timeout: float = 30, last_line: str | None = None) -> bool:
        """Toggle the switch on the settings row whose label starts with `label`. The switch sits at the
        right edge of the card, on the row of its label. A label that wraps passes `last_line`, a phrase on
        its final line; the switch is centred on the whole label block, from `label` down to it."""
        start = self.wait_text(label, timeout)
        if start is None:
            self.log(f"  no switch label '{label}' on screen")
            return False
        top, bottom = start.y0, start.y1
        if last_line is not None:
            end = self.wait_text(last_line, timeout)
            if end is None:
                self.log(f"  no line '{last_line}' under switch label '{label}'")
                return False
            top = min(top, end.y0)
            bottom = max(bottom, end.y1)
        frame = self.frame()
        self.pointer.click(switch_x(frame, (top + bottom) / 2), (top + bottom) / 2)
        return True

    def click_field(self, caption: str, timeout: float = 30) -> bool:
        """Focus the input box under `caption`: the field starts at the caption's left edge."""
        box = self.wait_text(caption, timeout)
        if box is None:
            self.log(f"  no field caption '{caption}' on screen")
            return False
        self.pointer.click(box.x0 + FIELD_INSET, box.y1 + FIELD_DROP)
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
