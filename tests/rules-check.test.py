#!/usr/bin/env python3
"""scripts/rules-check.py: the registry is read from the checks, statuses come from evidence,
unknown rule IDs are found, allowlists are counted, the report survives JSON, and --offline
runs no app."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

_spec = importlib.util.spec_from_file_location("rules_check", ROOT / "scripts" / "rules-check.py")
rc = importlib.util.module_from_spec(_spec)
sys.modules["rules_check"] = rc
_spec.loader.exec_module(rc)


def small_book() -> "rc.RuleBook":
    def rule(rule_id, priority="P1", repeats=2, status="none"):
        return {"id": rule_id, "section": rule_id[:2], "rule": f"rule {rule_id}", "quote": "q", "priority": priority,
                "repeats": repeats, "status": status, "enforced_by": "hand text", "proposed_check": "p"}

    ids = ["SP-01", "SP-02", "SP-03", "SP-04", "SP-05", "AL-01", "AL-02", "AL-03", "AL-04", "AL-05"]
    return rc.RuleBook({
        "version": 1, "notes": "n",
        "sections": [{"code": "SP", "title": "Spacing", "group": "Layout", "count": 5},
                     {"code": "AL", "title": "Alignment", "group": "Layout", "count": 5}],
        "rules": [rule(i, "P0" if i == "AL-05" else "P1", 5 if i == "AL-05" else 2, "enforced" if i == "SP-01" else "none") for i in ids],
    })


def fake_registry(book, derived: dict[str, dict[str, list[str]]]) -> "rc.Registry":
    """A registry whose checks are the ones named in `derived` (check -> rule key -> IDs)."""
    out = {}
    for check_id, keys in derived.items():
        item = rc.Derived()
        for key, ids in keys.items():
            item.keys[key] = set(ids)
            item.cited.update(ids)
        out[check_id] = item
    registry = rc.Registry(book, out, {"unknown": [], "uncited": [], "documented": [], "unkeyed": []})
    return registry


def run_record(check_id, state, findings=(), exit_code=0, scope=""):
    return {"id": check_id, "title": check_id, "state": state, "exit": exit_code, "seconds": 1.0, "argv": [], "scope": scope,
            "ran_at": "", "log": "", "reused": False,
            "counts": {"fail": sum(f["severity"] == "fail" for f in findings), "allowed": sum(f["severity"] == "allowed" for f in findings),
                       "known-open": sum(f["severity"] == "known-open" for f in findings), "unattributed": 0},
            "notes": [], "findings": list(findings)}


def finding(check, key, ids, severity="fail", where="cap:node", reason=""):
    return rc.make_finding(check, key, ids, severity, where, f"{key} broke {ids}", reason)


class Statuses(unittest.TestCase):
    """A fake findings set -> the status of every rule."""

    def setUp(self):
        self.book = small_book()
        self.derived = {
            "clean-check": {"k1": ["SP-01"]},
            "red-check": {"k2": ["SP-02"], "k3": ["SP-03"]},
            "allow-check": {"k4": ["SP-04"]},
            "known-check": {"k5": ["SP-05"]},
            "idle-check": {"k6": ["AL-01"]},
            "broken-check": {"k7": ["AL-02"]},
            "half-check-a": {"k8": ["AL-03"]},
            "half-check-b": {"k9": ["AL-03"]},
        }
        self.registry = fake_registry(self.book, self.derived)
        self.runs = [
            run_record("clean-check", "clean"),
            run_record("red-check", "red", [finding("red-check", "k2", ["SP-02"]), finding("red-check", "k2", ["SP-02"], where="b:n"),
                                            finding("red-check", "k2", ["SP-02"], where="c:n"), finding("red-check", "k2", ["SP-02"], where="d:n")], 1),
            run_record("allow-check", "clean", [finding("allow-check", "k4", ["SP-04"], "allowed", reason="drawn that way")]),
            run_record("known-check", "clean", [finding("known-check", "k5", ["SP-05"], "known-open", reason="open defect")]),
            run_record("idle-check", "not-run"),
            run_record("broken-check", "errored", exit_code=2),
            run_record("half-check-a", "clean"),
            run_record("half-check-b", "not-run"),
        ]
        self.rows = {row["id"]: row for row in rc.compute_statuses(self.book, self.registry, self.runs, {"AL-05": "a person looks"})}

    def status(self, rule_id):
        return self.rows[rule_id]["status"]

    def test_a_check_that_ran_with_no_finding_is_passing(self):
        self.assertEqual(self.status("SP-01"), rc.PASSING)

    def test_failing_rules_count_their_findings_and_keep_the_first_three(self):
        row = self.rows["SP-02"]
        self.assertEqual(row["status"], rc.FAILING)
        self.assertEqual(row["counts"]["fail"], 4)
        self.assertEqual(len(row["first"]), 3)

    def test_a_rule_the_red_check_cites_but_did_not_violate_is_passing(self):
        self.assertEqual(self.status("SP-03"), rc.PASSING)

    def test_findings_an_allowlist_covers_make_the_rule_allowlisted_with_the_count(self):
        row = self.rows["SP-04"]
        self.assertEqual(row["status"], rc.ALLOWLISTED)
        self.assertEqual(row["counts"]["allowed"], 1)

    def test_known_open_defects_are_not_passing(self):
        self.assertEqual(self.status("SP-05"), rc.KNOWN_OPEN)

    def test_a_check_that_was_not_executed_gives_not_run_never_passing(self):
        self.assertEqual(self.status("AL-01"), rc.NOT_RUN)

    def test_a_check_that_crashed_gives_errored(self):
        self.assertEqual(self.status("AL-02"), rc.ERRORED)

    def test_one_check_ran_and_the_other_did_not_is_partly_run(self):
        self.assertEqual(self.status("AL-03"), rc.PARTLY_RUN)

    def test_no_check_is_unenforced_and_a_listed_rule_is_manual(self):
        self.assertEqual(self.status("AL-04"), rc.UNENFORCED)
        self.assertEqual(self.status("AL-05"), rc.MANUAL)

    def test_exit_status_follows_failing_and_errored_and_strict_adds_known_and_unenforced_p0(self):
        sections = rc.section_counts(self.book, list(self.rows.values()))
        self.assertEqual(sum(s["rules"] for s in sections), 10)
        report = {"rules": list(self.rows.values()), "checks": self.runs}
        self.assertEqual(rc.exit_code_of(report, False), 1)
        green = [run_record("clean-check", "clean")]
        quiet = {"rules": [dict(self.rows["SP-01"]), dict(self.rows["SP-05"], status=rc.KNOWN_OPEN)], "checks": green}
        self.assertEqual(rc.exit_code_of(quiet, False), 0)
        self.assertEqual(rc.exit_code_of(quiet, True), 1)
        p0 = {"rules": [dict(self.rows["SP-01"]), dict(self.rows["AL-05"], status=rc.UNENFORCED, priority="P0")], "checks": green}
        self.assertEqual(rc.exit_code_of(p0, False), 0)
        self.assertEqual(rc.exit_code_of(p0, True), 1)

    def test_a_finding_with_no_rule_ID_fails_the_gate_without_touching_a_rule(self):
        runs = [run_record("clean-check", "red", [finding("clean-check", "kx", [])], 1)]
        runs[0]["counts"]["unattributed"] = 1
        rows = rc.compute_statuses(self.book, self.registry, runs + self.runs[1:], {})
        self.assertEqual({r["id"]: r["status"] for r in rows}["SP-01"], rc.PASSING)
        self.assertEqual(rc.exit_code_of({"rules": rows, "checks": runs}, False), 1)


class FindingsFromOutput(unittest.TestCase):
    def setUp(self):
        self.book = small_book()
        self.registry = fake_registry(self.book, {"lint": {"inset": ["SP-01"], "*": ["SP-02"]}, "texty": {"slide": ["SP-03"]}})
        self.registry.derived["texty"].messages["slide"] = r"did not slide"

    def check(self, check_id="lint", parser="json"):
        return rc.Check(check_id, "t", "x.py", parser=parser)

    def test_json_findings_take_ids_from_the_finding_then_its_message_then_the_registry(self):
        rows = [
            {"rule": "inset", "capture": "c", "target": "t", "what": "[AL-01, AL-02] cited in the message"},
            {"rule": "inset", "capture": "c", "target": "t", "what": "no ids here"},
            {"rule": "other", "capture": "c", "target": "t", "what": "falls to the check default"},
            {"rule": "state", "ids": "AL-04, AL-05", "target": "t", "detail": "ids field"},
            {"rule": "inset", "capture": "c", "target": "t", "what": "covered", "allowed": "by design"},
            {"rule": "inset", "capture": "c", "target": "t", "what": "known", "known": "open defect"},
        ]
        found = rc.findings_from_json(self.check(), rows, self.registry)
        self.assertEqual([f["ids"] for f in found], [["AL-01", "AL-02"], ["SP-01"], ["SP-02"], ["AL-04", "AL-05"], ["SP-01"], ["SP-01"]])
        self.assertEqual([f["severity"] for f in found], ["fail", "fail", "fail", "fail", "allowed", "known-open"])
        self.assertEqual(found[4]["reason"], "by design")

    def test_text_findings_need_a_bracketed_ID_or_a_declared_message_and_notes_are_known_open(self):
        output = textwrap.dedent("""\
            checked 3 things
            [AL-01] a thing is wrong
            pop 1 did not slide
            note: [SP-03] pop 2 is not push 2 mirrored
            unused ALLOWED entry: x y z
            all quiet
            """)
        found, problems = rc.findings_from_text(self.check("texty", "text"), output, self.registry)
        self.assertEqual([(f["key"], f["ids"], f["severity"]) for f in found],
                         [("cited", ["AL-01"], "fail"), ("slide", ["SP-03"], "fail"), ("cited", ["SP-03"], "known-open")])
        self.assertEqual(problems, ["unused ALLOWED entry: x y z"])

    def test_frame_gate_rows_over_the_fail_level_are_findings_and_warnings_are_not(self):
        self.registry.derived["lint"].keys["page-mount"] = {"SP-04"}
        rows = [{"page": "a", "size": "default", "level": "fail", "note": "slow"}, {"page": "b", "size": "default", "level": "warn", "note": "x"}]
        found = rc.findings_from_frame_gate(self.check(parser="frame-gate"), rows, self.registry)
        self.assertEqual([(f["location"], f["ids"]) for f in found], [("a@default", ["SP-04"])])


class RegistryIsReadFromTheChecks(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "checks").mkdir()
        (self.root / "checks" / "one.py").write_text(textwrap.dedent('''\
            """Holds SP-02 (said in the docstring only) and ZZ-99."""
            RULE_IDS = {"declared": ["AL-01"], "*": ["AL-02"]}
            RULE_MESSAGES = {"declared": r"went wrong"}

            def finding(rule, ids, what):
                return (rule, ids, what)

            def judge(flag):
                return [
                    finding("pair-key", ["SP-01"], "a"),
                    ("tuple-key", ["AL-03", "AL-04"], "b"),
                    f"[SP-03, AL-05] {flag} was cited in a message",
                    finding("late" if flag else "early", ["SP-04"] if flag else ["SP-05"], "c"),
                    "[QQ-12] an unknown ID in a message",
                ]

            def run_fixtures():
                return [finding("fixture-key", ["AL-05"], "made up")]
            '''))
        (self.root / "checks" / "meta.rs").write_text(textwrap.dedent('''\
            Rule::Hover => Meta {
                key: "state-hover",
                label: "No hover state",
                ids: "ST-06, FC-04",
                why: "w",
            },
            Rule::Odd => Meta { key: "odd-key", label: "Odd \\"one\\"", ids: "AL-01", why: "w", fix: "f" },
            _ => Meta { key: "", label: "", ids: "", why: "", fix: "" },
            '''))

    def tearDown(self):
        self.tmp.cleanup()

    def test_python_keys_ids_declared_and_cited_come_from_the_ast_not_a_table(self):
        with mock.patch.object(rc, "ROOT", self.root):
            derived = rc.derive_python(self.root / "checks" / "one.py")
        self.assertEqual(derived.keys["pair-key"], {"SP-01"})
        self.assertEqual(derived.keys["tuple-key"], {"AL-03", "AL-04"})
        self.assertEqual(derived.keys["late"], {"SP-04"})
        self.assertEqual(derived.keys["early"], {"SP-05"})
        self.assertEqual(derived.keys["declared"], {"AL-01"})
        self.assertEqual(derived.keys["*"], {"AL-02"})
        self.assertNotIn("fixture-key", derived.keys)
        self.assertEqual(derived.messages, {"declared": "went wrong"})
        self.assertIn("SP-03", derived.cited)
        self.assertIn("AL-05", derived.cited)  # from the message, not from the fixture
        self.assertNotIn("SP-02", derived.cited)
        self.assertIn("SP-02", derived.documented)

    def test_rust_meta_tables_give_key_and_ids_and_the_fallback_arm_is_ignored(self):
        with mock.patch.object(rc, "ROOT", self.root):
            derived = rc.derive_rust_meta(self.root / "checks" / "meta.rs")
        self.assertEqual(derived.keys, {"state-hover": {"ST-06", "FC-04"}, "odd-key": {"AL-01"}})

    def test_ids_the_book_does_not_have_are_reported_by_check(self):
        book = small_book()
        check = rc.Check("one", "t", "checks/one.py", rust_meta=["checks/meta.rs"])
        with mock.patch.object(rc, "ROOT", self.root), mock.patch.object(rc, "CHECKS", [check]):
            registry = rc.build_registry(book, self.root)
        unknown = "\n".join(registry.problems["unknown"])
        for rule_id in ("ZZ-99", "QQ-12", "ST-06", "FC-04"):
            self.assertIn(rule_id, unknown)
        self.assertNotIn("SP-01 ", unknown)
        self.assertTrue(any("SP-02" in line for line in registry.problems["documented"]))

    def test_a_check_with_no_ids_is_reported(self):
        (self.root / "checks" / "bare.py").write_text('"""Nothing."""\n')
        check = rc.Check("bare", "t", "checks/bare.py")
        with mock.patch.object(rc, "ROOT", self.root), mock.patch.object(rc, "CHECKS", [check]):
            registry = rc.build_registry(small_book(), self.root)
        self.assertEqual(len(registry.problems["uncited"]), 1)

    def test_rule_checks_inverts_the_registry(self):
        check = rc.Check("one", "t", "checks/one.py")
        with mock.patch.object(rc, "ROOT", self.root), mock.patch.object(rc, "CHECKS", [check]):
            registry = rc.build_registry(small_book(), self.root)
            cited = registry.rule_checks()
        self.assertEqual(cited["SP-01"], [("one", ["pair-key"])])
        self.assertEqual(cited["AL-01"], [("one", ["declared"])])


class TheRealRegistry(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.book = rc.RuleBook.load()
        cls.registry = rc.build_registry(cls.book)

    def test_every_id_a_check_cites_is_in_rules_json(self):
        self.assertEqual(self.registry.problems["unknown"], [])

    def test_every_check_script_exists_and_cites_at_least_one_rule(self):
        for check in rc.CHECKS:
            self.assertTrue((ROOT / check.script).exists(), check.script)
        self.assertEqual(self.registry.problems["uncited"], [])

    def test_the_rust_rule_tables_are_in_the_registry(self):
        keys = self.registry.derived["state-check"].keys
        self.assertEqual(keys["state-hover"], {"ST-06"})
        self.assertIn("SP-01", self.registry.derived["layout-lint"].keys["inset-asymmetry"])
        self.assertEqual(len([k for k in keys if k]), 21)

    def test_no_id_cited_outside_the_checks_is_unknown(self):
        self.assertEqual(rc.stray_citations(self.book), [])

    def test_the_tool_never_wrote_results_into_rules_json(self):
        data = json.loads(RULES_TEXT())
        self.assertNotIn("generated_fields", data)
        self.assertEqual({r["status"] for r in data["rules"]} - {"enforced", "partial", "design-only", "none"}, set())
        for rule in data["rules"]:
            self.assertNotIn("hand_notes", rule)
        self.assertEqual(len(data["rules"]), 242)
        self.assertEqual(rc.dump_rules(data), RULES_TEXT(), "the file is not what json.dumps(indent=1) writes")

    def test_manual_rules_exist_in_the_book(self):
        self.assertEqual([i for i in rc.load_manual() if i not in self.book.rules], [])


def RULES_TEXT() -> str:
    return (ROOT / "design" / "rules.json").read_text()


def coverage_book() -> "rc.RuleBook":
    def rule(rule_id, status, enforced_by):
        return {"id": rule_id, "section": rule_id[:2], "rule": "r", "quote": "q", "priority": "P1", "repeats": 1,
                "status": status, "enforced_by": enforced_by, "proposed_check": "p"}

    return rc.RuleBook({
        "version": 1, "notes": "n", "sections": [{"code": "SP", "title": "S", "group": "G", "count": 6}],
        "rules": [
            rule("SP-01", "enforced", "script: layout-lint:inset-asymmetry [SP-01] and ui-lint (source)"),
            rule("SP-02", "enforced", "scripts/state-check.py touch-target; unit: tokens.rs"),
            rule("SP-03", "none", "none"),
            rule("SP-04", "design-only", "Docs only"),
            rule("SP-05", "partial", "unit: notice_tests.rs"),
            rule("SP-06", "enforced", "pixel-check.py text-contrast; number-roll"),
        ],
    })


class DeclaredCoverage(unittest.TestCase):
    """enforced_by and status of rules.json are declared by hand; the registry validates them and never overwrites them,
    except `enforced_by` by --sync-coverage."""

    def setUp(self):
        self.book = coverage_book()
        self.registry = fake_registry(self.book, {
            "layout-lint": {"inset-asymmetry": ["SP-01", "SP-03"]},
            "ui-lint": {"direct-styling": ["SP-05"]},
            "state-check": {"touch-target": ["SP-02"]},
            "pixel-check": {"text-contrast": ["SP-06"]},
        })

    def test_a_rule_that_names_a_check_that_does_not_cite_it_is_reported(self):
        found = rc.validate_coverage(self.book, self.registry)
        self.assertIn("SP-01: enforced_by names ui-lint, which does not cite it", found["named_not_cited"])
        self.assertEqual(len(found["named_not_cited"]), 1)

    def test_a_check_that_cites_a_rule_the_declaration_omits_is_reported(self):
        found = rc.validate_coverage(self.book, self.registry)
        self.assertIn("SP-03: layout-lint cites it, enforced_by omits it", found["cited_not_named"])
        self.assertIn("SP-05: ui-lint cites it, enforced_by omits it", found["cited_not_named"])
        self.assertEqual(len(found["cited_not_named"]), 2)

    def test_the_declared_status_is_checked_against_the_registry_both_ways(self):
        found = rc.validate_coverage(self.book, self.registry)
        self.assertEqual([line.split(" ")[0] for line in found["declared_none_but_cited"]], ["SP-03"])
        self.assertEqual(found["declared_enforced_no_check"], [])
        book = coverage_book()
        registry = fake_registry(book, {"layout-lint": {"inset-asymmetry": ["SP-04"]}})
        found = rc.validate_coverage(book, registry)
        self.assertEqual(sorted(line.split(" ")[0] for line in found["declared_enforced_no_check"]), ["SP-01", "SP-02", "SP-05", "SP-06"])

    def test_aliases_name_a_check_the_way_the_book_does(self):
        found = rc.validate_coverage(self.book, self.registry)
        self.assertFalse([line for line in found["cited_not_named"] if line.startswith("SP-06")])

    def test_sync_changes_enforced_by_and_nothing_else(self):
        before = json.loads(json.dumps(self.book.data))
        after, changed = rc.sync_coverage(self.book.data, self.registry)
        self.assertEqual(json.loads(json.dumps(self.book.data)), before, "the input was changed")
        self.assertEqual(sorted(changed), ["SP-01", "SP-02", "SP-03", "SP-05", "SP-06"])
        for rule_after, rule_before in zip(after["rules"], before["rules"]):
            self.assertEqual({k: v for k, v in rule_after.items() if k != "enforced_by"},
                             {k: v for k, v in rule_before.items() if k != "enforced_by"})
        by_id = {r["id"]: r["enforced_by"] for r in after["rules"]}
        self.assertEqual(by_id["SP-01"], "script: layout-lint:inset-asymmetry [SP-01] and ui-lint (source) || registry: layout-lint[inset-asymmetry]")
        self.assertEqual(by_id["SP-03"], "registry: layout-lint[inset-asymmetry]")
        self.assertEqual(by_id["SP-04"], "Docs only")
        self.assertEqual({r["id"]: r["status"] for r in after["rules"]}, {r["id"]: r["status"] for r in before["rules"]})

    def test_sync_twice_is_the_same_and_a_vanished_citation_takes_its_segment_away(self):
        once, _ = rc.sync_coverage(self.book.data, self.registry)
        twice, changed = rc.sync_coverage(once, self.registry)
        self.assertEqual(changed, [])
        self.assertEqual(rc.dump_rules(once), rc.dump_rules(twice))
        registry = fake_registry(self.book, {"layout-lint": {"inset-asymmetry": ["SP-01"]}})
        later, changed = rc.sync_coverage(once, registry)
        by_id = {r["id"]: r["enforced_by"] for r in later["rules"]}
        self.assertEqual(by_id["SP-03"], "none")
        self.assertEqual(by_id["SP-02"], "scripts/state-check.py touch-target; unit: tokens.rs")
        self.assertIn("SP-03", changed)

    def test_main_syncs_only_when_asked_and_only_enforced_by(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "rules.json"
            path.write_text(rc.dump_rules(self.book.data))
            original = path.read_text()
            with mock.patch.object(rc, "RULES_JSON", path), mock.patch.object(rc, "MANUAL_JSON", Path(tmp) / "none.json"), \
                    mock.patch.object(rc, "build_registry", return_value=self.registry), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(rc.main(["--registry"]), 0)
                self.assertEqual(path.read_text(), original, "--registry wrote rules.json")
                self.assertEqual(rc.main(["--sync-coverage"]), 0)
            synced = json.loads(path.read_text())
            self.assertNotEqual(path.read_text(), original)
            self.assertEqual([r["status"] for r in synced["rules"]], [r["status"] for r in self.book.data["rules"]])
            self.assertNotIn("generated_fields", synced)
            self.assertEqual(rc.dump_rules(synced), path.read_text())

    def test_a_file_not_in_the_dump_format_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "rules.json"
            path.write_text(json.dumps(self.book.data))
            with mock.patch.object(rc, "RULES_JSON", path), mock.patch.object(rc, "MANUAL_JSON", Path(tmp) / "none.json"), \
                    mock.patch.object(rc, "build_registry", return_value=self.registry), contextlib.redirect_stdout(io.StringIO()), \
                    contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(rc.main(["--sync-coverage"]), 2)
            self.assertEqual(path.read_text(), json.dumps(self.book.data))


class Allowlists(unittest.TestCase):
    def test_python_allowlists_are_listed_with_their_reasons_and_matched_findings_are_counted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "c.py").write_text(textwrap.dedent('''\
                _WHY = "shared reason"
                ALLOWED = [
                    {"rule": "k1", "capture": "cap", "where": "Node", "why": "drawn that way on purpose"},
                    {"rule": "k1", "capture": "cap", "where": "Other", "why": "never matched"},
                ]
                KNOWN = [_known("k2", "Thing", _WHY)]
                BLOCKING_ALLOWED = {"a/b.rs": "the worker pool"}
                OTHER = [{"rule": "no", "why": "not a list that matters"}]
                '''))
            check = rc.Check("c", "t", "c.py", parser="json")
            with mock.patch.object(rc, "ROOT", root):
                entries = rc.python_allowlists(check, None)
        by_reason = {e["reason"]: e for e in entries}
        self.assertEqual(set(by_reason), {"drawn that way on purpose", "never matched", "shared reason", "the worker pool"})
        self.assertEqual(by_reason["shared reason"]["kind"], "known-open")
        self.assertEqual(by_reason["the worker pool"]["where"], "a/b.rs")
        book = small_book()
        registry = fake_registry(book, {"c": {"k1": ["SP-01"]}})
        runs = [run_record("c", "clean", [finding("c", "k1", ["SP-01"], "allowed", reason="drawn that way on purpose")] * 3)]
        report = rc.build_report(book, registry, runs, {}, {"name": "full", "selected": 1, "jobs": 2, "reuse": ""}, _tree(), entries)
        counted = {e["reason"]: e["matched"] for e in report["allowlists"]}
        self.assertEqual(counted["drawn that way on purpose"], 3)
        self.assertEqual(counted["never matched"], 0)

    def test_the_real_allowlists_are_found(self):
        total = [e for check in rc.CHECKS for e in rc.python_allowlists(check, None) + rc.rust_allowlists(check)]
        self.assertTrue(all(e["reason"] for e in total), "an allowlist entry without a reason")
        self.assertTrue(any(e["check"] == "state-check" and e["kind"] == "known-open" for e in total))
        self.assertTrue(any(e["list"].startswith("Rust") for e in total))


def _tree() -> dict:
    return {"commit": "abc", "branch": "b", "dirty_files": 0, "target_dir": "/t", "newest_source": "now",
            "binaries": {"bingux-settings": {"built": "x", "older_than_source": False}}}


class ReportRoundTrip(unittest.TestCase):
    def test_the_report_is_plain_json_and_renders_the_same_after_a_round_trip(self):
        book = small_book()
        registry = fake_registry(book, {"c": {"k": ["SP-01", "SP-02"]}, "d": {"j": ["AL-01"]}})
        runs = [run_record("c", "red", [finding("c", "k", ["SP-02"]), finding("c", "k", ["SP-02"], "allowed", reason="why")], 1),
                run_record("d", "not-run")]
        entries = [{"check": "c", "list": "ALLOWED", "kind": "allowed", "rule": "k", "where": "n", "reason": "why", "source": "c.py:1"}]
        with mock.patch.object(rc, "layout_keys_without_ids", return_value=["clipped"]), mock.patch.object(rc, "stray_citations", return_value=[]):
            report = rc.build_report(book, registry, runs, {"AL-05": "look"}, {"name": "full", "selected": 2, "jobs": 2, "reuse": ""}, _tree(), entries)
        loaded = json.loads(json.dumps(report))
        self.assertEqual(loaded, report)
        self.assertEqual(rc.render_markdown(loaded), rc.render_markdown(report))
        self.assertEqual(rc.render_summary(loaded), rc.render_summary(report))
        text = rc.render_markdown(report)
        for heading in ("## Counts", "## Checks", "## Sections", "## Top failing rules", "## Unenforced P0 and P1 rules, by repeats",
                        "## Allowlist entries", "## Registry integrity", "SP-02"):
            self.assertIn(heading, text)
        self.assertIn("enforced+failing (1)", text)
        self.assertIn("not run", text)
        self.assertEqual(report["totals"]["rules"], 10)
        self.assertEqual(sum(report["totals"][s] for s in rc.STATUSES), 10)

    def test_the_terminal_summary_has_one_row_per_section_and_a_total(self):
        book = small_book()
        registry = fake_registry(book, {"c": {"k": ["SP-01"]}})
        with mock.patch.object(rc, "layout_keys_without_ids", return_value=[]), mock.patch.object(rc, "stray_citations", return_value=[]):
            report = rc.build_report(book, registry, [run_record("c", "clean")], {}, {"name": "full", "selected": 1, "jobs": 2, "reuse": ""}, _tree(), [])
        lines = rc.render_summary(report).splitlines()
        self.assertTrue(any(line.startswith("SP ") for line in lines))
        self.assertTrue(any(line.startswith("AL ") for line in lines))
        self.assertTrue(any("TOTAL" in line for line in lines))
        self.assertLess(len(lines), 50)


class OfflineMode(unittest.TestCase):
    """--offline runs the checks that need no app, and the report says the others did not run."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        scripts = self.root / "scripts"
        scripts.mkdir()
        self.marker = self.root / "app-started"
        self.counter = self.root / "count"
        (scripts / "quiet.py").write_text('print("quiet: nothing to say")\n')
        (scripts / "loud.py").write_text(textwrap.dedent(f'''\
            import pathlib, sys
            RULE_IDS = {{"loud-key": ["SP-02"]}}
            c = pathlib.Path({str(self.counter)!r})
            c.write_text(str(int(c.read_text() or 0) + 1) if c.exists() else "1")
            print("[SP-02] one thing is wrong")
            sys.exit(1)
            '''))
        (scripts / "needs_app.py").write_text(textwrap.dedent(f'''\
            import pathlib
            RULE_IDS = {{"app-key": ["AL-01"]}}
            pathlib.Path({str(self.marker)!r}).write_text("started")
            '''))
        self.checks = [
            rc.Check("quiet", "q", "scripts/quiet.py", offline_args=[], sources=[]),
            rc.Check("loud", "l", "scripts/loud.py", offline_args=[]),
            rc.Check("needs-app", "a", "scripts/needs_app.py"),
        ]
        (scripts / "quiet.py").write_text('RULE_IDS = {"quiet-key": ["SP-01"]}\nprint("quiet: nothing to say")\n')

    def tearDown(self):
        self.tmp.cleanup()

    def main(self, *args):
        out = io.StringIO()
        with mock.patch.object(rc, "ROOT", self.root), mock.patch.object(rc, "CHECKS", self.checks), \
                mock.patch.object(rc, "CHECK_BY_ID", {c.id: c for c in self.checks}), \
                mock.patch.object(rc, "RuleBook", rc.RuleBook), \
                mock.patch.object(rc, "RULES_JSON", ROOT / "design" / "rules.json"), \
                mock.patch.object(rc, "MANUAL_JSON", self.root / "none.json"), \
                mock.patch.object(rc, "layout_keys_without_ids", return_value=[]), \
                mock.patch.object(rc, "stray_citations", return_value=[]), \
                mock.patch.object(rc, "tree_state", return_value=_tree()), \
                mock.patch.object(rc, "rust_allowlists", return_value=[]), \
                contextlib.redirect_stdout(out):
            code = rc.main(["--out", str(self.root / "out"), *args])
        return code, out.getvalue()

    def report(self) -> dict:
        return json.loads((self.root / "out" / "rules-report.json").read_text())

    def test_offline_runs_no_app_and_marks_its_rules_not_run(self):
        code, text = self.main("--offline")
        self.assertFalse(self.marker.exists(), "an app check ran in --offline mode")
        self.assertEqual(code, 1, text)
        report = self.report()
        status = {row["id"]: row["status"] for row in report["rules"]}
        self.assertEqual(status["SP-01"], rc.PASSING)
        self.assertEqual(status["SP-02"], rc.FAILING)
        self.assertEqual(status["AL-01"], rc.NOT_RUN)
        states = {run["id"]: run["state"] for run in report["checks"]}
        self.assertEqual(states, {"quiet": "clean", "loud": "red", "needs-app": "not-run"})
        self.assertEqual(report["mode"]["name"], "offline")
        self.assertIn("needs the apps", [r for r in report["checks"] if r["id"] == "needs-app"][0]["notes"][0])

    def test_a_partial_run_does_not_overwrite_the_committed_report_and_no_run_writes_rules_json(self):
        design = ROOT / "design"
        watched = (design / "rules-report.md", design / "rules-report.json", design / "rules.json")
        before = {p: p.read_bytes() if p.exists() else None for p in watched}
        self.main("--offline")
        with mock.patch.object(rc, "REPORT_MD", self.root / "full.md"), mock.patch.object(rc, "REPORT_JSON", self.root / "full.json"):
            self.main()
        after = {p: p.read_bytes() if p.exists() else None for p in before}
        self.assertEqual(before, after)
        self.assertTrue((self.root / "out" / "rules-report.md").exists())

    def test_a_full_run_runs_the_app_check_too(self):
        with mock.patch.object(rc, "REPORT_MD", self.root / "full.md"), mock.patch.object(rc, "REPORT_JSON", self.root / "full.json"):
            code, _ = self.main()
        self.assertTrue(self.marker.exists())
        self.assertTrue((self.root / "full.json").exists())
        self.assertEqual(json.loads((self.root / "full.json").read_text())["mode"]["name"], "full")

    def test_only_and_skip_select_checks(self):
        self.main("--only", "quiet")
        states = {run["id"]: run["state"] for run in self.report()["checks"]}
        self.assertEqual(states, {"quiet": "clean", "loud": "not-run", "needs-app": "not-run"})
        self.main("--skip", "loud,needs-app")
        states = {run["id"]: run["state"] for run in self.report()["checks"]}
        self.assertEqual(states["quiet"], "clean")
        self.assertEqual(states["loud"], "not-run")

    def test_reuse_reads_the_earlier_output_and_does_not_run_the_check_again(self):
        self.main("--only", "loud")
        self.assertEqual(self.counter.read_text(), "1")
        reuse = self.root / "out"
        again = self.root / "again"
        out = io.StringIO()
        with mock.patch.object(rc, "ROOT", self.root), mock.patch.object(rc, "CHECKS", self.checks), \
                mock.patch.object(rc, "CHECK_BY_ID", {c.id: c for c in self.checks}), \
                mock.patch.object(rc, "RULES_JSON", ROOT / "design" / "rules.json"), \
                mock.patch.object(rc, "MANUAL_JSON", self.root / "none.json"), \
                mock.patch.object(rc, "layout_keys_without_ids", return_value=[]), \
                mock.patch.object(rc, "stray_citations", return_value=[]), \
                mock.patch.object(rc, "tree_state", return_value=_tree()), \
                mock.patch.object(rc, "rust_allowlists", return_value=[]), \
                contextlib.redirect_stdout(out):
            rc.main(["--out", str(again), "--only", "loud", "--reuse", str(reuse)])
        self.assertEqual(self.counter.read_text(), "1", "the check ran again")
        report = json.loads((again / "rules-report.json").read_text())
        run = [r for r in report["checks"] if r["id"] == "loud"][0]
        self.assertTrue(run["reused"])
        self.assertEqual(run["state"], "red")

    def test_an_unknown_check_name_is_refused(self):
        with self.assertRaises(SystemExit):
            self.main("--only", "no-such-check")


