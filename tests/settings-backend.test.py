import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('settings', Path(__file__).resolve().parents[1] / 'shell/bingux/settings-backend.py')
settings = importlib.util.module_from_spec(spec)
spec.loader.exec_module(settings)

class SettingsTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory(); self.addCleanup(directory.cleanup)
        env = patch.dict(os.environ, {'XDG_CONFIG_HOME': directory.name}); env.start(); self.addCleanup(env.stop)
    def test_roundtrip_and_partial_updates(self):
        data = settings.write({'desktop': {'dockAlignment': 'left', 'dockSize': 40}})
        self.assertEqual(settings.read(), data)
        settings.write({'previews': {'enabled': False}})
        self.assertEqual(settings.read()['desktop']['dockSize'], 40)
    def test_search_engine_validation_preserves_file_on_failure(self):
        data = settings.write({})
        before = settings.config_path().read_bytes()
        for url in ['javascript:{query}', 'https://example.com/', 'https://user:pass@example.com/{query}', 'https://{query}.com/']:
            invalid = copy.deepcopy(data); invalid['search']['engines'][0]['url'] = url
            with self.assertRaises(ValueError): settings.write(invalid)
            self.assertEqual(settings.config_path().read_bytes(), before)
        data['search']['engines'].append({'id': 'example', 'name': 'Example', 'shortcut': 'ex', 'url': 'https://example.com/?q={query}', 'enabled': True})
        data['search']['defaultEngine'] = 'example'
        self.assertEqual(settings.write(data)['search']['defaultEngine'], 'example')
    def test_rejects_duplicate_widgets_and_incompatible_destinations(self):
        layout = {'top-left': ['search'], 'top-center': ['clock'], 'top-right': [], 'dock': [], 'sidebar': ['notes']}
        settings.write({'desktop': {'layout': layout}})
        for key, items in [('dock', ['search']), ('dock', ['notes']), ('sidebar', []), ('top-left', ['search', 'search'])]:
            invalid = copy.deepcopy(layout); invalid[key] = items
            with self.assertRaises(ValueError): settings.write({'desktop': {'layout': invalid}})
    def test_rejects_invalid_behaviour_and_locations(self):
        for data in [{'desktop': {'dockSize': 999}}, {'desktop': {'dockClick': 'unknown'}}, {'search': {'fileRoots': ['relative/path']}}]:
            with self.assertRaises(ValueError): settings.write(data)

    def test_control_centre_order_roundtrip_and_validation(self):
        order = ['bluetooth', 'network', 'power', 'dnd']
        settings.write({'desktop': {'controlOrder': order}})
        self.assertEqual(settings.read()['desktop']['controlOrder'], order)
        before = settings.config_path().read_bytes()
        for invalid in [['network', 'network'], ['unknown'], [None], 'network']:
            with self.assertRaises(ValueError): settings.write({'desktop': {'controlOrder': invalid}})
            self.assertEqual(settings.config_path().read_bytes(), before)

    def test_controls_move_between_containers_without_duplicate_placements(self):
        layout = {'top-left': ['search'], 'top-center': ['clock'], 'top-right': ['controls'],
                  'dock': ['control-network'], 'sidebar': ['notes']}
        settings.write({'desktop': {'layout': layout, 'controlOrder': ['bluetooth', 'dnd']}})
        self.assertEqual(settings.read()['desktop']['layout'], layout)
        before = settings.config_path().read_bytes()
        with self.assertRaises(ValueError):
            settings.write({'desktop': {'controlOrder': ['network', 'bluetooth']}})
        with self.assertRaises(ValueError):
            settings.write({'desktop': {'controlOrder': None}})
        for item in ['control-network', 'control-unknown']:
            duplicate = copy.deepcopy(layout)
            duplicate['top-right'].append(item)
            with self.assertRaises(ValueError):
                settings.write({'desktop': {'layout': duplicate}})
        self.assertEqual(settings.config_path().read_bytes(), before)

    def test_fixed_space_width_validation(self):
        settings.write({'desktop': {'widgetOptions': {'spacer:1': {'width': 48}}}})
        self.assertEqual(settings.read()['desktop']['widgetOptions']['spacer:1']['width'], 48)
        before = settings.config_path().read_bytes()
        for key, width in [('spacer:1', 0), ('spacer:1', 161), ('spacer:1', True), ('spring:1', 40), ('search', 40)]:
            with self.assertRaises(ValueError): settings.write({'desktop': {'widgetOptions': {key: {'width': width}}}})
            self.assertEqual(settings.config_path().read_bytes(), before)

    def test_spacing_instances_roundtrip_and_validate(self):
        layout = {'top-left': ['search', 'spring:1', 'spacer:1'], 'top-center': ['clock'],
                  'top-right': ['spring:2', 'controls'], 'dock': [], 'sidebar': ['notes']}
        settings.write({'desktop': {'layout': layout}})
        self.assertEqual(settings.read()['desktop']['layout'], layout)
        before = settings.config_path().read_bytes()
        for zone, item in [('dock', 'spring:3'), ('sidebar', 'spacer:2'), ('top-left', 'spring:2'),
                           ('top-left', 'spring'), ('top-left', 'spring:0'), ('top-left', 'spacer:invalid')]:
            invalid = copy.deepcopy(layout)
            invalid[zone].append(item)
            with self.assertRaises(ValueError): settings.write({'desktop': {'layout': invalid}})
        self.assertEqual(settings.config_path().read_bytes(), before)

    def test_label_and_icon_instances_persist_with_appearance(self):
        layout = {'top-left': ['search', 'label:1'], 'top-center': ['clock'],
                  'top-right': ['controls'], 'dock': ['icon:1', 'label:2'], 'sidebar': ['notes']}
        options = {'label:1': {'label': 'Work'}, 'icon:1': {'icon': 'starred-symbolic'}}
        settings.write({'desktop': {'layout': layout, 'widgetOptions': options}})
        self.assertEqual(settings.read()['desktop']['layout'], layout)
        self.assertEqual(settings.read()['desktop']['widgetOptions'], options)
        before = settings.config_path().read_bytes()
        for zone, item in [('sidebar', 'label:3'), ('dock', 'icon:0'), ('top-left', 'label'),
                           ('dock', 'label:1'), ('dock', 'icon:10000')]:
            invalid = copy.deepcopy(layout)
            invalid[zone].append(item)
            with self.assertRaises(ValueError): settings.write({'desktop': {'layout': invalid}})
            self.assertEqual(settings.config_path().read_bytes(), before)

    def test_runtime_import_is_exact_and_happens_once(self):
        snapshot = {'version': 1, 'controlCentreReady': True,
            'layout': {'top-left': ['search'], 'top-center': ['clock'],
                'top-right': ['keyboard', 'metrics', 'controls'], 'dock': [], 'sidebar': ['notes', 'terminal']},
            'sidebar': {'edge': 'left'},
            'dock': {'pinnedApps': ['discord-canary', 'org.telegram'], 'order': ['TelegramDesktop', 'discord']},
            'controlCentre': {'vpn': True, 'dnd': True, 'nightLight': True, 'power': False, 'awake': False}}
        settings.write({'desktop': {'dockSize': 48}})
        imported = settings.import_layout(snapshot)
        self.assertEqual(imported['desktop']['layout'], snapshot['layout'])
        self.assertEqual(imported['desktop']['dockApps'], snapshot['dock'])
        self.assertEqual(imported['desktop']['controlCentre'], snapshot['controlCentre'])
        self.assertEqual(imported['desktop']['sidebarEdge'], 'left')
        self.assertEqual(imported['desktop']['dockSize'], 48)
        self.assertEqual(imported['desktop']['layoutVersion'], 1)
        backup = settings.config_path().parent / 'layout-before-import.json'
        original_backup = backup.read_bytes()
        settings.write({'desktop': {'dockSize': 64, 'sidebarEdge': 'right'}})
        self.assertEqual(settings.import_layout({})['desktop']['dockSize'], 64)
        self.assertEqual(settings.read()['desktop']['sidebarEdge'], 'right')
        self.assertEqual(backup.read_bytes(), original_backup)
        with self.assertRaises(ValueError): settings.write({'desktop': {'layoutVersion': 0}})

    def test_control_group_import_extends_existing_layout_without_resetting_it(self):
        original = settings.write({'desktop': {'layoutVersion': 1, 'dockSize': 72,
            'sidebarEdge': 'left', 'controlOrder': ['bluetooth', 'network', 'power'],
            'dockApps': {'pinnedApps': ['example.app'], 'order': ['example.app']}}})
        imported = settings.import_layout({'version': 1, 'controlCentreReady': True,
            'controlLayout': settings.native_control_layout()})
        expected = copy.deepcopy(original)
        expected['desktop']['controlLayout'] = settings.native_control_layout()
        self.assertEqual(imported, expected)
        with self.assertRaises(ValueError): settings.write({'desktop': {'controlLayout': None}})
        backup = settings.config_path().parent / 'control-layout-before-import.json'
        self.assertTrue(backup.is_file())
        previous = backup.read_bytes()
        custom = settings.native_control_layout()
        custom['groups']['control-centre'].reverse()
        settings.write({'desktop': {'controlLayout': custom}})
        self.assertEqual(settings.import_layout({})['desktop']['controlLayout'], custom)
        self.assertEqual(backup.read_bytes(), previous)

    def test_moved_control_actions_keep_one_saved_owner(self):
        layout = {'top-left': [], 'top-center': [], 'top-right': [], 'dock': [], 'sidebar': ['notes']}
        groups = settings.native_control_layout()
        settings.write({'desktop': {'layout': layout, 'controlLayout': groups}})
        for action in settings.PORTABLE_CONTROLS:
            moved = copy.deepcopy(layout); moved['dock'] = [action]
            before = settings.config_path().read_bytes()
            with self.assertRaises(ValueError): settings.write({'desktop': {'layout': moved}})
            self.assertEqual(settings.config_path().read_bytes(), before)
            owned = copy.deepcopy(groups)
            group = next(name for name, ids in settings.CONTROL_GROUPS.items() if action in ids)
            owned['groups'][group].remove(action)
            settings.write({'desktop': {'layout': moved, 'controlLayout': owned}})
            self.assertEqual(settings.read()['desktop']['layout']['dock'], [action])
            valid = settings.config_path().read_bytes()
            invalid = settings.read(); invalid['desktop']['controlLayout'] = groups
            settings.config_path().write_text(json.dumps(invalid))
            with self.assertRaises(ValueError): settings.read()
            settings.config_path().write_bytes(valid)
            settings.write({'desktop': {'layout': layout, 'controlLayout': groups}})

    def test_control_group_validation_preserves_the_last_saved_layout(self):
        settings.write({'desktop': {'controlLayout': settings.native_control_layout()}})
        before = settings.config_path().read_bytes()
        invalid = []
        for version in [2, True, '1']:
            value = settings.native_control_layout(); value['version'] = version; invalid.append(value)
        value = settings.native_control_layout(); value['groups']['controls-header'].append('control-account'); invalid.append(value)
        value = settings.native_control_layout(); value['groups']['controls-audio'] = ['control-settings']; invalid.append(value)
        value = settings.native_control_layout(); value['groups']['missing'] = []; invalid.append(value)
        for value in invalid:
            with self.assertRaises(ValueError): settings.write({'desktop': {'controlLayout': value}})
            self.assertEqual(settings.config_path().read_bytes(), before)

    def test_unknown_version_and_failed_import_do_not_replace_settings(self):
        settings.write({})
        before = settings.config_path().read_bytes()
        with self.assertRaises(ValueError): settings.import_layout({'version': 1, 'controlCentreReady': False})
        for desktop in [{'layoutVersion': 2}, {'layoutVersion': True},
                        {'dockApps': {'order': ['same', 'same'], 'pinnedApps': []}},
                        {'containers': {'dock': {'display': 'invalid'}}},
                        {'widgetOptions': {'clock': {'display': 'invalid'}}}]:
            with self.assertRaises(ValueError): settings.write({'desktop': desktop})
            self.assertEqual(settings.config_path().read_bytes(), before)

if __name__ == '__main__': unittest.main()
