import copy
import importlib.util
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