class IncompleteRuns(unittest.TestCase):
    """A check that did not measure everything never gives a rule `passing`."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.raw = Path(self.tmp.name)
        self.book = small_book()
        self.registry = fake_registry(self.book, {"cap": {"k": ["SP-01"]}})

    def tearDown(self):
        self.tmp.cleanup()

    def evaluate(self, log: str, exit_code: int, findings=None, parser="json"):
        check = rc.Check("cap", "t", "x.py", parser=parser, json_flag=parser == "json")
        (self.raw / "cap.log").write_text(log)
        if findings is not None:
            (self.raw / "cap.json").write_text(json.dumps(findings))
        return rc.evaluate(check, {"exit": exit_code, "seconds": 1, "argv": [], "scope": ""}, self.raw, self.registry)

    def rules(self, record):
        return {r["id"]: r["status"] for r in rc.compute_statuses(self.book, self.registry, [record], {})}

    def test_a_clean_run_is_complete_and_passes(self):
        record = self.evaluate("12 captures, 0 findings\n", 0, [])
        self.assertEqual((record["state"], record["complete"]), ("clean", True))
        self.assertEqual(self.rules(record)["SP-01"], rc.PASSING)

    def test_a_capture_that_timed_out_means_partly_run_not_passing(self):
        record = self.evaluate("no captures: taxonomy (timed out after 600s), see x\n\n12 captures, 0 findings\n", 1, [])
        self.assertEqual((record["state"], record["complete"]), ("errored", False))
        self.assertEqual(self.rules(record)["SP-01"], rc.ERRORED)

    def test_findings_still_fail_a_rule_when_the_run_was_incomplete(self):
        record = self.evaluate("no captures: taxonomy (timed out)\n", 1, [{"rule": "k", "capture": "c", "target": "t", "what": "bad"}])
        self.assertEqual((record["state"], record["complete"]), ("red", False))
        self.assertEqual(self.rules(record)["SP-01"], rc.FAILING)

    def test_sweeps_that_stopped_early_are_incomplete_even_with_exit_zero(self):
        record = self.evaluate("181 captures, 3 findings, 52 sweeps stopped early, 0 crashed\n", 0, [])
        self.assertEqual((record["state"], record["complete"]), ("clean", False))
        self.assertEqual(self.rules(record)["SP-01"], rc.PARTLY_RUN)

    def test_an_unused_allowlist_entry_alone_makes_the_check_red_but_what_it_measured_stands(self):
        record = self.evaluate("unused ALLOWED entry: k cap node: why\n\n10 captures, 0 findings\n", 1, [])
        self.assertEqual((record["state"], record["complete"]), ("red", True))
        self.assertEqual(self.rules(record)["SP-01"], rc.PASSING)
        report = {"rules": rc.compute_statuses(self.book, self.registry, [record], {}), "checks": [record]}
        self.assertEqual(rc.exit_code_of(report, False), 1)

    def test_success_without_a_findings_file_proves_nothing(self):
        record = self.evaluate("12 captures\n", 0, None)
        self.assertEqual((record["state"], record["complete"]), ("errored", False))

    def test_a_gate_only_check_that_fails_is_a_violation_of_its_default_rules(self):
        self.registry.derived["cap"].keys["*"] = {"SP-02"}
        check = rc.Check("cap", "t", "x.py", gate_only=True)
        (self.raw / "cap.log").write_text("out of date with labels.tsv: a, b\n")
        record = rc.evaluate(check, {"exit": 1, "seconds": 1, "argv": [], "scope": ""}, self.raw, self.registry)
        self.assertEqual(record["state"], "red")
        self.assertEqual(record["findings"][0]["ids"], ["SP-02"])

    def test_arg_adds_options_to_one_checks_command_only(self):
        ctx = rc.Context(out=self.raw, jobs=2, build=False, offline=False, reuse=None, extra={"cap": ["--timeout", "1800"]})
        cap = rc.Check("cap", "t", "scripts/x.py", jobs_flag=True)
        other = rc.Check("other", "t", "scripts/y.py", jobs_flag=True)
        self.assertEqual(rc.command_for(cap, ctx, self.raw / "cap.json")[-4:], ["--jobs", "2", "--timeout", "1800"])
        self.assertEqual(rc.command_for(other, ctx, self.raw / "o.json")[-2:], ["--jobs", "2"])



class StaleBinaries(unittest.TestCase):
    """A binary older than the source it was built from measured an older tree: no rule passes on it."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.source = self.root / "apps" / "settings" / "src" / "main.rs"
        self.source.parent.mkdir(parents=True)
        self.source.write_text("fn main() {}")
        (self.root / "apps" / "settings" / "Cargo.toml").write_text("")
        (self.root / "crates" / "ui").mkdir(parents=True)
        (self.root / "crates" / "ui" / "Cargo.toml").write_text("")
        self.target = self.root / "target"
        (self.target / "debug").mkdir(parents=True)
        self.binary = self.target / "debug" / "bingux-settings"
        self.binary.write_text("elf")
        (self.target / "debug" / "bingux-settings.d").write_text(f"{self.binary}: {self.source}\n\n{self.source}:\n")

    def tearDown(self):
        self.tmp.cleanup()

    def set_times(self, built: float, source: float):
        import os
        os.utime(self.binary, (built, built))
        os.utime(self.source, (source, source))
        for manifest in self.root.glob("*/*/Cargo.toml"):
            os.utime(manifest, (1, 1))

    def test_a_binary_older_than_a_source_in_its_dep_info_is_stale_and_names_the_file(self):
        self.set_times(built=1000, source=2000)
        info = rc.binary_freshness(self.target, self.root)["bingux-settings"]
        self.assertTrue(info["stale"])
        self.assertEqual(info["newest_source"], "apps/settings/src/main.rs")
        self.assertEqual((info["built_epoch"], info["newest_source_epoch"]), (1000, 2000))

    def test_a_binary_newer_than_every_source_is_fresh(self):
        self.set_times(built=3000, source=2000)
        self.assertFalse(rc.binary_freshness(self.target, self.root)["bingux-settings"]["stale"])

    def test_a_missing_binary_is_stale(self):
        self.assertTrue(rc.binary_freshness(self.target, self.root)["bingux-taxonomy"]["stale"])

    def test_only_checks_that_render_an_app_need_fresh_binaries(self):
        by_id = {check.id: check for check in rc.CHECKS}
        self.assertFalse(by_id["ui-lint"].needs_binaries(False))
        self.assertFalse(by_id["icons-golden"].needs_binaries(False))
        self.assertTrue(by_id["copy-lint"].needs_binaries(False))
        self.assertFalse(by_id["copy-lint"].needs_binaries(True))
        self.assertTrue(by_id["layout-lint"].needs_binaries(False))
        self.assertFalse(by_id["unit-tests"].needs_binaries(False))

    def evaluate(self, meta_extra: dict, allow_stale: bool):
        book = small_book()
        registry = fake_registry(book, {"cap": {"k": ["SP-01"]}})
        check = rc.Check("cap", "t", "x.py", parser="json", json_flag=True)
        (self.root / "cap.log").write_text("12 captures, 0 findings\n")
        (self.root / "cap.json").write_text("[]")
        meta = {"exit": 0, "seconds": 1, "argv": [], "scope": "", **meta_extra}
        record = rc.evaluate(check, meta, self.root, registry, allow_stale=allow_stale)
        status = {r["id"]: r["status"] for r in rc.compute_statuses(book, registry, [record], {})}
        return record, status

    def test_a_check_on_stale_binaries_is_partly_run_never_passing_and_the_exit_is_one(self):
        record, status = self.evaluate({"stale_binaries": ["bingux-settings"]}, allow_stale=False)
        self.assertEqual(status["SP-01"], rc.PARTLY_RUN)
        self.assertEqual(record["stale"], ["bingux-settings"])
        self.assertIn("older than their sources", record["notes"][0])
        report = {"rules": rc.compute_statuses(small_book(), fake_registry(small_book(), {"cap": {"k": ["SP-01"]}}), [record], {}), "checks": [record]}
        self.assertEqual(rc.exit_code_of(report, False), 1)

    def test_allow_stale_reports_the_run_as_measured(self):
        record, status = self.evaluate({"stale_binaries": ["bingux-settings"]}, allow_stale=True)
        self.assertEqual(status["SP-01"], rc.PASSING)
        self.assertEqual(record["stale"], [])

    def test_fresh_binaries_pass(self):
        record, status = self.evaluate({"stale_binaries": []}, allow_stale=False)
        self.assertEqual(status["SP-01"], rc.PASSING)

    def test_an_earlier_run_that_did_not_record_freshness_counts_as_stale(self):
        record, status = self.evaluate({}, allow_stale=False)
        self.assertEqual(status["SP-01"], rc.PARTLY_RUN)
        self.assertIn("not recorded", record["notes"][0])


