import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk, GLib
from pages.base_page import BasePage
from backend import github


class AuthPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "GitHub Authentication", tag="auth")

        self.status = Adw.StatusPage()
        self.status.set_icon_name("dialog-password-symbolic")
        self.status.set_title("Sign in to GitHub")
        self.status.set_description(
            "Authenticate with GitHub to access your NixOS configuration repository."
        )
        self.content.append(self.status)

        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.CENTER)

        self.login_btn = Gtk.Button(label="Sign in with Browser")
        self.login_btn.add_css_class("pill")
        self.login_btn.add_css_class("suggested-action")
        self.login_btn.connect("clicked", self._on_login)
        btn_box.append(self.login_btn)

        self.refresh_btn = Gtk.Button(label="Check Status")
        self.refresh_btn.add_css_class("pill")
        self.refresh_btn.connect("clicked", self._on_refresh)
        btn_box.append(self.refresh_btn)

        self.content.append(btn_box)

        self.status_label = Gtk.Label()
        self.status_label.add_css_class("dim-label")
        self.content.append(self.status_label)

        self.add_nav_buttons()

    def should_show(self):
        return self.state.install_type == "repository"

    def on_enter(self):
        self._check_auth()

    def _on_login(self, _btn):
        github.login()
        self.status_label.set_text("Browser opened \u2014 complete sign-in, then click Check Status.")

    def _on_refresh(self, _btn):
        self._check_auth()

    def _check_auth(self):
        if github.is_authenticated():
            self.status.set_icon_name("emblem-ok-symbolic")
            self.status.set_title("Authenticated")
            self.status.set_description("You are signed in to GitHub.")
            self.status_label.set_text("")
            self.login_btn.set_sensitive(False)
            self.state.gh_authenticated = True
            github.configure_nix_token()
        else:
            self.state.gh_authenticated = False

    def validate(self):
        if not self.state.gh_authenticated:
            self.status_label.set_text("Please sign in to GitHub first.")
            return False
        return True
