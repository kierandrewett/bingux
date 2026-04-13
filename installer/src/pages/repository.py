import gi
import re
import subprocess

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk, GLib
from pages.base_page import BasePage
from backend import github
import threading


class RepositoryPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Repository", tag="repository")

        group = Adw.PreferencesGroup()
        group.set_title("NixOS Configuration")
        group.set_description("Enter the URL of your NixOS config repository.")

        self.url_row = Adw.EntryRow(title="Repository URL")
        self.url_row.set_text("https://github.com/")
        group.add(self.url_row)

        self.content.append(group)

        # Auth section — hidden until clone fails with auth error
        self.auth_group = Adw.PreferencesGroup()
        self.auth_group.set_title("Authentication Required")
        self.auth_group.set_description("This repository requires authentication.")
        self.auth_group.set_visible(False)

        self.token_row = Adw.PasswordEntryRow(title="Access Token")
        self.token_row.set_tooltip_text("Personal access token or OAuth token")
        self.auth_group.add(self.token_row)

        auth_btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        auth_btn_box.set_margin_top(8)

        self.gh_btn = Gtk.Button(label="Sign in via GitHub CLI")
        self.gh_btn.add_css_class("pill")
        self.gh_btn.connect("clicked", self._on_gh_login)
        auth_btn_box.append(self.gh_btn)

        self.gl_btn = Gtk.Button(label="Sign in via GitLab CLI")
        self.gl_btn.add_css_class("pill")
        self.gl_btn.connect("clicked", self._on_gl_login)
        auth_btn_box.append(self.gl_btn)

        auth_wrapper = Adw.ActionRow()
        auth_wrapper.set_child(auth_btn_box)
        self.auth_group.add(auth_wrapper)

        self.content.append(self.auth_group)

        # Host selection — shown after successful clone
        self.host_group = Adw.PreferencesGroup()
        self.host_group.set_title("Target Host")
        self.host_group.set_description("Select the NixOS configuration to install.")
        self.host_group.set_visible(False)

        self.host_combo = Adw.ComboRow(title="Host")
        self.host_group.add(self.host_combo)
        self.content.append(self.host_group)

        self.spinner = Gtk.Spinner()
        self.spinner.set_halign(Gtk.Align.CENTER)
        self.content.append(self.spinner)

        self.error_label = Gtk.Label()
        self.error_label.add_css_class("error")
        self.error_label.set_wrap(True)
        self.content.append(self.error_label)

        self.add_nav_buttons()

    def should_show(self):
        return self.state.install_type == "repository"

    def validate(self):
        url = self.url_row.get_text().strip()
        if not url:
            self.error_label.set_text("Please enter a repository URL.")
            return False

        if not self.state.repo_cloned or self.state.repo_url != url:
            self._clone_and_enumerate(url)
            return False

        if not self.state.selected_host:
            self.error_label.set_text("Please select a host.")
            return False

        return True

    def _get_clone_url(self, url):
        """Inject access token into URL if provided."""
        token = self.token_row.get_text().strip()
        if not token:
            # Try gh/glab CLI tokens
            token = github.get_token()
        if token and url.startswith("https://"):
            # https://oauth2:<token>@github.com/...
            return re.sub(r"^https://", f"https://oauth2:{token}@", url)
        return url

    def _clone_and_enumerate(self, url):
        self.error_label.set_text("")
        self.spinner.start()
        self.spinner.set_visible(True)
        self.next_btn.set_sensitive(False)

        clone_url = self._get_clone_url(url)

        def work():
            ok, err = github.clone_repo(clone_url)
            if not ok:
                is_auth = any(s in err.lower() for s in [
                    "authentication", "could not read username",
                    "403", "401", "permission denied", "not found",
                ])
                GLib.idle_add(self._on_clone_error, err, is_auth)
                return

            # Configure nix token for flake access
            github.configure_nix_token_from_url(clone_url)

            hosts = github.enumerate_hosts()
            GLib.idle_add(self._on_hosts_found, url, hosts)

        threading.Thread(target=work, daemon=True).start()

    def _on_clone_error(self, err, is_auth):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.next_btn.set_sensitive(True)

        if is_auth:
            self.auth_group.set_visible(True)
            self.error_label.set_text("Authentication failed. Sign in or provide an access token, then try again.")
        else:
            self.error_label.set_text(f"Clone failed: {err}")

    def _on_hosts_found(self, url, hosts):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.next_btn.set_sensitive(True)
        self.auth_group.set_visible(False)
        self.state.repo_url = url
        self.state.repo_cloned = True

        if not hosts:
            self.error_label.set_text("No nixosConfigurations found in the repository.")
            return

        self.state.available_hosts = hosts

        model = Gtk.StringList()
        for h in hosts:
            model.append(h)
        self.host_combo.set_model(model)
        self.host_combo.connect("notify::selected", self._on_host_selected)
        self.host_group.set_visible(True)

        if len(hosts) == 1:
            self.host_combo.set_selected(0)
            self.state.selected_host = hosts[0]

    def _on_host_selected(self, combo, _pspec):
        idx = combo.get_selected()
        if idx < len(self.state.available_hosts):
            self.state.selected_host = self.state.available_hosts[idx]

    def _on_gh_login(self, _btn):
        subprocess.Popen([
            "foot", "--", "bash", "-c",
            "gh auth login -p https -w; echo; echo 'Done — close this window and try again.'; read",
        ])

    def _on_gl_login(self, _btn):
        subprocess.Popen([
            "foot", "--", "bash", "-c",
            "echo 'Paste a GitLab personal access token:'; read -s TOKEN; "
            "git config --global credential.helper store; "
            "echo \"https://oauth2:$TOKEN@gitlab.com\" >> ~/.git-credentials; "
            "echo; echo 'Done — close this window and try again.'; read",
        ])
