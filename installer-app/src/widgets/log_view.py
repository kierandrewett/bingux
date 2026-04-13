import gi

gi.require_version("Gtk", "4.0")

from gi.repository import Gtk, GLib, Pango


class LogView(Gtk.ScrolledWindow):
    def __init__(self):
        super().__init__()
        self.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)

        self.textview = Gtk.TextView()
        self.textview.set_editable(False)
        self.textview.set_cursor_visible(False)
        self.textview.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        self.textview.set_monospace(True)
        self.textview.add_css_class("card")
        self.textview.set_top_margin(8)
        self.textview.set_bottom_margin(8)
        self.textview.set_left_margin(12)
        self.textview.set_right_margin(12)

        self.buffer = self.textview.get_buffer()
        self.set_child(self.textview)

    def append(self, text):
        end = self.buffer.get_end_iter()
        self.buffer.insert(end, text)
        # Auto-scroll to bottom
        end = self.buffer.get_end_iter()
        self.textview.scroll_to_iter(end, 0.0, False, 0.0, 1.0)

    def clear(self):
        self.buffer.set_text("")
