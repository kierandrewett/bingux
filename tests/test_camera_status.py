import runpy
import unittest
from pathlib import Path

module = runpy.run_path(str(Path(__file__).parents[1] / "shell/bingux/camera-status.py"))
captures = module["captures"]
screen_sharing_captures = module["screen_sharing_captures"]


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

    def test_sharing_uses_portal_identity_even_without_a_running_video_node(self):
        client = {
            "type": "PipeWire:Interface:Client",
            "info": {
                "props": {
                    "pipewire.access": "portal",
                    "pipewire.access.portal.app_id": "com.rustdesk.RustDesk",
                    "pipewire.access.portal.media_roles": "",
                    "application.name": "rustdesk",
                    "application.process.binary": "rustdesk",
                },
            },
        }
        self.assertEqual(
            screen_sharing_captures([client, client]),
            [{"app": "rustdesk", "appId": "com.rustdesk.RustDesk", "device": "Screen sharing"}],
        )
        props = client["info"]["props"]
        props["pipewire.access.portal.app_id"] = ""
        self.assertEqual(screen_sharing_captures([client])[0]["appId"], "rustdesk")
        props["pipewire.access.portal.media_roles"] = "Camera"
        self.assertEqual(screen_sharing_captures([client]), [])
        del props["pipewire.access.portal.media_roles"]
        self.assertEqual(screen_sharing_captures([client]), [])
        props["pipewire.access.portal.media_roles"] = ""
        props["pipewire.access"] = "flatpak"
        self.assertEqual(screen_sharing_captures([client]), [])

    def test_sharing_lists_multiple_apps_and_handles_missing_identity(self):
        clients = [
            {
                "type": "PipeWire:Interface:Client",
                "info": {
                    "props": {
                        "pipewire.access": "portal",
                        "pipewire.access.portal.media_roles": "",
                        **identity,
                    },
                },
            }
            for identity in [{"application.name": "Firefox", "application.id": "org.mozilla.firefox"}, {}]
        ]
        self.assertEqual(
            screen_sharing_captures(clients),
            [
                {"app": "Firefox", "appId": "org.mozilla.firefox", "device": "Screen sharing"},
                {"app": "Unknown application", "appId": "", "device": "Screen sharing"},
            ],
        )


if __name__ == "__main__":
    unittest.main()
