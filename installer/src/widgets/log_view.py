import gi

gi.require_version("Gtk", "4.0")

from gi.repository import Gtk, Pango


class LogView(Gtk.ScrolledWindow):
    def __init__(self):
        super().__init__()
        self.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        self.set_vexpand(True)
        self.set_size_request(-1, 350)

        self.textview = Gtk.TextView()
        self.textview.set_editable(False)
        self.textview.set_cursor_visible(False)
        self.textview.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        self.textview.add_css_class("card")
        self.textview.set_top_margin(12)
        self.textview.set_bottom_margin(12)
        self.textview.set_left_margin(16)
        self.textview.set_right_margin(16)

        # Explicit monospace font via Pango
        font_desc = Pango.FontDescription.from_string("JetBrains Mono 10")
        self.textview.override_font(font_desc)

        self.buffer = self.textview.get_buffer()
        self.set_child(self.textview)

    def append(self, text):
        end = self.buffer.get_end_iter()
        self.buffer.insert(end, text)
        end = self.buffer.get_end_iter()
        self.textview.scroll_to_iter(end, 0.0, False, 0.0, 1.0)

    def clear(self):
        self.buffer.set_text("")
