import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from state import InstallerState
from pages.welcome import WelcomePage
from pages.locale import LocalePage
from pages.network import NetworkPage
from pages.install_type import InstallTypePage
from pages.system_config import SystemConfigPage
from pages.repository import RepositoryPage
from pages.disk import DiskPage
from pages.partitioning import PartitioningPage
from pages.user_setup import UserSetupPage
from pages.summary import SummaryPage
from pages.install import InstallPage
from pages.complete import CompletePage
from pages.repair import RepairPage
from widgets.step_indicator import StepIndicator


STEP_LABELS = ["Setup", "Config", "Disk", "User", "Review", "Install"]


class BinguxInstallerWindow(Adw.ApplicationWindow):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.set_title("Install Bingux")
        self.set_default_size(900, 650)
        self.set_resizable(False)
        self.set_deletable(False)
        self.connect("close-request", lambda _: True)

        self.state = InstallerState()

        # Main layout
        main_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)

        # Step indicator bar (hidden on welcome/complete)
        self.step_bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.step_bar.set_margin_start(12)
        self.step_bar.set_margin_end(12)
        self.step_bar.set_margin_top(8)
        self.step_bar.set_margin_bottom(4)
        self.step_bar.set_visible(False)

        self.step_indicator = StepIndicator(len(STEP_LABELS), STEP_LABELS)
        self.step_indicator.set_hexpand(True)
        self.step_bar.append(self.step_indicator)

        main_box.append(self.step_bar)

        self.nav_view = Adw.NavigationView()
        self.nav_view.set_vexpand(True)
        self.nav_view.connect("notify::visible-page", self._on_page_changed)
        main_box.append(self.nav_view)

        self.set_content(main_box)

        self.pages = [
            WelcomePage(self),           # 0
            LocalePage(self),            # 1
            NetworkPage(self),           # 2 (skipped if online)
            InstallTypePage(self),       # 3
            SystemConfigPage(self),      # 3 (fresh only)
            RepositoryPage(self),        # 4 (repo only)
            DiskPage(self),              # 5
            PartitioningPage(self),      # 6 (manual only)
            UserSetupPage(self),         # 7
            SummaryPage(self),           # 8
            InstallPage(self),           # 9
            CompletePage(self),          # 10
        ]

        self._page_to_step = {
            0: -1,                  # Welcome (no steps)
            1: -1,                  # Locale (own title)
            2: 0, 3: 0,            # Network, InstallType -> Setup
            4: 1, 5: 1,            # SystemConfig, Repository -> Config
            6: 2, 7: 2,            # Disk, Partitioning -> Disk
            8: 3,                   # User
            9: 4,                   # Review
            10: 5, 11: -1,         # Install, Complete (no steps)
        }

        self.repair_page = RepairPage(self)
        self.nav_view.add(self.pages[0])

    def go_next(self):
        current = self.nav_view.get_visible_page()
        idx = self._page_index(current)

        for i in range(idx + 1, len(self.pages)):
            page = self.pages[i]
            if page.should_show():
                if hasattr(page, "on_enter"):
                    page.on_enter()
                self.nav_view.push(page)
                self.state.save()
                return

    def go_back(self):
        self.nav_view.pop()

    def _page_index(self, page):
        for i, p in enumerate(self.pages):
            if p is page:
                return i
        return -1

    def _on_page_changed(self, nav_view, _pspec):
        page = nav_view.get_visible_page()
        idx = self._page_index(page)
        step = self._page_to_step.get(idx, -1)

        if step >= 0:
            self.step_bar.set_visible(True)
            self.step_indicator.set_current(step)
        else:
            self.step_bar.set_visible(False)
