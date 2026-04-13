import gi

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

    def _clone_and_enumerate(self, url):
        self.error_label.set_text("")
        self.spinner.start()
        self.spinner.set_visible(True)
        self.next_btn.set_sensitive(False)

        def work():
            ok, err = github.clone_repo(url)
            if not ok:
                GLib.idle_add(self._on_clone_error, err)
                return
            hosts = github.enumerate_hosts()
            GLib.idle_add(self._on_hosts_found, url, hosts)

        threading.Thread(target=work, daemon=True).start()

    def _on_clone_error(self, err):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.next_btn.set_sensitive(True)
        self.error_label.set_text(f"Clone failed: {err}")

    def _on_hosts_found(self, url, hosts):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.next_btn.set_sensitive(True)
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