class UnitTestsAsARegistrySource(unittest.TestCase):
    """tests/rules-tests.json maps cargo test modules to rules; the run reports pass and fail per rule."""

    LOG = textwrap.dedent("""\
        running 6 tests
        test controls::alpha::one ... ok
        test controls::alpha::two ... ok
        test controls::beta::sinks ... FAILED
        test controls::beta::floats ... ok
        test controls::gamma::skipped ... ignored
        test other::controls::alpha::three ... ok
        test result: FAILED. 4 passed; 1 failed; 1 ignored
        """)

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        src = self.root / "crates" / "ui" / "src" / "controls"
        src.mkdir(parents=True)
        (src / "alpha.rs").write_text("#[test]\nfn one() {}\n")
        (src / "beta.rs").write_text("#[test]\nfn sinks() {}\n")
        (src / "gamma.rs").write_text("#[test]\nfn skipped() {}\n")
        (src / "none.rs").write_text("fn no_test() {}\n")
        self.map = self.root / "rules-tests.json"
        self.map.write_text(json.dumps({"package": "bingux-ui", "features": "testing", "tests": [
            {"module": "controls::alpha", "rules": ["SP-01"], "covers": "a"},
            {"module": "controls::beta", "rules": ["SP-02", "AL-01"], "covers": "b"},
            {"module": "controls::gamma", "rules": ["AL-02"], "covers": "c"},
            {"module": "controls::gone", "rules": ["AL-03"], "covers": "d"},
        ]}))
        self.book = small_book()
        self.check = rc.Check("unit-tests", "t", "rules-tests.json", kind="unit-tests")
        self.derived = rc.derive_unit_tests(self.map)
        self.registry = rc.Registry(self.book, {"unit-tests": self.derived}, {"unknown": [], "uncited": [], "documented": [], "unkeyed": [], "tests_unmatched": []})
        self.raw = self.root / "raw"
        self.raw.mkdir()

    def tearDown(self):
        self.tmp.cleanup()

    def record(self, log: str, exit_code: int = 101):
        (self.raw / "unit-tests.log").write_text(log)
        return rc.evaluate(self.check, {"exit": exit_code, "seconds": 1, "argv": [], "scope": ""}, self.raw, self.registry)

    def statuses(self, record):
        return {r["id"]: r for r in rc.compute_statuses(self.book, self.registry, [record], {})}

    def test_the_map_is_the_registry_source_of_the_rules_it_names(self):
        self.assertEqual(self.derived.cited, {"SP-01", "SP-02", "AL-01", "AL-02", "AL-03"})
        self.assertEqual(self.derived.keys["controls::beta"], {"SP-02", "AL-01"})
        self.assertEqual(self.registry.rule_checks()["SP-02"], [("unit-tests", ["controls::beta"])])

    def test_a_module_with_a_test_is_found_in_the_source_and_one_without_is_not(self):
        by_module = {entry["module"]: entry for entry in self.derived.tests}
        self.assertTrue(rc.test_files_for(by_module["controls::alpha"], self.root))
        self.assertEqual(rc.test_files_for(by_module["controls::gone"], self.root), [])
        self.assertEqual(rc.test_files_for({"package": "bingux-ui", "module": "controls::none"}, self.root), [])

    def test_a_test_file_is_found_at_the_path_it_is_mounted_at_not_where_it_sits(self):
        # `controls/delta_tests.rs` sits beside `delta.rs` but is declared inside it, so cargo calls
        # its tests `controls::delta::tests`: a map entry that names the file's place runs no test.
        src = self.root / "crates" / "ui" / "src" / "controls"
        (src / "delta.rs").write_text('#[cfg(test)]\n#[path = "delta_tests.rs"]\nmod tests;\n')
        (src / "delta_tests.rs").write_text("#[test]\nfn one() {}\n")
        (src / "epsilon.rs").write_text("pub fn f() {}\n#[cfg(test)]\nmod edge_tests {\n    #[test]\n    fn one() {}\n}\n")
        found = lambda module: rc.test_files_for({"package": "bingux-ui", "module": module}, self.root)  # noqa: E731
        self.assertTrue(found("controls::delta::tests"))
        self.assertEqual(found("controls::delta_tests"), [])
        self.assertTrue(found("controls::epsilon::edge_tests"))
        self.assertEqual(found("controls::epsilon::tests"), [])  # the file has tests, but not a module of that name

    def test_pass_and_fail_come_per_rule_from_the_tests_of_its_modules(self):
        rows = self.statuses(self.record(self.LOG))
        self.assertEqual(rows["SP-01"]["status"], rc.PASSING)
        self.assertEqual([(t["module"], t["passed"]) for t in rows["SP-01"]["tests"]], [("controls::alpha", 2)])  # not other::controls::alpha
        self.assertEqual((rows["SP-02"]["status"], rows["AL-01"]["status"]), (rc.FAILING, rc.FAILING))
        self.assertEqual(rows["SP-02"]["tests"][0]["failed"], 1)
        self.assertEqual(rows["SP-02"]["first"][0]["location"], "controls::beta::sinks")

    def test_a_module_whose_tests_are_all_ignored_or_gone_fails_its_rules(self):
        rows = self.statuses(self.record(self.LOG))
        self.assertEqual(rows["AL-02"]["status"], rc.FAILING)  # ignored only
        self.assertEqual(rows["AL-03"]["status"], rc.FAILING)  # no test of that module ran
        self.assertIn("no test of this module ran", rows["AL-03"]["first"][0]["message"])

    def test_a_run_that_ran_no_test_is_errored_and_gives_no_verdict(self):
        record = self.record("error[E0432]: unresolved import\n", 101)
        self.assertEqual(record["state"], "errored")
        self.assertFalse(record["complete"])
        self.assertEqual(self.statuses(record)["SP-01"]["status"], rc.ERRORED)

    def test_the_real_map_names_only_known_rules_and_modules_that_have_tests(self):
        registry = rc.build_registry(rc.RuleBook.load())
        self.assertEqual(registry.problems["unknown"], [])
        self.assertEqual(registry.problems["tests_unmatched"], [])
        self.assertGreater(len(registry.derived["unit-tests"].tests), 50)

    def test_offline_runs_no_unit_tests_and_says_why(self):
        with tempfile.TemporaryDirectory() as out, contextlib.redirect_stdout(io.StringIO()) as printed:
            rc.main(["--offline", "--no-report", "--out", out, "--only", "unit-tests"])
            self.assertFalse((Path(out) / "raw" / "unit-tests.log").exists())
        self.assertIn("not-run", printed.getvalue())


