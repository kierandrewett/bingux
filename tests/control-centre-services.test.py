import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch

path = Path(__file__).resolve().parents[1] / 'shell/bingux/control-centre-services.py'
spec = importlib.util.spec_from_file_location('controls', path)
controls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controls)


class ControlTests(unittest.TestCase):
    def test_tailscale_nodes_preferences_and_exit_identity(self):
        data = {'BackendState': 'Running', 'Self': {'ID': 'self', 'HostName': 'Desktop', 'Online': True},
                'Peer': {'key': {'ID': 'exit', 'HostName': 'Gateway', 'Online': True, 'ExitNodeOption': True,
                                 'TailscaleIPs': ['100.64.0.2']},
                         'offline': {'ID': 'off', 'HostName': 'Laptop', 'Online': False}}}
        result = controls.tailscale_state(data, {'CorpDNS': True, 'RouteAll': False, 'ExitNodeID': 'exit'})
        self.assertEqual([node['id'] for node in result['nodes']], ['self', 'exit', 'off'])
        self.assertTrue(result['nodes'][0]['self'])
        self.assertEqual(result['exitNode'], 'exit')
        self.assertEqual(result['preferences'], {'accept-dns': True, 'accept-routes': False})

    def test_provider_exit_nodes_keep_location_and_are_not_tailnet_devices(self):
        nodes = controls.tailscale_state({'Peer': {
            'vpn': {'ID': 'vpn', 'ExitNodeOption': True, 'Tags': ['tag:mullvad-exit-node'],
                    'Location': {'Country': 'United Kingdom', 'CountryCode': 'GB', 'City': 'London'}},
            'gateway': {'ID': 'gateway', 'ExitNodeOption': True}}})['nodes']
        vpn = next(node for node in nodes if node['id'] == 'vpn')
        self.assertEqual((vpn['provider'], vpn['city'], vpn['country']), ('Mullvad', 'London', 'United Kingdom'))
        self.assertEqual(vpn['countryCode'], 'GB')
        self.assertEqual(next(node for node in nodes if node['id'] == 'gateway')['countryCode'], '')
        self.assertEqual(next(node for node in nodes if node['id'] == 'gateway')['provider'], '')

    def test_tailscale_actions_validate_live_exit_nodes_and_only_set_one_preference(self):
        model = controls.Controls()
        self.addCleanup(model.executor.shutdown)
        data = {'BackendState': 'Running', 'Peer': {
            'key': {'ID': 'exit', 'Online': True, 'ExitNodeOption': True, 'TailscaleIPs': ['100.64.0.2']},
            'offline': {'ID': 'offline', 'Online': False, 'ExitNodeOption': True},
            'ordinary': {'ID': 'ordinary', 'Online': True, 'ExitNodeOption': False}}}
        with patch.object(controls, 'run', return_value=json.dumps(data)) as command:
            model.apply({'kind': 'tailscale', 'setting': 'exit-node', 'value': 'exit'})
            command.assert_called_with(['tailscale', 'set', '--exit-node=100.64.0.2'], timeout=12)
            model.apply({'kind': 'tailscale', 'setting': 'exit-node', 'value': ''})
            command.assert_called_with(['tailscale', 'set', '--exit-node='], timeout=12)
            for setting in ['accept-dns', 'accept-routes', 'shields-up', 'exit-node-allow-lan-access']:
                model.apply({'kind': 'tailscale', 'setting': setting, 'enabled': False})
                command.assert_called_with(['tailscale', 'set', '--' + setting + '=false'], timeout=12)
            for target in ['offline', 'ordinary', '--reset', 'missing', None]:
                with self.assertRaises(ValueError):
                    model.apply({'kind': 'tailscale', 'setting': 'exit-node', 'value': target})
            for setting, enabled in [('ssh', True), ('accept-dns', 'false')]:
                with self.assertRaises(ValueError):
                    model.apply({'kind': 'tailscale', 'setting': setting, 'enabled': enabled})

    def test_vpn_states_do_not_confuse_private_network_with_exit_node(self):
        private = controls.tailscale_state({'BackendState': 'Running'})
        self.assertEqual(private['subtitle'], 'Private network connected')
        exit_node = controls.tailscale_state({'BackendState': 'Running', 'ExitNodeStatus': {'Online': True}})
        self.assertEqual(exit_node['subtitle'], 'Exit node active')
        self.assertFalse(controls.tailscale_state({'BackendState': 'NeedsLogin'})['canToggle'])
        self.assertEqual(controls.tailscale_state({'BackendState': 'Running', 'ExitNodeStatus': {'Online': False}})['subtitle'], 'Exit node unavailable')

    def test_mullvad_transition_and_lockdown_states_are_explicit(self):
        self.assertFalse(controls.mullvad_state({'state': 'connecting'})['canToggle'])
        self.assertIn('traffic blocked', controls.mullvad_state({'state': 'disconnected', 'details': {'locked_down': True}})['subtitle'])
        connected = controls.mullvad_state({'state': 'connected', 'details': {'location': {'country': 'Sweden'}}})
        self.assertTrue(connected['connected'])
        self.assertEqual(connected['subtitle'], 'Connected - Sweden')

    def test_only_saved_vpn_types_are_exposed(self):
        with patch.object(controls.shutil, 'which', side_effect=lambda name: name if name == 'nmcli' else None), patch.object(controls, 'run', return_value='first:vpn:Work\\: VPN:\nsecond:bridge:Docker:docker0\nthird:wireguard:Personal:wg0'):
            rows = controls.vpn_state()
        self.assertEqual([row['name'] for row in rows], ['Work: VPN', 'Personal'])
        self.assertFalse(rows[0]['connected'])
        self.assertTrue(rows[1]['connected'])

    def test_actions_keep_identifiers_as_arguments_and_never_reset_preferences(self):
        model = controls.Controls()
        self.addCleanup(model.executor.shutdown)
        model.snapshot = {'vpns': [{'id': 'tailscale', 'canToggle': True}, {'id': 'nm:abc', 'canToggle': True}]}
        with patch.object(controls, 'run') as command:
            model.apply({'kind': 'vpn', 'id': 'tailscale', 'enabled': False})
            command.assert_called_with(['tailscale', 'down'], timeout=12)
            model.apply({'kind': 'vpn', 'id': 'tailscale', 'enabled': True})
            command.assert_called_with(['tailscale', 'up'], timeout=12)
            model.apply({'kind': 'vpn', 'id': 'nm:abc', 'enabled': True})
            command.assert_called_with(['nmcli', '--wait', '10', 'connection', 'up', 'uuid', 'abc'], timeout=12)
            with self.assertRaises(ValueError):
                model.apply({'kind': 'vpn', 'id': 'nm:unknown', 'enabled': True})

    def test_unknown_actions_and_non_boolean_switches_are_rejected(self):
        model = controls.Controls()
        self.addCleanup(model.executor.shutdown)
        for request in [{'kind': 'command', 'command': 'anything'}, {'kind': 'vpn', 'enabled': 'false'}]:
            with self.assertRaises(ValueError):
                model.apply(request)


if __name__ == '__main__':
    unittest.main()
