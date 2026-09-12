import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "calendar_events", Path(__file__).resolve().parents[1] / "shell/bingux/calendar-events.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class EventsTest(unittest.TestCase):
    def test_overlap_and_exclusive_end(self):
        store = module.EventStore()
        store.update(
            [
                ("s\n1\n", "Previous", 10, 20, {}),
                ("s\n2\n", "Spanning", 10, 40, {}),
                ("s\n3\n", "Instant", 20, 20, {}),
                ("s\n4\n", "Next", 30, 40, {}),
            ]
        )
        self.assertEqual([e["title"] for e in store.within(20, 30)], ["Spanning", "Instant"])

    def test_recurring_replacement_and_removed_source(self):
        store = module.EventStore()
        store.update([("s\nevent\nold", "Old instance", 20, 25, {})])
        store.update([("s\nevent\nnew", "Updated", 22, 25, {}), ("s\nevent\nnext", "Next", 30, 35, {})])
        self.assertEqual(len(store.events), 2)
        self.assertNotIn("s\nevent\nold", store.events)
        store.remove_prefix("s\n")
        self.assertEqual(store.events, {})

    def test_updated_title_and_empty_summary(self):
        store = module.EventStore()
        store.update([("s\nevent\n", "Old", 20, 25, {})])
        store.update([("s\nevent\n", "", 20, 25, {})])
        self.assertEqual(store.within(0, 30)[0]["title"], "Untitled event")


if __name__ == "__main__":
    unittest.main()
