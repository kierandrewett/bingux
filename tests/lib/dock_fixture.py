"""Build a Quickshell fixture directory around the production Dock.

The fixture copies the shell's components, replaces the compositor's toplevel
manager with the test's `testManager` object and adds inspection aliases to the
Dock. Grouping, timers, the Repeater, layout and animations stay production
code. `config` holds an optional bingux.toml so a test starts from saved state.

Users: tests/dock-state.py, tests/dock-folders.py.
"""

from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[2]


def prepare(fixture, test_qml, shell_dir=None, config_toml=None):
    """Populate `fixture` and return the environment overrides the shell needs."""
    fixture = Path(fixture)
    source = Path(shell_dir) if shell_dir else ROOT / "shell/bingux"
    for component in source.iterdir():
        if component.suffix in (".qml", ".js") or component.name == "qmldir":
            shutil.copy2(component, fixture)
    dock_file = fixture / "Dock.qml"
    dock = dock_file.read_text().replace("ToplevelManager", "root.testManager")
    dock = dock.replace(
        "required property var settings",
        """required property var settings
    required property var testManager
    property alias testItems: dockItems
    property alias testSurface: dockSurface
    property alias testTooltip: dockTooltip
    property alias testLaunchEffect: dockLaunchEffect
    property alias testLaunchOverlay: launchOverlay""",
    )
    dock = dock.replace("model: root.testManager.toplevels", "model: root.testManager.toplevels.values")
    dock = dock.replace(
        "id: dockButton",
        """id: dockButton
                    property alias testMenu: dockButton.appMenu
                    property alias testMouse: dockMouse
                    property alias testIndicators: windowIndicators""",
    )
    dock_file.write_text(dock)
    menu_file = fixture / "DockAppMenu.qml"
    menu_file.write_text(menu_file.read_text().replace("ToplevelManager", "dock.testManager"))
    indicators_file = fixture / "DockWindowIndicators.qml"
    indicators_file.write_text(
        indicators_file.read_text().replace("id: root", "id: root\n    property alias testView: strip", 1)
    )
    shutil.copy2(ROOT / "tests/launch-feedback-mock.qml", fixture / "LaunchFeedback.qml")
    shutil.copy2(test_qml, fixture / "shell.qml")
    environment = {"XDG_CONFIG_HOME": str(fixture / "config")}
    (fixture / "config/bingux").mkdir(parents=True, exist_ok=True)
    if config_toml is not None:
        (fixture / "config/bingux/bingux.toml").write_text(config_toml)
    return environment
