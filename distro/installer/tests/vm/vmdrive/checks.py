"""Pass/fail records for a driver run, saved as results.json in the artifact directory."""

import json
from pathlib import Path


class Checks:
    def __init__(self, artifacts):
        self.path = Path(artifacts) / "results.json"
        self.results = []

    def expect(self, condition, message, detail=""):
        """Record one check and print it. Returns the boolean so callers can branch on it."""
        passed = bool(condition)
        self.results.append({"check": message, "pass": passed, "detail": str(detail)})
        print(f"{'PASS' if passed else 'FAIL'} {message}" + (f": {detail}" if detail and not passed else ""))
        return passed

    @property
    def ok(self):
        return bool(self.results) and all(item["pass"] for item in self.results)

    def save(self):
        failed = sum(1 for item in self.results if not item["pass"])
        self.path.write_text(json.dumps({
            "ok": self.ok, "passed": len(self.results) - failed, "failed": failed, "checks": self.results,
        }, indent=2) + "\n", encoding="utf-8")
