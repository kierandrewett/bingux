"""Named seams in shell/bingux/shell.qml for tests that rewrite it.

Tests that splice production QML must locate their seams here, not with local
regexes. A seam that moves raises SpliceError naming the seam, so a rename shows
up as one clear message instead of an unrelated failure inside a fixture.
"""

from pathlib import Path
import re

REPO = Path(__file__).resolve().parents[2]
SHELL_DIR = REPO / "shell/bingux"
SHELL = SHELL_DIR / "shell.qml"

# Object ids that tests splice into or replace. Add a name here when a test
# needs a new seam; do not re-derive it inline.
TOP_BAR = "topBar"
BAR_OVERFLOW = "barOverflow"
DOCK = "dock"


class SpliceError(AssertionError):
    pass


def read(path=SHELL):
    return Path(path).read_text()


def _object_matches(text, object_id):
    return list(re.finditer(r"\b\w+\s*\{\s*id:\s*" + re.escape(object_id) + r"\b", text))


def _block_end(text, start):
    """Index just past the brace that closes the first '{' at or after start."""
    depth = 0
    index = text.index("{", start)
    while True:
        char = text[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index + 1
        index += 1


def object_block(text, object_id, label=None):
    """Return the full `Type { id: object_id ... }` block; SpliceError if absent or ambiguous."""
    matches = _object_matches(text, object_id)
    if len(matches) != 1:
        raise SpliceError(
            f"shell.qml seam moved: object 'id: {object_id}' found {len(matches)} times"
            f" (expected 1){' while locating ' + label if label else ''}"
        )
    start = matches[0].start()
    return text[start : _block_end(text, start)]


def id_line(text, object_id):
    """Return the exact `id: object_id` line, with indentation and newline, for a single-line seam."""
    lines = re.findall(r"^[ \t]*id:\s*" + re.escape(object_id) + r"[ \t]*\n", text, flags=re.M)
    if len(lines) != 1:
        raise SpliceError(f"shell.qml seam moved: line 'id: {object_id}' found {len(lines)} times (expected 1)")
    return lines[0]


def replace_once(text, old, new, seam):
    """Replace the single occurrence of old; SpliceError naming seam otherwise."""
    count = text.count(old)
    if count != 1:
        raise SpliceError(f"shell.qml seam moved: {seam} matched {count} times (expected 1)")
    return text.replace(old, new)


def replace_object(text, object_id, new):
    """Replace the whole block of one named object."""
    block = object_block(text, object_id)
    return replace_once(text, block, new, f"object {object_id}")


def insert_before_root_end(text, fragment, seam="root object"):
    """Insert fragment before the final '}' of the document (the root object's closer)."""
    end = text.rfind("}")
    if end < 0 or text[end + 1 :].strip():
        raise SpliceError(f"shell.qml seam moved: {seam} has no closing brace at end of file")
    return text[:end] + fragment + text[end:]
