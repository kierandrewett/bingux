import runpy
import unittest
from pathlib import Path

module = runpy.run_path(str(Path(__file__).parents[1] / "shell/bingux/camera-status.py"))
captures = module["captures"]


class CameraStatusTest(unittest.TestCase):
    def test_running_camera_stream_has_owner_and_device(self):
        nodes = [
            {
                "info": {
                    "state": "running",
                    "props": {
                        "media.role": "Camera",
                        "application.name": "Firefox WebRTC",
                        "application.process.binary": "firefox",
                        "node.description": "Integrated camera",
                    },
                },
            }
        ]
        self.assertEqual(
            captures(nodes),
            [{"app": "Firefox WebRTC", "appId": "firefox", "device": "Integrated camera"}],
        )

    def test_non_camera_and_non_running_nodes_are_ignored(self):
        nodes = [
            {"info": {"state": "running", "props": {"media.role": "Screen", "application.name": "OBS"}}},
            {"info": {"state": "suspended", "props": {"media.role": "Camera", "application.name": "Firefox"}}},
        ]
        self.assertEqual(captures(nodes), [])


if __name__ == "__main__":
    unittest.main()
