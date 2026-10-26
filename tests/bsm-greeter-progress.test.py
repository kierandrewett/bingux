#!/usr/bin/env python3
"""Keep the greeter's progress arithmetic equal to bsm-proto's.

shell/bingux/auth/ProgressModel.js repeats the stage order, work units,
counted stages and labels of packages/bsm-proto/src/progress.rs, because the
control socket sends the model, not the fraction. This test parses the Rust
source and runs the JavaScript with node, so a change on one side fails here.
It also checks the fraction against the cases the Rust unit tests state.
"""

import json
from pathlib import Path
import re
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
RUST = (ROOT / "packages/bsm-proto/src/progress.rs").read_text()
JS = ROOT / "shell/bingux/auth/ProgressModel.js"
AVAILABILITY_JS = ROOT / "shell/bingux/auth/Availability.js"
STATE = (ROOT / "packages/bsm-proto/src/state.rs").read_text()


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def rust_stages():
    order = re.search(r"pub const ALL: \[Stage; \d+\] = \[(.*?)\];", RUST, re.S).group(1)
    names = [m.group(1) for m in re.finditer(r"Stage::(\w+)", order)]
    weights = dict(re.findall(r"Stage::(\w+) => (\d+),", re.search(r"fn weight.*?\n    }\n", RUST, re.S).group(0)))
    counted = re.search(r"fn is_counted.*?matches!\(self, (.*?)\)", RUST, re.S).group(1)
    counted = {m.group(1) for m in re.finditer(r"Stage::(\w+)", counted)}
    labels = dict(re.findall(r'Stage::(\w+) => "([^"]+)"', re.search(r"fn label_while_pending.*?\n    }\n", RUST, re.S).group(0)))
    return [
        {"id": snake(n), "weight": int(weights[n]), "counted": n in counted, "label": labels[n]}
        for n in names
    ]


def node(script):
    program = "const src = require('fs').readFileSync(process.argv[1], 'utf8').replace(/^\.pragma library/m, '');\nconst m = new Function(src + '\\n;return {STAGES, fraction, pending, indeterminate, label, counts, normalise, complete};')();\n" + script
    result = subprocess.run(["node", "-e", program, str(JS)], capture_output=True, text=True)
    if result.returncode != 0:
        raise AssertionError(result.stderr)
    return json.loads(result.stdout)


class ProgressParity(unittest.TestCase):
    def test_stages_weights_counts_and_labels_match_the_rust_model(self):
        js = node("console.log(JSON.stringify(m.STAGES.map(s => ({id: s.id, weight: s.weight, counted: s.counted, label: s.label}))))")
        self.assertEqual(js, rust_stages())
        self.assertEqual(sum(s["weight"] for s in js), 100)

    def test_fraction_matches_the_rust_cases(self):
        out = node(
            """
const model = (reached, extra) => m.normalise(Object.assign({session: 's00000001', generation: 7, revision: 1, reached, counted_stage: null, done: 0, total: 0, failure: null}, extra || {}));
const early = model(['auth_accepted', 'pam_session']);
const counted = model(['auth_accepted', 'pam_session', 'host_connected', 'gnoblin_ready'], {counted_stage: 'outputs_configured', done: 1, total: 2});
const idle = model(['auth_accepted', 'pam_session', 'host_connected', 'gnoblin_ready']);
const all = model(['auth_accepted', 'pam_session', 'host_connected', 'gnoblin_ready', 'outputs_configured', 'bingux_launched', 'surfaces_mapped', 'desktop_ready', 'ready_to_reveal']);
console.log(JSON.stringify({early: m.fraction(early), counted: m.fraction(counted), idle: m.fraction(idle), idleIndeterminate: m.indeterminate(idle), countedIndeterminate: m.indeterminate(counted), all: m.fraction(all), complete: m.complete(all), label: m.label(all)}));
"""
        )
        # packages/bsm-proto/src/progress.rs tests: 5 + 10 = 15; 40 + 10 * 1 / 2 = 45.
        self.assertEqual(out["early"], 15)
        self.assertEqual(out["counted"], 45)
        self.assertEqual(out["idle"], 40)
        self.assertTrue(out["idleIndeterminate"])
        self.assertFalse(out["countedIndeterminate"])
        self.assertEqual(out["all"], 100)
        self.assertTrue(out["complete"])
        self.assertEqual(out["label"], "Ready")

    def test_the_bar_never_reads_a_clock(self):
        for name in ("ProgressModel.js", "AuthProgress.qml"):
            text = (ROOT / "shell/bingux/auth" / name).read_text()
            self.assertNotIn("Date.now", text)
            self.assertNotIn("new Date", text)
            self.assertNotIn("Timer", text, "progress must move only when the model does")

    def test_availability_values_are_the_protocol_ones(self):
        enum = re.search(r"pub enum SessionAvailability \{(.*?)\}", STATE, re.S).group(1)
        values = [snake(v.strip().rstrip(",")) for v in enum.splitlines() if v.strip() and not v.strip().startswith("//")]
        js = AVAILABILITY_JS.read_text()
        for value in values:
            self.assertIn('"' + value + '"', js, value)


if __name__ == "__main__":
    unittest.main()
