import copy
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
BIN = os.environ.get("BINGUX_CONFIG_BIN", str(ROOT / "build/cargo/release/bingux-config"))

CONTROL_GROUPS = {
    "control-centre": [
        "controls-header",
        "controls-audio",
        "control-divider",
        "controls-tiles",
        "control-media",
        "control-customise",
    ],
    "controls-header": [
        "control-account",
        "control-header-space",
        "control-battery",
        "control-settings",
        "control-session",
        "control-lock",
    ],
    "controls-audio": ["control-volume", "control-microphone"],
}

PORTABLE_CONTROLS = {
    "control-account",
    "control-settings",
    "control-session",
    "control-lock",
    "control-volume",
    "control-microphone",
    "control-battery",
    "control-media",
    "controls-header",
    "controls-audio",
    "controls-tiles",
    "control-divider",
    "control-header-space",
    "control-customise",
}


def cli(*args, stdin=None):
    return subprocess.run(
        [BIN, *args],
        input=stdin if stdin is not None else "",
        capture_output=True,
        text=True,
        check=False,
    )


def run(*args, stdin=None):
    return json.loads(cli(*args, stdin=stdin).stdout)


def config_path():
    return Path(os.environ["XDG_CONFIG_HOME"]) / "bingux" / "bingux.toml"


def read():
    reply = run("dump")
    if "error" in reply:
        raise ValueError(reply["error"])
    return reply["data"]


def write(patch_data):
    reply = run("save", stdin=json.dumps(patch_data))
    if "error" in reply:
        raise ValueError(reply["error"])
    return reply["data"]


def import_layout(snapshot):
    reply = run("import-layout", stdin=json.dumps(snapshot))
    if "error" in reply:
        raise ValueError(reply["error"])
    return reply["data"]


def native_control_layout():
    return {"version": 1, "groups": copy.deepcopy(CONTROL_GROUPS)}


def toml_value(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{json.dumps(key)} = {toml_value(item)}" for key, item in value.items()) + " }"
    raise TypeError(f"cannot encode {value!r} as TOML")


def write_desktop_toml(desktop):
    """Write a raw [desktop] table (snake_case keys) straight into bingux.toml, bypassing the binary."""
    text = "[desktop]\n" + "".join(f"{key} = {toml_value(value)}\n" for key, value in desktop.items())
    config_path().parent.mkdir(parents=True, exist_ok=True)
    config_path().write_text(text)
    return text


class BinguxConfigTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        env = patch.dict(os.environ, {"XDG_CONFIG_HOME": directory.name})
        env.start()
        self.addCleanup(env.stop)

    def test_roundtrip_and_partial_updates(self):
        data = write({"desktop": {"dockAlignment": "left", "dockSize": 40}})
        self.assertEqual(read(), data)
        write({"previews": {"enabled": False}})
        self.assertEqual(read()["desktop"]["dockSize"], 40)

    def test_read_exposes_canonical_defaults_without_resetting_saved_settings(self):
        defaults = run("dump")["desktopDefaults"]
        write({"desktop": {"dockSize": 40, "dockAlignment": "left"}})
        before = config_path().read_bytes()
        result = run("dump")
        self.assertEqual(result["desktopDefaults"], defaults)
        self.assertEqual(result["data"]["desktop"]["dockSize"], 40)
        self.assertEqual(config_path().read_bytes(), before)

    def test_search_engine_validation_preserves_file_on_failure(self):
        data = write({})
        before = config_path().read_bytes()
        for url in [
            "javascript:{query}",
            "https://example.com/",
            "https://user:pass@example.com/{query}",
            "https://{query}.com/",
        ]:
            invalid = copy.deepcopy(data)
            invalid["search"]["engines"][0]["url"] = url
            with self.assertRaises(ValueError):
                write(invalid)
            self.assertEqual(config_path().read_bytes(), before)
        data["search"]["engines"].append(
            {
                "id": "example",
                "name": "Example",
                "shortcut": "ex",
                "url": "https://example.com/?q={query}",
                "enabled": True,
            }
        )
        data["search"]["defaultEngine"] = "example"
        self.assertEqual(write(data)["search"]["defaultEngine"], "example")

    def test_rejects_duplicate_widgets_and_incompatible_destinations(self):
        layout = {"top-left": ["search"], "top-center": ["clock"], "top-right": [], "dock": [], "sidebar": ["notes"]}
        write({"desktop": {"layout": layout}})
        for key, items in [
            ("dock", ["search"]),
            ("dock", ["notes"]),
            ("sidebar", ["unknown"]),
            ("top-left", ["search", "search"]),
        ]:
            invalid = copy.deepcopy(layout)
            invalid[key] = items
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": invalid}})

    def test_sidebar_panels_can_leave_an_empty_sidebar(self):
        panels = ["terminal", "notes", "monitor", "calendar", "media", "tasks"]
        for zone in ["top-left", "top-center", "top-right", "dock"]:
            layout = {key: [] for key in ["top-left", "top-center", "top-right", "dock", "sidebar"]}
            layout[zone] = panels
            write({"desktop": {"layout": layout}})
            self.assertEqual(read()["desktop"]["layout"], layout)
        layout = {key: [] for key in ["top-left", "top-center", "top-right", "dock", "sidebar"]}
        controls = native_control_layout()
        controls["groups"]["control-centre"].extend(panels)
        write({"desktop": {"layout": layout, "controlLayout": controls}})
        self.assertEqual(read()["desktop"]["controlLayout"], controls)
        before = config_path().read_bytes()
        for panel in panels:
            duplicate = dict(layout, dock=[panel])
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": duplicate, "controlLayout": controls}})
            self.assertEqual(config_path().read_bytes(), before)

    def test_rejects_invalid_behaviour_and_locations(self):
        for data in [
            {"desktop": {"dockSize": 999}},
            {"desktop": {"dockClick": "unknown"}},
            {"search": {"fileRoots": ["relative/path"]}},
        ]:
            with self.assertRaises(ValueError):
                write(data)

    def test_control_centre_order_roundtrip_and_validation(self):
        order = ["bluetooth", "network", "power", "dnd"]
        write({"desktop": {"controlOrder": order}})
        self.assertEqual(read()["desktop"]["controlOrder"], order)
        before = config_path().read_bytes()
        for invalid in [["network", "network"], ["unknown"], [None], "network"]:
            with self.assertRaises(ValueError):
                write({"desktop": {"controlOrder": invalid}})
            self.assertEqual(config_path().read_bytes(), before)

    def test_controls_move_between_containers_without_duplicate_placements(self):
        layout = {
            "top-left": ["search"],
            "top-center": ["clock"],
            "top-right": ["controls"],
            "dock": ["control-network"],
            "sidebar": ["notes"],
        }
        write({"desktop": {"layout": layout, "controlOrder": ["bluetooth", "dnd"]}})
        self.assertEqual(read()["desktop"]["layout"], layout)
        before = config_path().read_bytes()
        with self.assertRaises(ValueError):
            write({"desktop": {"controlOrder": ["network", "bluetooth"]}})
        with self.assertRaises(ValueError):
            write({"desktop": {"controlOrder": None}})
        for item in ["control-network", "control-unknown"]:
            duplicate = copy.deepcopy(layout)
            duplicate["top-right"].append(item)
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": duplicate}})
        self.assertEqual(config_path().read_bytes(), before)

    def test_status_and_decoration_control_centre_placement(self):
        layout = {"top-left": [], "top-center": [], "top-right": [], "dock": [], "sidebar": ["terminal"]}
        for item in ["search", "clock", "controls", "label:1", "icon:3"]:
            groups = native_control_layout()
            groups["groups"]["control-centre"].insert(1, item)
            write({"desktop": {"layout": layout, "controlLayout": groups}})
            self.assertEqual(read()["desktop"]["controlLayout"], groups)
            before = config_path().read_bytes()
            duplicate = copy.deepcopy(layout)
            duplicate["top-left"].append(item)
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": duplicate}})
            self.assertEqual(config_path().read_bytes(), before)
        for item in ["unknown", "label", "icon:0", "label:10000", "terminal"]:
            groups = native_control_layout()
            groups["groups"]["control-centre"].append(item)
            with self.assertRaises(ValueError):
                write({"desktop": {"controlLayout": groups}})
        groups = native_control_layout()
        groups["groups"]["control-centre"] = ["label:" + str(index) for index in range(1, 258)]
        with self.assertRaises(ValueError):
            write({"desktop": {"controlLayout": groups}})

    def test_fixed_space_width_validation(self):
        write({"desktop": {"widgetOptions": {"spacer:1": {"width": 48}, "control-header-space": {"width": 64}}}})
        self.assertEqual(read()["desktop"]["widgetOptions"]["spacer:1"]["width"], 48)
        self.assertEqual(read()["desktop"]["widgetOptions"]["control-header-space"]["width"], 64)
        before = config_path().read_bytes()
        for key, width in [
            ("spacer:1", 0),
            ("spacer:1", 161),
            ("spacer:1", True),
            ("spring:1", 40),
            ("search", 40),
            ("control-header-space", 0),
            ("control-header-space", 161),
            ("control-header-space", True),
        ]:
            with self.assertRaises(ValueError):
                write({"desktop": {"widgetOptions": {key: {"width": width}}}})
            self.assertEqual(config_path().read_bytes(), before)

    def test_spacing_instances_roundtrip_and_validate(self):
        layout = {
            "top-left": ["search", "spring:1", "spacer:1"],
            "top-center": ["clock"],
            "top-right": ["spring:2", "controls"],
            "dock": [],
            "sidebar": ["notes"],
        }
        write({"desktop": {"layout": layout}})
        self.assertEqual(read()["desktop"]["layout"], layout)
        before = config_path().read_bytes()
        for zone, item in [
            ("dock", "spring:3"),
            ("sidebar", "spacer:2"),
            ("top-left", "spring:2"),
            ("top-left", "spring"),
            ("top-left", "spring:0"),
            ("top-left", "spacer:invalid"),
        ]:
            invalid = copy.deepcopy(layout)
            invalid[zone].append(item)
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": invalid}})
        self.assertEqual(config_path().read_bytes(), before)

    def test_label_and_icon_instances_persist_with_appearance(self):
        layout = {
            "top-left": ["search", "label:1"],
            "top-center": ["clock"],
            "top-right": ["controls"],
            "dock": ["icon:1", "label:2"],
            "sidebar": ["notes"],
        }
        options = {"label:1": {"label": "Work"}, "icon:1": {"icon": "starred-symbolic"}}
        write({"desktop": {"layout": layout, "widgetOptions": options}})
        self.assertEqual(read()["desktop"]["layout"], layout)
        self.assertEqual(read()["desktop"]["widgetOptions"], options)
        before = config_path().read_bytes()
        for zone, item in [
            ("sidebar", "label:0"),
            ("dock", "icon:0"),
            ("top-left", "label"),
            ("dock", "label:1"),
            ("dock", "icon:10000"),
        ]:
            invalid = copy.deepcopy(layout)
            invalid[zone].append(item)
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": invalid}})
            self.assertEqual(config_path().read_bytes(), before)

    def test_sidebar_status_and_decorations_allow_no_native_panel(self):
        layout = {
            "top-left": [],
            "top-center": [],
            "top-right": [],
            "dock": [],
            "sidebar": ["clock", "label:1", "notes", "icon:1", "search"],
        }
        write({"desktop": {"layout": layout}})
        self.assertEqual(read()["desktop"]["layout"], layout)
        write({"desktop": {"layout": dict(layout, sidebar=["clock", "label:1"])}})
        before = config_path().read_bytes()
        for invalid in [dict(layout, dock=["clock"]), dict(layout, sidebar=layout["sidebar"] + ["spring:1"])]:
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": invalid}})
            self.assertEqual(config_path().read_bytes(), before)

    def test_native_controls_persist_in_sidebar_without_duplicate_members(self):
        layout = {
            "top-left": [],
            "top-center": [],
            "top-right": [],
            "dock": [],
            "sidebar": ["controls-audio", "controls-header", "control-network", "notes"],
        }
        groups = native_control_layout()
        groups["groups"]["control-centre"].remove("controls-audio")
        groups["groups"]["control-centre"].remove("controls-header")
        write({"desktop": {"layout": layout, "controlLayout": groups, "controlOrder": ["bluetooth"]}})
        self.assertEqual(read()["desktop"]["layout"], layout)
        before = config_path().read_bytes()
        invalid = copy.deepcopy(groups)
        invalid["groups"]["control-centre"].append("controls-audio")
        with self.assertRaises(ValueError):
            write({"desktop": {"controlLayout": invalid}})
        self.assertEqual(config_path().read_bytes(), before)

    def test_runtime_import_is_exact_and_happens_once(self):
        snapshot = {
            "version": 1,
            "controlCentreReady": True,
            "layout": {
                "top-left": ["search"],
                "top-center": ["clock"],
                "top-right": ["keyboard", "metrics", "controls"],
                "dock": [],
                "sidebar": ["notes", "terminal"],
            },
            "sidebar": {"edge": "left"},
            "dock": {"pinnedApps": ["discord-canary", "org.telegram"], "order": ["TelegramDesktop", "discord"]},
            "controlCentre": {"vpn": True, "dnd": True, "nightLight": True, "power": False, "awake": False},
        }
        write({"desktop": {"dockSize": 48}})
        imported = import_layout(snapshot)
        self.assertEqual(imported["desktop"]["layout"], snapshot["layout"])
        self.assertEqual(imported["desktop"]["dockApps"], snapshot["dock"])
        self.assertEqual(imported["desktop"]["controlCentre"], snapshot["controlCentre"])
        self.assertEqual(imported["desktop"]["sidebarEdge"], "left")
        self.assertEqual(imported["desktop"]["dockSize"], 48)
        self.assertEqual(imported["desktop"]["layoutVersion"], 1)
        backup = config_path().parent / "layout-before-import.json"
        original_backup = backup.read_bytes()
        write({"desktop": {"dockSize": 64, "sidebarEdge": "right"}})
        self.assertEqual(import_layout({})["desktop"]["dockSize"], 64)
        self.assertEqual(read()["desktop"]["sidebarEdge"], "right")
        self.assertEqual(backup.read_bytes(), original_backup)
        with self.assertRaises(ValueError):
            write({"desktop": {"layoutVersion": 0}})

    def test_control_group_import_extends_existing_layout_without_resetting_it(self):
        original = write(
            {
                "desktop": {
                    "layoutVersion": 1,
                    "dockSize": 72,
                    "sidebarEdge": "left",
                    "controlOrder": ["bluetooth", "network", "power"],
                    "dockApps": {"pinnedApps": ["example.app"], "order": ["example.app"]},
                }
            }
        )
        imported = import_layout({"version": 1, "controlCentreReady": True, "controlLayout": native_control_layout()})
        expected = copy.deepcopy(original)
        expected["desktop"]["controlLayout"] = native_control_layout()
        self.assertEqual(imported, expected)
        with self.assertRaises(ValueError):
            write({"desktop": {"controlLayout": None}})
        backup = config_path().parent / "control-layout-before-import.json"
        self.assertTrue(backup.is_file())
        previous = backup.read_bytes()
        custom = native_control_layout()
        custom["groups"]["control-centre"].reverse()
        write({"desktop": {"controlLayout": custom}})
        self.assertEqual(import_layout({})["desktop"]["controlLayout"], custom)
        self.assertEqual(backup.read_bytes(), previous)

    def test_moved_control_actions_keep_one_saved_owner(self):
        layout = {"top-left": [], "top-center": [], "top-right": [], "dock": [], "sidebar": ["notes"]}
        groups = native_control_layout()
        write({"desktop": {"layout": layout, "controlLayout": groups}})
        for action in PORTABLE_CONTROLS:
            moved = copy.deepcopy(layout)
            moved["dock"] = [action]
            before = config_path().read_bytes()
            with self.assertRaises(ValueError):
                write({"desktop": {"layout": moved}})
            self.assertEqual(config_path().read_bytes(), before)
            owned = copy.deepcopy(groups)
            group = next(name for name, ids in CONTROL_GROUPS.items() if action in ids)
            owned["groups"][group].remove(action)
            write({"desktop": {"layout": moved, "controlLayout": owned}})
            self.assertEqual(read()["desktop"]["layout"]["dock"], [action])
            valid = config_path().read_bytes()
            write_desktop_toml({"layout": moved, "control_layout": groups})
            with self.assertRaises(ValueError):
                read()
            config_path().write_bytes(valid)
            write({"desktop": {"layout": layout, "controlLayout": groups}})

    def test_control_group_validation_preserves_the_last_saved_layout(self):
        write({"desktop": {"controlLayout": native_control_layout()}})
        before = config_path().read_bytes()
        invalid = []
        for version in [2, True, "1"]:
            value = native_control_layout()
            value["version"] = version
            invalid.append(value)
        value = native_control_layout()
        value["groups"]["controls-header"].append("control-account")
        invalid.append(value)
        value = native_control_layout()
        value["groups"]["controls-audio"] = ["control-settings"]
        invalid.append(value)
        value = native_control_layout()
        value["groups"]["missing"] = []
        invalid.append(value)
        for value in invalid:
            with self.assertRaises(ValueError):
                write({"desktop": {"controlLayout": value}})
            self.assertEqual(config_path().read_bytes(), before)

    def test_read_rejects_invalid_desktop_data_without_rewriting_it(self):
        base = {
            "layout": {
                "top-left": ["search"],
                "top-center": ["clock"],
                "top-right": [],
                "dock": [],
                "sidebar": ["notes"],
            }
        }
        mutations = [
            lambda d: d["layout"].pop("dock"),
            lambda d: d["layout"].update(dock=["clock"]),
            lambda d: d["layout"].update(dock=["unknown"]),
            lambda d: d.update(dock_apps={"pinned_apps": [1], "order": []}),
            lambda d: d.update(containers={"dock": {"display": "invalid"}}),
            lambda d: d.update(widget_options={"clock": {"label": 42}}),
            lambda d: d.update(dock_size="large"),
        ]
        for mutate in mutations:
            value = copy.deepcopy(base)
            mutate(value)
            raw = write_desktop_toml(value)
            with self.subTest(desktop=value):
                with self.assertRaises(ValueError):
                    read()
                self.assertEqual(config_path().read_text(), raw)

    @unittest.skipIf(hasattr(os, "geteuid") and os.geteuid() == 0, "root bypasses directory permissions")
    def test_failed_atomic_replace_preserves_settings_and_removes_temporary_file(self):
        write({"desktop": {"dockSize": 40}})
        before = config_path().read_bytes()
        directory = config_path().parent
        directory.chmod(0o500)
        try:
            reply = run("save", stdin=json.dumps({"desktop": {"dockSize": 64}}))
        finally:
            directory.chmod(0o700)
        self.assertIn("error", reply)
        self.assertEqual(config_path().read_bytes(), before)
        leftovers = [
            p.name for p in directory.iterdir() if p.name.startswith(".bingux.toml.") and p.name.endswith(".tmp")
        ]
        self.assertEqual(leftovers, [])

    def test_unknown_version_and_failed_import_do_not_replace_settings(self):
        write({})
        before = config_path().read_bytes()
        with self.assertRaises(ValueError):
            import_layout({"version": 1, "controlCentreReady": False})
        for desktop in [
            {"layoutVersion": 2},
            {"layoutVersion": True},
            {"dockApps": {"order": ["same", "same"], "pinnedApps": []}},
            {"containers": {"dock": {"display": "invalid"}}},
            {"widgetOptions": {"clock": {"display": "invalid"}}},
        ]:
            with self.assertRaises(ValueError):
                write({"desktop": desktop})
            self.assertEqual(config_path().read_bytes(), before)

    def test_migrates_legacy_settings_json_on_first_command(self):
        directory = config_path().parent
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "settings.json").write_text(json.dumps({"desktop": {"dockSize": 60}}))
        reply = run("dump")
        self.assertNotIn("error", reply)
        self.assertEqual(reply["data"]["desktop"]["dockSize"], 60)
        self.assertIn("dock_size = 60", config_path().read_text())
        self.assertTrue((directory / "settings.json.migrated").exists())

    def test_missing_ai_harness_blocks_writes_but_not_reads(self):
        # The harness being installed is a property of the machine, not the
        # file: a saved choice must keep loading after the program is removed.
        config_path().parent.mkdir(parents=True, exist_ok=True)
        config_path().write_text('[search.ai]\nharness = "pi"\nexecutable = "/nonexistent/pi"\n')
        reply = run("dump")
        self.assertNotIn("error", reply, reply)
        self.assertEqual(reply["data"]["search"]["ai"]["harness"], "pi")
        reply = run("save", stdin=json.dumps({"search": {"ai": {"harness": "pi", "executable": "/nonexistent/pi"}}}))
        self.assertIn("not installed", reply["error"])
        # Unrelated writes still work while the harness is missing.
        reply = run("save", stdin=json.dumps({"desktop": {"dockSize": 64}}))
        self.assertNotIn("error", reply, reply)

    def test_save_preserves_comments_in_existing_toml(self):
        config_path().parent.mkdir(parents=True, exist_ok=True)
        config_path().write_text("[desktop]\n# keep me\ndock_size = 60 # inline\n")
        reply = run("save", stdin=json.dumps({"desktop": {"dockSize": 64}}))
        self.assertNotIn("error", reply)
        text = config_path().read_text()
        for needle in ["# keep me", "# inline", "dock_size = 64"]:
            self.assertIn(needle, text)

    def test_set_get_check_and_unknown_subcommand(self):
        result = cli("set", "desktop.dock_size", "64")
        self.assertEqual(result.returncode, 0, result.stderr)
        got = cli("get", "desktop.dock_size")
        self.assertEqual(got.returncode, 0, got.stderr)
        self.assertEqual(got.stdout.strip(), "64")
        config_path().parent.mkdir(parents=True, exist_ok=True)
        config_path().write_text("[desktop]\ndock_size = 999\n")
        checked = cli("check")
        self.assertEqual(checked.returncode, 1)
        self.assertIn("desktop.dock_size", checked.stderr)
        self.assertEqual(cli("bogus").returncode, 2)

    def test_show_personal_images_defaults_on_and_is_a_validated_toggle(self):
        self.assertEqual(cli("get", "privacy.show_personal_images").stdout.strip(), "true")
        self.assertIs(read()["privacy"]["showPersonalImages"], True)
        self.assertEqual(cli("set", "privacy.show_personal_images", "false").returncode, 0)
        self.assertEqual(cli("get", "privacy.show_personal_images").stdout.strip(), "false")
        self.assertIs(read()["privacy"]["showPersonalImages"], False)
        bad = cli("set", "privacy.show_personal_images", "maybe")
        self.assertNotEqual(bad.returncode, 0)
        self.assertEqual(cli("get", "privacy.show_personal_images").stdout.strip(), "false")


if __name__ == "__main__":
    unittest.main()
