import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from state import InstallerState
from pages.welcome import WelcomePage
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


# Step labels for the indicator (simplified view of the flow)
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

        # Main layout: step indicator on top, nav view below
        main_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)

        self.step_indicator = StepIndicator(len(STEP_LABELS), STEP_LABELS)
        self.step_indicator.set_margin_top(8)
        main_box.append(self.step_indicator)

        self.nav_view = Adw.NavigationView()
        self.nav_view.set_vexpand(True)
        self.nav_view.connect("notify::visible-page", self._on_page_changed)
        main_box.append(self.nav_view)

        self.set_content(main_box)

        self.pages = [
            WelcomePage(self),           # step 0 (Setup)
            InstallTypePage(self),       # step 0
            SystemConfigPage(self),      # step 1 (Config)
            RepositoryPage(self),        # step 1
            DiskPage(self),              # step 2 (Disk)
            PartitioningPage(self),      # step 2
            UserSetupPage(self),         # step 3 (User)
            SummaryPage(self),           # step 4 (Review)
            InstallPage(self),           # step 5 (Install)
            CompletePage(self),          # step 5
        ]

        # Map each page index to a step indicator index
        self._page_to_step = {
            0: 0, 1: 0,       # Welcome, InstallType -> Setup
            2: 1, 3: 1,       # SystemConfig, Repository -> Config
            4: 2, 5: 2,       # Disk, Partitioning -> Disk
            6: 3,              # User -> User
            7: 4,              # Summary -> Review
            8: 5, 9: 5,       # Install, Complete -> Install
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
        step = self._page_to_step.get(idx, 0)
        self.step_indicator.set_current(step)

        # Hide step indicator on complete page
        self.step_indicator.set_visible(idx < 9)
