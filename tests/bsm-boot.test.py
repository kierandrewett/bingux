#!/usr/bin/env python3
"""Unit tests of tests/bsm_boot_analysis.py on synthetic frames and on log lines copied from real boots.

No VM. The kernel and tracer lines below are verbatim from the 2026.10.27 image booted with
`bingux.bsm` and `drm.debug=0x15` (see docs/bsm/boot.md): they pin the formats the parsers rely on.
"""

import os
import sys
import unittest

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bsm_boot_analysis as A  # noqa: E402


def thumb(colour, mark=False, noise=False, size=(100, 160)):
    image = np.zeros((*size, 3), np.uint8)
    image[:] = colour
    if mark:
        image[40:60, 70:90] = (240, 240, 245)
    if noise:
        rng = np.random.default_rng(1)
        image = np.clip(image.astype(np.int16) + rng.integers(-40, 40, image.shape), 0, 255).astype(np.uint8)
    return image


def frame(t, scene_colour, *, size=(1280, 800), mark=False, noise=False):
    image = thumb(scene_colour, mark, noise)
    mean = float(A.thumb_luma(image).mean())
    f = A.BootFrame(t, size, "content", mean, image)
    f.scene = A.scene_of(size, mean, image)
    return f


SPLASH = (10, 10, 13)
COVER = (13, 15, 23)


class Scenes(unittest.TestCase):
    def test_each_scene_is_named(self):
        self.assertEqual(frame(0, (0, 0, 0)).scene, "black")
        self.assertEqual(frame(0, SPLASH, mark=True).scene, "splash")
        self.assertEqual(frame(0, SPLASH).scene, "flat-splash")
        self.assertEqual(frame(0, COVER).scene, "flat-cover")
        self.assertEqual(frame(0, (60, 20, 20)).scene, "flat")
        self.assertEqual(frame(0, (40, 60, 100), noise=True).scene, "content")
        self.assertEqual(frame(0, (0, 0, 0), size=(640, 480)).scene, "placeholder")
        self.assertEqual(frame(0, (40, 60, 100), size=(1024, 768)).scene, "size-change")

    def test_runs_group_consecutive_frames(self):
        frames = [frame(0, SPLASH, mark=True), frame(1, SPLASH, mark=True), frame(2, (0, 0, 0)), frame(3, SPLASH, mark=True)]
        self.assertEqual([r[0] for r in A.scene_runs(frames)], ["splash", "black", "splash"])
        self.assertEqual(A.scene_runs(frames)[0][3], 2)


class Interlude(unittest.TestCase):
    def sequence(self, *steps):
        colours = {
            "splash": dict(scene_colour=SPLASH, mark=True),
            "black": dict(scene_colour=(0, 0, 0)),
            "cover": dict(scene_colour=COVER),
            "content": dict(scene_colour=(40, 60, 100), noise=True),
        }
        return [frame(t, **colours[name]) for t, name in steps]

    def test_a_direct_cut_has_no_interlude_frames(self):
        result = A.interlude(self.sequence((1.0, "splash"), (2.0, "splash"), (5.0, "content")))
        self.assertEqual(result["entries"], [])
        self.assertEqual(result["bad_frames"], 0)
        self.assertEqual(result["seconds"], 0.0)

    def test_black_and_cover_between_splash_and_greeter_are_measured(self):
        result = A.interlude(self.sequence((1.0, "splash"), (2.0, "splash"), (2.5, "black"), (12.0, "cover"), (30.0, "content")))
        self.assertEqual(result["bad_frames"], 1)
        self.assertAlmostEqual(result["bad_seconds"], 9.5)
        self.assertAlmostEqual(result["by_scene"]["flat-cover"], 18.0)
        self.assertAlmostEqual(result["seconds"], 27.5)

    def test_no_splash_or_no_greeter_means_not_seen(self):
        self.assertIsNone(A.interlude(self.sequence((1.0, "black"), (2.0, "content"))))
        self.assertIsNone(A.interlude(self.sequence((1.0, "splash"), (2.0, "black"))))

    def test_a_splash_frame_after_black_restarts_the_window(self):
        result = A.interlude(self.sequence((1.0, "splash"), (2.0, "black"), (3.0, "splash"), (4.0, "content")))
        self.assertEqual(result["entries"], [])

    def test_frame_gaps_are_the_recording_floor(self):
        frames = self.sequence((1.0, "splash"), (1.4, "splash"), (4.4, "content"))
        self.assertAlmostEqual(A.frame_gaps(frames, 0, 10), 3.0)


