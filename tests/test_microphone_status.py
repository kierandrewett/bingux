import runpy
import unittest
from pathlib import Path

module = runpy.run_path(str(Path(__file__).parents[1] / 'shell/bingux/microphone-status.py'))
captures = module['captures']
affects_capture = module['affects_capture']


class MicrophoneStatusTest(unittest.TestCase):
    sources = [dict(index=1, name='microphone', description='USB microphone'),
               dict(index=2, name='speakers.monitor')]

    def stream(self, source=1, **props):
        return dict(source=source, properties={'application.name': 'Firefox', **props})

    def test_speaker_and_peak_monitors_are_not_microphones(self):
        streams = [self.stream(2), self.stream(**{'stream.capture.sink': 'true'}),
                   self.stream(**{'stream.monitor': 'true'}),
                   self.stream(**{'resample.peaks': 'true'})]
        self.assertEqual(captures(self.sources, streams), [])

    def test_snapshot_client_events_do_not_trigger_more_snapshots(self):
        for action in ('new', 'change', 'remove'):
            self.assertFalse(affects_capture(f"Event '{action}' on client #42"))
            self.assertFalse(affects_capture(f"Event '{action}' on sink-input #42"))
            self.assertTrue(affects_capture(f"Event '{action}' on source-output #42"))
            self.assertTrue(affects_capture(f"Event '{action}' on source #42"))
        self.assertTrue(affects_capture("Event 'change' on server"))

    def test_real_capture_has_owner_and_device(self):
        self.assertEqual(captures(self.sources, [self.stream()]),
                         [dict(app='Firefox', device='USB microphone', muted=False)])

    def test_muted_capture_remains_visible_and_paused_capture_does_not(self):
        stream = self.stream()
        stream['mute'] = True
        self.assertTrue(captures(self.sources, [stream])[0]['muted'])
        stream['corked'] = True
        self.assertEqual(captures(self.sources, [stream]), [])

    def test_multiple_owners_and_duplicate_streams(self):
        streams = [self.stream(), self.stream(), self.stream(**{'application.name': 'Discord'})]
        self.assertEqual([x['app'] for x in captures(self.sources, streams)], ['Firefox', 'Discord'])


if __name__ == '__main__':
    unittest.main()