class SplitFindings(unittest.TestCase):
    """Findings are counted by kind; kinds no rule holds are shown, not dropped; the five layout rules cite a rule or say why not."""

    @classmethod
    def setUpClass(cls):
        cls.book = rc.RuleBook.load()
        cls.registry = rc.build_registry(cls.book)

    def test_clipped_overlap_hidden_label_and_horizontal_scroll_hold_tx_02(self):
        for check in ("layout-lint", "overflow-check"):
            keys = self.registry.derived[check].keys
            for key in ("clipped", "overlap", "hidden-label", "horizontal-scroll"):
                self.assertEqual(keys[key], {"TX-02"}, f"{check} {key}")

    def test_outside_parent_is_declared_unmapped_with_a_reason_in_both_checks(self):
        for check in ("layout-lint", "overflow-check"):
            derived = self.registry.derived[check]
            self.assertIn("outside-parent", derived.unmapped)
            self.assertGreater(len(derived.unmapped["outside-parent"]), 40)
            self.assertEqual(self.registry.ids_for(check, "outside-parent"), [])
        self.assertEqual(rc.layout_keys_without_ids(self.registry), [])

    def test_no_layout_rule_of_the_inspector_is_left_without_a_rule_or_a_reason(self):
        path = ROOT / "crates/ui/src/inspector/layout_checks.rs"
        import re
        keys = set(re.findall(r'Rule::\w+\s*=>\s*"([a-z][a-z0-9-]*)"', path.read_text()))
        derived = self.registry.derived["layout-lint"]
        for key in keys:
            self.assertTrue(derived.keys.get(key) or key in derived.unmapped, key)

    def test_overflow_check_maps_each_kind_as_layout_lint_does(self):
        layout, overflow = self.registry.derived["layout-lint"].keys, self.registry.derived["overflow-check"].keys
        for key, ids in overflow.items():
            self.assertEqual(ids, layout[key], key)

    def test_overflow_findings_carry_their_scenario_and_are_counted_by_kind(self):
        check = next(c for c in rc.CHECKS if c.id == "overflow-check")
        rows = [{"rule": "wrapped", "capture": "scale200-taxonomy:a", "what": "w", "target": "t"},
                {"rule": "wrapped", "capture": "scale200-settings-x-phone:p", "what": "w", "target": "t"},
                {"rule": "wrapped", "capture": "pseudo-taxonomy:a", "what": "w", "target": "t"},
                {"rule": "outside-parent", "capture": "pseudo-taxonomy:a", "what": "o", "target": "t"}]
        found = rc.findings_from_json(check, rows, self.registry)
        self.assertEqual([f["scenario"] for f in found], ["scale200", "scale200", "pseudo", "pseudo"])
        self.assertEqual(found[0]["ids"], ["TX-02"])
        self.assertEqual(found[3]["ids"], [])
        self.assertIn("layout-integrity", found[3]["unmapped"])
        run = run_record("overflow-check", "red", found)
        kinds = {(k["key"], k["scenario"]): k for k in rc.findings_by_kind([run])}
        self.assertEqual(kinds[("wrapped", "scale200")]["fail"], 2)
        self.assertEqual(kinds[("wrapped", "pseudo")]["fail"], 1)
        self.assertEqual(kinds[("outside-parent", "pseudo")]["ids"], [])
        unmapped = {u["key"]: u for u in rc.unmapped_report(self.registry, [run]) if u["check"] == "overflow-check"}
        self.assertEqual(unmapped["outside-parent"]["fail"], 1)

if __name__ == "__main__":
    unittest.main()
