import importlib.util
from pathlib import Path
import subprocess
import time
import unittest

spec = importlib.util.spec_from_file_location("actions", Path(__file__).resolve().parents[1] / "shell/bingux/process-action.py")
actions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(actions)

class ProcessActions(unittest.TestCase):
    def test_signals_and_reused_pid_guard(self):
        child = subprocess.Popen(["sleep", "30"])
        try:
            def stat():
                return Path(f"/proc/{child.pid}/stat").read_text().rsplit(") ", 1)[1].split()
            start = int(stat()[19])
            with self.assertRaises(ValueError):
                actions.act(child.pid, start + 1, "kill")
            self.assertIsNone(child.poll())
            actions.act(child.pid, start, "pause")
            for _ in range(100):
                if stat()[0] == "T": break
                time.sleep(.01)
            self.assertEqual(stat()[0], "T")
            actions.act(child.pid, start, "resume")
            for _ in range(100):
                if stat()[0] != "T": break
                time.sleep(.01)
            self.assertNotEqual(stat()[0], "T")
            actions.act(child.pid, start, "end")
            self.assertEqual(child.wait(timeout=3), -15)
        finally:
            if child.poll() is None: child.kill()
            child.wait()

    def test_batch_continues_after_stale_selection(self):
        children = [subprocess.Popen(["sleep", "30"]) for _ in range(2)]
        try:
            selections = [{"pid": child.pid, "startTime": int(Path(f"/proc/{child.pid}/stat").read_text().rsplit(") ", 1)[1].split()[19])} for child in children]
            self.assertEqual(actions.act_batch(selections, "pause"), "Paused 2 processes")
            self.assertEqual(actions.act_batch(selections, "resume"), "Resumed 2 processes")
            stale = dict(selections[0], startTime=selections[0]["startTime"] + 1)
            result = actions.act_batch([stale, selections[1]], "end")
            self.assertIn("1 failed", result)
            self.assertEqual(children[1].wait(timeout=3), -15)
            self.assertIsNone(children[0].poll())
        finally:
            for child in children:
                if child.poll() is None: child.kill()
                child.wait()

    def test_force_quit(self):
        child = subprocess.Popen(["sleep", "30"])
        try:
            start = int(Path(f"/proc/{child.pid}/stat").read_text().rsplit(") ", 1)[1].split()[19])
            actions.act(child.pid, start, "kill")
            self.assertEqual(child.wait(timeout=3), -9)
        finally:
            if child.poll() is None: child.kill()
            child.wait()

if __name__ == "__main__": unittest.main()
