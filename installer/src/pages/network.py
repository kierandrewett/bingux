import gi
import subprocess
import threading

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk, GLib
from pages.base_page import BasePage


def _is_online():
    try:
        r = subprocess.run(
            ["ping", "-c", "1", "-W", "3", "1.1.1.1"],
            capture_output=True, timeout=5,
        )
        return r.returncode == 0
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False


def _get_wifi_networks():
    """Scan for WiFi networks via nmcli."""
    try:
        subprocess.run(["nmcli", "dev", "wifi", "rescan"], capture_output=True, timeout=10)
        r = subprocess.run(
            ["nmcli", "-t", "-f", "SSID,SIGNAL,SECURITY", "dev", "wifi", "list"],
            capture_output=True, text=True, check=True, timeout=10,
        )
        networks = []
        seen = set()
        for line in r.stdout.strip().split("\n"):
            parts = line.split(":")
            if len(parts) >= 3 and parts[0] and parts[0] not in seen:
                seen.add(parts[0])
                networks.append({
                    "ssid": parts[0],
                    "signal": parts[1],
                    "security": parts[2] if parts[2] else "Open",
                })
        networks.sort(key=lambda n: int(n["signal"] or "0"), reverse=True)
        return networks
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, FileNotFoundError):
        return []


def _has_wifi():
    try:
        r = subprocess.run(
            ["nmcli", "-t", "-f", "TYPE,STATE", "dev"],
            capture_output=True, text=True, timeout=5,
        )
        return "wifi" in r.stdout
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, FileNotFoundError):
        return False


def _connect_wifi(ssid, password=""):
    try:
        cmd = ["nmcli", "dev", "wifi", "connect", ssid]
        if password:
            cmd += ["password", password]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        return r.returncode == 0, r.stderr or r.stdout
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False, "Connection timed out"


