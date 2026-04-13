import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk


class BasePage(Adw.NavigationPage):
    def __init__(self, window, title, tag=None, show_title=False):
        super().__init__(title=title, tag=tag or title.lower().replace(" ", "-"))
        self.window = window
        self.state = window.state

        self.toolbar_view = Adw.ToolbarView()
        self.set_child(self.toolbar_view)

        # Header — hidden for most pages (step indicator is in the window)
        header = Adw.HeaderBar()
        header.set_show_back_button(False)
        if not show_title:
            header.set_show_title(False)
            header.set_visible(False)
        self.toolbar_view.add_top_bar(header)

        # Scrollable content, vertically centered
        self.content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        self.content.set_margin_start(48)
        self.content.set_margin_end(48)
        self.content.set_margin_top(24)
        self.content.set_margin_bottom(24)
        self.content.set_valign(Gtk.Align.CENTER)
        self.content.set_halign(Gtk.Align.CENTER)
        self.content.set_size_request(500, -1)

        scroll = Gtk.ScrolledWindow(vexpand=True, hscrollbar_policy=Gtk.PolicyType.NEVER)
        scroll.set_child(self.content)
        self.toolbar_view.set_content(scroll)

    def add_nav_buttons(self, next_label="Next", show_back=True):
        """Add sticky bottom bar: Back on left, Next on right."""
        bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        bar.set_margin_start(24)
        bar.set_margin_end(24)
        bar.set_margin_top(8)
        bar.set_margin_bottom(12)

        if show_back:
            back_btn = Gtk.Button(label="Back")
            back_btn.add_css_class("pill")
            back_btn.connect("clicked", lambda _: self.window.go_back())
            bar.append(back_btn)

        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        bar.append(spacer)

        self.next_btn = Gtk.Button(label=next_label)
        self.next_btn.add_css_class("pill")
        self.next_btn.add_css_class("suggested-action")
        self.next_btn.connect("clicked", lambda _: self._on_next())
        bar.append(self.next_btn)

        self.toolbar_view.add_bottom_bar(bar)

    def _on_next(self):
        if self.validate():
            self.on_leave()
            self.window.go_next()

    def should_show(self):
        return True

    def validate(self):
        return True

    def on_enter(self):
        pass

    def on_leave(self):
        pass
