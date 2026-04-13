import gi

gi.require_version("Gtk", "4.0")

from gi.repository import Gtk


class StepIndicator(Gtk.Box):
    """Horizontal step indicator: (1)──(2)──(3)──..."""

    def __init__(self, steps, labels=None):
        super().__init__(orientation=Gtk.Orientation.HORIZONTAL, spacing=0)
        self.set_halign(Gtk.Align.CENTER)
        self.set_margin_top(12)
        self.set_margin_bottom(12)

        self._steps = steps
        self._labels = labels or [str(i + 1) for i in range(steps)]
        self._circles = []
        self._lines = []
        self._step_labels = []

        for i in range(steps):
            # Step circle + label column
            col = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            col.set_halign(Gtk.Align.CENTER)

            circle = Gtk.Label(label=self._labels[i])
            circle.set_size_request(28, 28)
            circle.set_halign(Gtk.Align.CENTER)
            circle.set_valign(Gtk.Align.CENTER)
            self._circles.append(circle)
            col.append(circle)

            self.append(col)

            # Connector line (except after last)
            if i < steps - 1:
                line = Gtk.Separator(orientation=Gtk.Orientation.HORIZONTAL)
                line.set_size_request(48, -1)
                line.set_valign(Gtk.Align.CENTER)
                self._lines.append(line)
                self.append(line)

        self.set_current(0)

    def set_current(self, index):
        for i, circle in enumerate(self._circles):
            # Remove old classes
            for cls in ["accent", "success"]:
                circle.remove_css_class(cls)

            ctx = circle.get_style_context()

            if i < index:
                # Completed
                circle.set_label("\u2713")
                circle.add_css_class("success")
                self._apply_css(circle, """
                    label {
                        background: @success_bg_color;
                        color: @success_fg_color;
                        border-radius: 14px;
                        font-weight: bold;
                        font-size: 14px;
                    }
                """)
            elif i == index:
                # Current
                circle.set_label(self._labels[i])
                circle.add_css_class("accent")
                self._apply_css(circle, """
                    label {
                        background: @accent_bg_color;
                        color: @accent_fg_color;
                        border-radius: 14px;
                        font-weight: bold;
                        font-size: 12px;
                    }
                """)
            else:
                # Upcoming
                circle.set_label(self._labels[i])
                self._apply_css(circle, """
                    label {
                        background: alpha(@window_fg_color, 0.1);
                        color: alpha(@window_fg_color, 0.5);
                        border-radius: 14px;
                        font-size: 12px;
                    }
                """)

    def _apply_css(self, widget, css):
        provider = Gtk.CssProvider()
        provider.load_from_string(css)
        widget.get_style_context().add_provider(provider, 800)
