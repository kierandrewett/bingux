"""Bounded, geometry-only AT-SPI query. Never reads or stores input text."""

import json
import signal
import sys


def caret(pid):
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi

    Atspi.set_timeout(40, 40)
    desktop = Atspi.get_desktop(0)
    pending = []
    for index in range(min(desktop.get_child_count(), 128)):
        app = desktop.get_child_at_index(index)
        if app.get_process_id() == pid:
            pending.append(app)
    visited = 0
    while pending and visited < 512:
        node = pending.pop()
        visited += 1
        try:
            # Request extended properties from lazy accessibility trees. Older
            # Chromium versions still need native input geometry for glyph bounds.
            node.get_attributes()
            states = node.get_state_set()
            if states.contains(Atspi.StateType.FOCUSED) and states.contains(Atspi.StateType.SHOWING):
                text = node.get_text_iface()
                if text:
                    offset = text.get_caret_offset()
                    count = text.get_character_count()
                    if offset < 0 or not count:
                        continue
                    rect = text.get_character_extents(min(offset, count - 1), Atspi.CoordType.SCREEN)
                    if rect.height > 0 and rect.width >= 0:
                        parent = node
                        surface = None
                        for _ in range(32):
                            if parent.get_role() in (Atspi.Role.FRAME, Atspi.Role.DIALOG, Atspi.Role.WINDOW):
                                bounds = parent.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
                                surface = dict(x=bounds.x, y=bounds.y, width=bounds.width, height=bounds.height)
                                break
                            parent = parent.get_parent()
                            if not parent:
                                break
                        # At end-of-input use the trailing edge of the last glyph.
                        return dict(
                            x=rect.x + (rect.width if offset >= count else 0),
                            y=rect.y,
                            width=1,
                            height=rect.height,
                            source="caret",
                            surface=surface,
                        )
            for index in range(min(node.get_child_count(), 128)):
                child = node.get_child_at_index(index)
                if child:
                    pending.append(child)
        except Exception:
            continue
    return None


if __name__ == "__main__":
    # OS-enforced deadline also covers blocked GI/D-Bus calls.
    signal.setitimer(signal.ITIMER_REAL, 0.18)
    try:
        print(json.dumps(caret(int(sys.argv[1]))), flush=True)
    except Exception:
        print("null", flush=True)
