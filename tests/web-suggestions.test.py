import importlib.util
from pathlib import Path
import threading
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "web_suggestions", Path(__file__).resolve().parents[1] / "packages/bingux-searchd/web-suggestions.py"
)
web = importlib.util.module_from_spec(spec)
spec.loader.exec_module(web)


class SuggestionsTests(unittest.TestCase):
    def test_filters_duplicates_controls_and_bounds(self):
        self.assertEqual(
            web.suggestions(
                ["rust", ["rust", "Rust book", "rust BOOK", "bad\ntext", "x" * 513, "rust tutorial"]], "RUST"
            ),
            ["Rust book", "rust tutorial"],
        )
        self.assertEqual(web.suggestions({}, "rust"), [])
        self.assertEqual(web.suggestions(["rust", ["rust book"]], "rust", 0), [])

    def test_does_not_suggest_for_explicit_private_paths_or_local_filters(self):
        for query in ["/home/person/private", "~/Documents", "? private question", "report ext:pdf", "a"]:
            self.assertFalse(web.eligible(query))
        self.assertTrue(web.eligible("rust programming"))

    def test_discards_inflight_stale_response(self):
        entered, release, completed = threading.Event(), threading.Event(), threading.Event()
        output = []

        def fetch(query):
            if query == "old query":
                entered.set()
                release.wait(2)
            return [query + " suggestion"]

        def emit(record):
            output.append(record)
            completed.set()

        provider = web.Provider(fetcher=fetch, emit=emit)
        threading.Thread(target=provider.run, daemon=True).start()
        provider.submit(dict(queryId="old", query="old query", limit=4))
        self.assertTrue(entered.wait(1))
        provider.submit(dict(queryId="new", query="new query", limit=4))
        release.set()
        self.assertTrue(completed.wait(2))
        self.assertEqual([record["queryId"] for record in output], ["new"])

    def test_activation_uses_selected_term_as_one_encoded_url(self):
        output = []
        provider = web.Provider(opener="/portal-opener", emit=output.append)
        provider.targets["chosen"] = "café & cats"
        with patch.object(web.subprocess, "Popen") as launch:
            provider.activate(dict(activationId="a", resultId="chosen"))
        self.assertEqual(launch.call_args.args[0], ["/portal-opener", "https://duckduckgo.com/?q=caf%C3%A9+%26+cats"])
        self.assertEqual(output[0]["type"], "activated")


if __name__ == "__main__":
    unittest.main()