class NetworkPage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Network", tag="network")

        self.status_icon = Gtk.Image()
        self.status_icon.set_pixel_size(48)
        self.status_icon.set_halign(Gtk.Align.CENTER)
        self.content.append(self.status_icon)

        self.status_label = Gtk.Label()
        self.status_label.add_css_class("title-3")
        self.status_label.set_halign(Gtk.Align.CENTER)
        self.content.append(self.status_label)

        self.status_detail = Gtk.Label()
        self.status_detail.add_css_class("dim-label")
        self.status_detail.set_halign(Gtk.Align.CENTER)
        self.content.append(self.status_detail)

        # WiFi section (hidden until needed)
        self.wifi_group = Adw.PreferencesGroup()
        self.wifi_group.set_title("WiFi Networks")
        self.wifi_group.set_visible(False)
        self.content.append(self.wifi_group)

        # Password dialog row (hidden until network selected)
        self.pass_group = Adw.PreferencesGroup()
        self.pass_group.set_visible(False)

        self.ssid_label = Gtk.Label()
        self.ssid_label.add_css_class("heading")
        self.pass_group.add(Adw.ActionRow(title="Selected Network"))

        self.pass_row = Adw.PasswordEntryRow(title="Password")
        self.pass_group.add(self.pass_row)

        connect_btn = Gtk.Button(label="Connect")
        connect_btn.add_css_class("pill")
        connect_btn.add_css_class("suggested-action")
        connect_btn.set_halign(Gtk.Align.CENTER)
        connect_btn.set_margin_top(8)
        connect_btn.connect("clicked", self._on_connect)
        self.pass_group.add(Adw.ActionRow(child=connect_btn))

        self.content.append(self.pass_group)

        # Ethernet prompt (hidden until needed)
        self.ethernet_label = Gtk.Label()
        self.ethernet_label.set_markup(
            "No WiFi adapter found. Please connect an <b>Ethernet cable</b> and try again."
        )
        self.ethernet_label.set_wrap(True)
        self.ethernet_label.set_halign(Gtk.Align.CENTER)
        self.ethernet_label.set_visible(False)
        self.content.append(self.ethernet_label)

        self.error_label = Gtk.Label()
        self.error_label.add_css_class("error")
        self.error_label.set_wrap(True)
        self.content.append(self.error_label)

        self.spinner = Gtk.Spinner()
        self.spinner.set_halign(Gtk.Align.CENTER)
        self.content.append(self.spinner)

        # Retry + skip buttons
        btn_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        btn_box.set_halign(Gtk.Align.CENTER)
        btn_box.set_margin_top(8)

        retry_btn = Gtk.Button(label="Retry")
        retry_btn.add_css_class("pill")
        retry_btn.connect("clicked", lambda _: self.on_enter())
        btn_box.append(retry_btn)

        self.content.append(btn_box)

        self.add_nav_buttons()
        self._selected_ssid = ""
        self._wifi_rows = []

    def should_show(self):
        # Skip if already online
        return not _is_online()

    def on_enter(self):
        self.error_label.set_text("")
        self.spinner.start()
        self.spinner.set_visible(True)
        self.wifi_group.set_visible(False)
        self.pass_group.set_visible(False)
        self.ethernet_label.set_visible(False)

        self.status_icon.set_from_icon_name("network-offline-symbolic")
        self.status_label.set_text("No internet connection")
        self.status_detail.set_text("Checking network...")

        threading.Thread(target=self._check_network, daemon=True).start()

    def _check_network(self):
        online = _is_online()
        if online:
            GLib.idle_add(self._show_connected)
            return

        has_wifi_dev = _has_wifi()
        if has_wifi_dev:
            networks = _get_wifi_networks()
            GLib.idle_add(self._show_wifi, networks)
        else:
            GLib.idle_add(self._show_ethernet)

    def _show_connected(self):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.status_icon.set_from_icon_name("network-wireless-symbolic")
        self.status_label.set_text("Connected")
        self.status_detail.set_text("You are connected to the internet.")

    def _show_wifi(self, networks):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.status_detail.set_text("Select a WiFi network to connect.")

        # Clear old rows
        for row in self._wifi_rows:
            self.wifi_group.remove(row)
        self._wifi_rows.clear()

        if not networks:
            self.status_detail.set_text("No WiFi networks found. Try again or connect Ethernet.")
            self.wifi_group.set_visible(False)
            return

        self.wifi_group.set_visible(True)
        for net in networks[:10]:
            row = Adw.ActionRow()
            row.set_title(net["ssid"])
            row.set_subtitle(f"Signal: {net['signal']}%  \u2022  {net['security']}")
            row.set_activatable(True)
            row.connect("activated", lambda _, s=net["ssid"], sec=net["security"]: self._on_network_selected(s, sec))

            icon_name = "network-wireless-signal-excellent-symbolic"
            sig = int(net["signal"] or "0")
            if sig < 30:
                icon_name = "network-wireless-signal-weak-symbolic"
            elif sig < 60:
                icon_name = "network-wireless-signal-ok-symbolic"
            elif sig < 80:
                icon_name = "network-wireless-signal-good-symbolic"

            icon = Gtk.Image.new_from_icon_name(icon_name)
            icon.set_pixel_size(20)
            row.add_prefix(icon)

            arrow = Gtk.Image.new_from_icon_name("go-next-symbolic")
            row.add_suffix(arrow)

            self.wifi_group.add(row)
            self._wifi_rows.append(row)

    def _show_ethernet(self):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.status_detail.set_text("")
        self.ethernet_label.set_visible(True)

    def _on_network_selected(self, ssid, security):
        self._selected_ssid = ssid
        if security == "Open" or not security:
            # Open network — connect directly
            self._do_connect(ssid, "")
        else:
            # Show password entry
            self.pass_group.set_visible(True)
            self.pass_row.set_text("")
            self.pass_row.grab_focus()

    def _on_connect(self, _btn):
        password = self.pass_row.get_text()
        self._do_connect(self._selected_ssid, password)

    def _do_connect(self, ssid, password):
        self.error_label.set_text("")
        self.spinner.start()
        self.spinner.set_visible(True)
        self.status_detail.set_text(f"Connecting to {ssid}...")

        def work():
            ok, err = _connect_wifi(ssid, password)
            if ok:
                GLib.idle_add(self._show_connected)
            else:
                GLib.idle_add(self._on_connect_fail, err)

        threading.Thread(target=work, daemon=True).start()

    def _on_connect_fail(self, err):
        self.spinner.stop()
        self.spinner.set_visible(False)
        self.status_detail.set_text("Connection failed.")
        self.error_label.set_text(err.strip())

    def validate(self):
        if _is_online():
            return True
        self.error_label.set_text("An internet connection is required to continue.")
        return False
