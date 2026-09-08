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

if __name__ == '__main__': unittest.main()