# Verbatim from a baseline boot (image as shipped: plain `plymouth quit`, then bsm).
BASELINE_TRACE = """\
37.2600 start tracer up
37.2600 clients-1 plymouthd:191:M
37.2640 crtc-crtc-0 en=1 act=1 mode=1280x800
37.2640 crtc-crtc-1 en=1 act=1 mode=1280x800
37.2640 plane-plane-0 crtc=crtc-0 by=plymouthd
37.2640 plane-plane-1 off
37.2640 plane-plane-2 crtc=crtc-1 by=plymouthd
37.2940 vt tty1
37.2940 kd graphics
37.2940 vtmode process
37.2940 plymouth yes
67.7030 clients-1 plymouthd:191:-
67.7030 kd text
67.7030 vtmode auto
67.7570 clients-1 
67.7580 plymouth no
67.8130 plane-plane-0 crtc=crtc-0 by=[fbcon]
67.8130 plane-plane-2 crtc=crtc-1 by=[fbcon]
77.8790 kd graphics
77.8790 vtmode process
77.8890 clients-1 seatd:643:M
77.9290 crtc-crtc-0 en=0 act=0 mode=
77.9290 crtc-crtc-1 en=0 act=0 mode=
77.9290 plane-plane-0 off
77.9290 plane-plane-2 off
97.7170 crtc-crtc-0 en=1 act=1 mode=1280x800
97.7170 plane-plane-0 crtc=crtc-0 by=bsm
97.7720 crtc-crtc-1 en=1 act=1 mode=1280x800
97.7720 plane-plane-2 crtc=crtc-1 by=bsm
"""

# Verbatim from a handover that works: Plymouth deactivated, seatd takes the master, no CRTC change.
SEAMLESS_TRACE = """\
37.2600 start tracer up
37.2600 clients-1 plymouthd:191:M
37.2640 crtc-crtc-0 en=1 act=1 mode=1280x800
37.2640 plane-plane-0 crtc=crtc-0 by=plymouthd
37.2940 vtmode process
37.2940 plymouth yes
60.0000 clients-1 plymouthd:191:-
60.0000 vtmode auto
64.0000 vtmode process
64.0300 clients-1 plymouthd:191:- seatd:643:M
66.0000 plane-plane-0 crtc=crtc-0 by=bsm
70.0000 clients-1 seatd:643:M
70.0100 plymouth no
"""


class Tracer(unittest.TestCase):
    def test_master_flag_parsing(self):
        self.assertEqual(A.master_holder("plymouthd:191:M"), "plymouthd:191")
        self.assertEqual(A.master_holder("bingux-shell:836:- seatd:643:M"), "seatd:643")
        self.assertIsNone(A.master_holder("bingux-shell:836:-"))
        self.assertIsNone(A.master_holder(""))

    def test_baseline_shows_fbcon_and_disabled_crtcs(self):
        summary = A.summarise_trace(A.parse_trace(BASELINE_TRACE), "1")
        self.assertEqual(summary["splash_master_lost"], 67.703)
        self.assertEqual(summary["next_master"], (77.889, "seatd:643"))
        self.assertEqual({c for _, c in summary["crtc_disabled"]}, {"crtc-0", "crtc-1"})
        self.assertTrue(summary["fbcon_planes"])
        self.assertTrue(summary["scanout_off"])
        self.assertEqual(summary["bsm_first_flip"], 97.717)
        self.assertEqual(summary["splash_gone"], 67.758)

    def test_seamless_handover_has_no_disable_no_fbcon_and_bsm_before_the_splash_ends(self):
        summary = A.summarise_trace(A.parse_trace(SEAMLESS_TRACE), "1")
        self.assertEqual(summary["crtc_disabled"], [])
        self.assertEqual(summary["fbcon_planes"], [])
        self.assertEqual(summary["scanout_off"], [])
        self.assertEqual(summary["next_master"], (64.03, "seatd:643"))
        self.assertLess(summary["bsm_first_flip"], summary["splash_gone"])
        self.assertEqual(summary["modes"]["crtc-0"], ["en=1 act=1 mode=1280x800"])


