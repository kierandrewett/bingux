import subprocess
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gtk
from pages.base_page import BasePage


LOCALES = [
    ("en_US.UTF-8", "English (United States)", "us"),
    ("en_GB.UTF-8", "English (United Kingdom)", "gb"),
    ("de_DE.UTF-8", "Deutsch (Deutschland)", "de"),
    ("fr_FR.UTF-8", "Fran\u00e7ais (France)", "fr"),
    ("es_ES.UTF-8", "Espa\u00f1ol (Espa\u00f1a)", "es"),
    ("it_IT.UTF-8", "Italiano (Italia)", "it"),
    ("pt_BR.UTF-8", "Portugu\u00eas (Brasil)", "br-abnt2"),
    ("pt_PT.UTF-8", "Portugu\u00eas (Portugal)", "pt"),
    ("nl_NL.UTF-8", "Nederlands (Nederland)", "nl"),
    ("pl_PL.UTF-8", "Polski (Polska)", "pl"),
    ("sv_SE.UTF-8", "Svenska (Sverige)", "se"),
    ("nb_NO.UTF-8", "Norsk Bokm\u00e5l (Norge)", "no"),
    ("da_DK.UTF-8", "Dansk (Danmark)", "dk"),
    ("fi_FI.UTF-8", "Suomi (Suomi)", "fi"),
    ("cs_CZ.UTF-8", "Ce\u0161tina (\u010cesko)", "cz"),
    ("ru_RU.UTF-8", "P\u0443\u0441\u0441\u043a\u0438\u0439 (P\u043e\u0441\u0441\u0438\u044f)", "ru"),
    ("ja_JP.UTF-8", "\u65e5\u672c\u8a9e (\u65e5\u672c)", "jp"),
    ("ko_KR.UTF-8", "\ud55c\uad6d\uc5b4 (\ud55c\uad6d)", "kr"),
    ("zh_CN.UTF-8", "\u4e2d\u6587 (\u4e2d\u56fd)", "cn"),
    ("tr_TR.UTF-8", "T\u00fcrk\u00e7e (T\u00fcrkiye)", "tr"),
    ("ar_SA.UTF-8", "\u0627\u0644\u0639\u0631\u0628\u064a\u0629", "ara"),
    ("hi_IN.UTF-8", "\u0939\u093f\u0928\u094d\u0926\u0940 (\u092d\u093e\u0930\u0924)", "in"),
]


class LocalePage(BasePage):
    def __init__(self, window):
        super().__init__(window, "Language", tag="locale", show_title=True)

        group = Adw.PreferencesGroup()
        group.set_title("Language & Keyboard")
        group.set_description("Choose your language. The keyboard layout will be set automatically.")

        self._locale_rows = []
        check_group = None

        for code, name, _keymap in LOCALES:
            row = Adw.ActionRow()
            row.set_title(name)
            row.set_subtitle(code)

            check = Gtk.CheckButton()
            if check_group is None:
                check_group = check
                check.set_active(True)
            else:
                check.set_group(check_group)

            check.connect("toggled", self._on_toggled, code, _keymap)
            row.add_prefix(check)
            row.set_activatable_widget(check)
            group.add(row)
            self._locale_rows.append((row, check, code))

        self.content.append(group)

        self.status_label = Gtk.Label()
        self.status_label.add_css_class("dim-label")
        self.status_label.set_halign(Gtk.Align.CENTER)
        self.content.append(self.status_label)

        self.add_nav_buttons(show_back=False)

        # Set initial state
        self.state.locale = LOCALES[0][0]
        self._current_keymap = LOCALES[0][2]

    def _on_toggled(self, check, code, keymap):
        if check.get_active():
            self.state.locale = code
            self._current_keymap = keymap
            self._apply_keymap(keymap)

    def _apply_keymap(self, keymap):
        """Apply keyboard layout immediately so passwords etc. work correctly."""
        try:
            # Try Wayland (sway/labwc)
            subprocess.run(
                ["swaymsg", "input", "*", "xkb_layout", keymap],
                capture_output=True, timeout=3,
            )
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass
        try:
            # Try X11 fallback
            subprocess.run(
                ["setxkbmap", keymap],
                capture_output=True, timeout=3,
            )
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass
        # Also set console keymap
        try:
            subprocess.run(
                ["sudo", "loadkeys", keymap],
                capture_output=True, timeout=3,
            )
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass

        self.status_label.set_text(f"Keyboard layout set to: {keymap}")

    def validate(self):
        return bool(self.state.locale)
