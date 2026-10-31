"""X11 keysym numbers for the keys tests press. Names follow keysymdef.h."""

from __future__ import annotations

NAMED = {
    "BackSpace": 0xFF08,
    "Tab": 0xFF09,
    "Return": 0xFF0D,
    "Escape": 0xFF1B,
    "Delete": 0xFFFF,
    "Home": 0xFF50,
    "Left": 0xFF51,
    "Up": 0xFF52,
    "Right": 0xFF53,
    "Down": 0xFF54,
    "Page_Up": 0xFF55,
    "Page_Down": 0xFF56,
    "End": 0xFF57,
    "Shift_L": 0xFFE1,
    "Control_L": 0xFFE3,
    "Alt_L": 0xFFE9,
    "Super_L": 0xFFEB,
    "space": 0x20,
}
NAMED.update({f"F{n}": 0xFFBD + n for n in range(1, 13)})


def keysym(name: str) -> int:
    """A keysym from a name (`Return`, `Alt_L`, `F2`) or a single character (`a`, `.`, `é`)."""
    if name in NAMED:
        return NAMED[name]
    if len(name) == 1:
        return keysym_for_character(name)
    raise KeyError(f"unknown key {name!r}; add it to tests/desktop/bxdesktop/keysyms.py")


def keysym_for_character(character: str) -> int:
    """Latin-1 characters are their own keysyms; the rest are 0x1000000 plus the code point."""
    code = ord(character)
    if character == "\n":
        return NAMED["Return"]
    if character == "\t":
        return NAMED["Tab"]
    return code if code < 0x100 else 0x1000000 + code