DMESG = """\
[   10.999122] virtio-pci 0000:00:01.0: [drm:drm_open_helper] comm="plymouthd", pid=191, minor=1
[   11.001272] virtio-pci 0000:00:01.0: [drm:drm_ioctl] comm="plymouthd" pid=191, dev=0xe201, auth=1, DRM_IOCTL_DROP_MASTER
[   11.112510] virtio-pci 0000:00:01.0: [drm:drm_ioctl] comm="plymouthd" pid=191, dev=0xe201, auth=1, DRM_IOCTL_SET_MASTER
[   11.116638] virtio-pci 0000:00:01.0: [drm:drm_mode_setcrtc] [CRTC:39:crtc-0]
[   11.116984] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_set_mode] modeset on [CRTC:39:crtc-0]
[   11.120504] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_enable] enabling [CRTC:39:crtc-0]
[   11.262499] virtio-pci 0000:00:01.0: [drm:drm_mode_setcrtc] [CRTC:46:crtc-1]
[   11.264165] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_enable] enabling [CRTC:46:crtc-1]
[   30.000000] bsm-boot-trace: mono=30.500000
[   67.697710] virtio-pci 0000:00:01.0: [drm:drm_ioctl] comm="plymouthd" pid=191, dev=0xe201, auth=1, DRM_IOCTL_DROP_MASTER
[   67.765727] virtio-pci 0000:00:01.0: [drm:drm_file_free] comm="plymouthd", pid=191, dev=0xe201, open_count=1
[   67.810171] virtio-pci 0000:00:01.0: [drm:drm_client_dev_restore] fbdev: ret=0
[   77.894042] virtio-pci 0000:00:01.0: [drm:drm_ioctl] comm="seatd" pid=643, dev=0xe201, auth=1, DRM_IOCTL_SET_MASTER
[   77.902656] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_disable] disabling [CRTC:39:crtc-0]
[   77.902733] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_disable] disabling [CRTC:46:crtc-1]
[   97.674791] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_set_mode] modeset on [CRTC:39:crtc-0]
[   97.674891] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_enable] enabling [CRTC:39:crtc-0]
[   97.742748] virtio-pci 0000:00:01.0: [drm:drm_atomic_helper_commit_crtc_enable] enabling [CRTC:46:crtc-1]
[   99.000000] bsm-boot-trace: mono=99.500000
"""


class Kernel(unittest.TestCase):
    def setUp(self):
        self.rows = A.parse_dmesg(DMESG)
        self.events = A.drm_events(self.rows)

    def test_modesets_after_the_splashs_own(self):
        setcrtc, later = A.modesets_after_splash(self.events)
        self.assertAlmostEqual(setcrtc, 11.262499)
        self.assertEqual([(kind, detail) for _, kind, detail in later], [
            ("crtc-disable", "39 crtc-0"),
            ("crtc-disable", "46 crtc-1"),
            ("crtc-setmode", "39 crtc-0"),
            ("crtc-enable", "39 crtc-0"),
            ("crtc-enable", "46 crtc-1"),
        ])

    def test_fbcon_took_over_after_plymouth_left(self):
        restores = A.fbdev_restores_after(self.events, 60.0)
        self.assertEqual(len(restores), 1)
        self.assertAlmostEqual(restores[0][0], 67.810171)

    def test_master_calls_name_the_process(self):
        masters = [(kind, detail) for _, kind, detail in self.events if kind in ("set-master", "drop-master")]
        self.assertIn(("set-master", "seatd 643"), masters)
        self.assertIn(("drop-master", "plymouthd 191"), masters)

    def test_marks_put_the_kernel_clock_on_monotonic(self):
        self.assertAlmostEqual(A.kernel_to_monotonic(self.rows), 0.5)
        self.assertIsNone(A.kernel_to_monotonic(A.parse_dmesg("[    1.000000] nothing")))


class Journal(unittest.TestCase):
    def test_rows_and_search(self):
        rows = A.journal_rows("[   11.187223] bingux systemd[1]: Started Show Plymouth Boot Screen.\n[   16.4] x\nnoise\n")
        self.assertEqual(len(rows), 2)
        self.assertEqual(A.first_row(rows, "Plymouth")[0], 11.187223)
        self.assertIsNone(A.first_row(rows, "Plymouth", after=12.0))


if __name__ == "__main__":
    unittest.main()
