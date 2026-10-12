#!/usr/bin/env python3
"""Small standard-library regression suite; no VM or source checkout needed."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("trace_verifier", HERE / "qml-cold-trace.py")
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)
POPUPS = verifier.DEFAULT_FORBIDDEN


def fixture(extra_events=(), extra_ranges=()):
    root = ET.Element("trace", version="1.02", traceStart="100", traceEnd="1000")
    data = ET.SubElement(root, "eventData", totalTime="900")
    events = [
        (10, "Compiling", "file:///shell/shell.qml", "shell.qml"),
        (20, "Creating", "file:///shell/shell.qml", "Quickshell/ShellRoot"),
        (30, "SceneGraph", "", ""),
    ] + list(extra_events)
    for index, kind, filename, details in events:
        event = ET.SubElement(data, "event", index=str(index))
        ET.SubElement(event, "type").text = kind
        if filename:
            ET.SubElement(event, "filename").text = filename
        if details:
            ET.SubElement(event, "details").text = details
    model = ET.SubElement(root, "profilerDataModel")
    for attrs in [
        dict(startTime="100", duration="10", eventIndex="10"),
        dict(startTime="120", duration="20", eventIndex="20"),
        dict(startTime="300", eventIndex="30"),
    ] + list(extra_ranges):
        ET.SubElement(model, "range", **attrs)
    return root


def event(index=99, kind="Compiling", filename=None, details=""):
    return (index, kind, filename or "file:///shell/" + POPUPS[0], details)


def span(index=99, start=200, duration=10):
    return dict(eventIndex=str(index), startTime=str(start), duration=str(duration))


class TraceVerifierTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name) / "test.qmltrace"

    def verify(self, root, bounded=False):
        ET.ElementTree(root).write(self.path, encoding="utf-8", xml_declaration=True)
        return verifier.verify_trace(self.path, POPUPS, bounded, "shell.qml")

    def invalid(self, root, bounded=False):
        with self.assertRaises(verifier.TraceError):
            self.verify(root, bounded)

    def test_clean_and_unused_definition(self):
        for bounded in (False, True):
            with self.subTest(bounded=bounded):
                result = self.verify(fixture([event()]), bounded)
                self.assertTrue(result["ok"])
                self.assertEqual(result["violations"], [])

    def test_all_three_popup_compiles(self):
        root = fixture(
            [event(90 + i, filename="file:///shell/" + name) for i, name in enumerate(POPUPS)],
            [span(90 + i) for i in range(3)],
        )
        for bounded in (False, True):
            result = self.verify(root, bounded)
            self.assertFalse(result["ok"])
            self.assertEqual({v["filename"] for v in result["violations"]}, set(POPUPS))
            self.assertEqual({v["type"] for v in result["violations"]}, {"Compiling"})

    def test_creating_and_caller_details(self):
        for filename, details in [
            ("qrc:/shell/CalendarPopup.qml", ""),
            ("file:///shell/shell.qml", "CalendarPopup.qml"),
        ]:
            result = self.verify(fixture([event(kind="Creating", filename=filename, details=details)], [span()]))
            self.assertFalse(result["ok"])
            self.assertEqual(result["violations"][0]["type"], "Creating")
            self.assertEqual(result["violations"][0]["filename"], "CalendarPopup.qml")

    def test_later_first_use(self):
        root = fixture([event()], [span(start=400)])
        self.assertFalse(self.verify(root)["ok"])
        self.assertTrue(self.verify(root, True)["ok"])

    def test_crossing_cutoff_and_equal_timestamp(self):
        for start, duration in [(290, 100), (300, 0)]:
            self.assertFalse(self.verify(fixture([event()], [span(start=start, duration=duration)]), True)["ok"])

    def test_earliest_referenced_marker_not_order(self):
        root = fixture([event()], [span(start=250), dict(startTime="200", eventIndex="30")])
        result = self.verify(root, True)
        self.assertTrue(result["ok"])
        self.assertEqual(result["window"]["endTime"], 200)

    def test_missing_marker_and_unreferenced_marker(self):
        root = fixture()
        root.find("profilerDataModel").remove(root.find("profilerDataModel")[-1])
        self.invalid(root, True)
        self.assertTrue(self.verify(root)["ok"])

    def test_url_encoding_and_exact_basename(self):
        self.assertFalse(
            self.verify(fixture([event(filename="file:///shell/%53ystemMetricsPopup.qml")], [span()]))["ok"]
        )
        for filename in ["file:///shell/OldSystemMetricsPopup.qml", "file:///shell/SystemMetricsPopup.qml.bak"]:
            self.assertTrue(self.verify(fixture([event(filename=filename)], [span()]))["ok"])

    def test_duplicate_and_unknown_definitions(self):
        self.invalid(fixture([event(index=10)]))
        self.invalid(fixture(extra_ranges=[span()]))
        for index in ["abc", "-1"]:
            self.invalid(fixture(extra_ranges=[dict(eventIndex=index, startTime="400")]))

    def test_bad_timings_and_missing_duration(self):
        for field, value in [("startTime", "oops"), ("startTime", "-1"), ("duration", "oops"), ("duration", "-1")]:
            attrs = span()
            attrs[field] = value
            self.invalid(fixture([event()], [attrs]), True)
        attrs = span()
        del attrs["duration"]
        self.invalid(fixture([event()], [attrs]))
        self.assertFalse(self.verify(fixture([event()], [span(duration=0)]))["ok"])

    def test_bad_bounds(self):
        for start, end in [("1000", "100"), ("100", "100"), ("no", "1000")]:
            root = fixture()
            root.set("traceStart", start)
            root.set("traceEnd", end)
            self.invalid(root)

    def test_no_qualifying_trace(self):
        for tag in ["eventData", "profilerDataModel"]:
            root = fixture()
            root.find(tag).clear()
            self.invalid(root)
        root = fixture()
        for e in root.find("eventData"):
            if e.findtext("type") != "SceneGraph":
                e.find("type").text = "DebugMessage"
        self.invalid(root)

    def test_requires_cold_types_and_entry_source(self):
        for kind in ["Compiling", "Creating"]:
            root = fixture()
            for e in root.find("eventData"):
                if e.findtext("type") == kind:
                    e.find("type").text = "DebugMessage"
            self.invalid(root)
        root = fixture()
        for e in root.find("eventData"):
            if e.find("filename") is not None:
                e.find("filename").text = "file:///shell/Other.qml"
            if e.find("details") is not None:
                e.find("details").text = "Other.qml"
        self.invalid(root)
        root = fixture()
        for r in root.find("profilerDataModel")[:2]:
            r.set("startTime", "400")
        self.invalid(root, True)

    def test_wrong_schema_missing_section_and_truncated_xml(self):
        for mutation in [
            lambda r: r.set("version", "1.03"),
            lambda r: setattr(r, "tag", "notTrace"),
            lambda r: r.remove(r.find("eventData")),
            lambda r: r.append(ET.Element("profilerDataModel")),
        ]:
            root = fixture()
            mutation(root)
            self.invalid(root)
        self.path.write_text('<trace version="1.02">')
        with self.assertRaises(verifier.TraceError):
            verifier.verify_trace(self.path, POPUPS, False, "shell.qml")

    def test_malformed_later_range_never_ignored(self):
        self.invalid(fixture([event()], [dict(eventIndex="99", startTime="900", duration="bad")]), True)

    def test_trace_start_overlap_and_outside_debug(self):
        root = fixture(
            [event(), event(index=98, kind="DebugMessage")],
            [span(start=90, duration=20), dict(eventIndex="98", startTime="10")],
        )
        self.assertFalse(self.verify(root)["ok"])

    def test_cli_exit_codes(self):
        for root, expected in [(fixture(), 0), (fixture([event()], [span()]), 1), (ET.Element("trace"), 2)]:
            ET.ElementTree(root).write(self.path)
            run = subprocess.run(
                [sys.executable, str(HERE / "qml-cold-trace.py"), str(self.path)], capture_output=True, text=True
            )
            self.assertEqual(run.returncode, expected, run.stderr + run.stdout)
            self.assertEqual(json.loads(run.stdout)["ok"], expected == 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
