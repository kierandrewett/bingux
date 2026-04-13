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


class BinguxInstallerWindow(Adw.ApplicationWindow):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.set_title("Install Bingux")
        self.set_default_size(800, 600)
        self.set_resizable(False)
        self.set_deletable(False)
        self.connect("close-request", lambda _: True)  # Block Alt+F4

        self.state = InstallerState()

        self.nav_view = Adw.NavigationView()
        self.set_content(self.nav_view)

        self.pages = [
            WelcomePage(self),
            InstallTypePage(self),
            SystemConfigPage(self),      # Fresh install only
            RepositoryPage(self),        # Repository only
            DiskPage(self),              # Disk + fs + encryption (wipe mode)
            PartitioningPage(self),      # Manual mode only
            UserSetupPage(self),
            SummaryPage(self),
            InstallPage(self),
            CompletePage(self),
        ]

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
