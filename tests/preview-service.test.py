import asyncio
import json
import os
import sys
from pathlib import Path
import tempfile
import unittest

SHELL = Path(__file__).resolve().parents[1] / "shell/bingux"
sys.path.insert(0, str(SHELL))
import preview_service as service  # noqa: E402 - The shell path must be added first.
from preview_limits import MAX_FILE_BYTES  # noqa: E402 - The shell path must be added first.


class FakeService(service.Service):
    def __init__(self, output):
        super().__init__([], output.append)
        self.calls = []
        self.cancelled = []

    async def run(self, args, background=False):
        self.calls.append((args, background))
        try:
            await asyncio.sleep(0.04 if background else 0.01)
        except asyncio.CancelledError:
            self.cancelled.append(args)
            raise
        text = Path(args[1]).read_text()
        if args[0] == "warm":
            return {"info": {"kind": "text", "text": text}, "pages": [{"image": "prepared page"}]}
        return {"kind": "text", "text": text}


class PreviewServiceTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.path = self.directory / "file.txt"
        self.path.write_text("original")
        self.output = []
        self.service = FakeService(self.output)
        self.addAsyncCleanup(self.service.close)

    async def test_prepared_info_and_pages_skip_foreground_work(self):
        self.service.preload([str(self.path)])
        await self.service.warm_task
        await self.service.request("info", ["info", str(self.path)])
        await self.service.request("page", ["page", str(self.path), "0", "932"])
        self.assertEqual(len(self.service.calls), 1)
        self.assertEqual(self.output[-1]["data"]["image"], "prepared page")

    async def test_changed_file_invalidates_cached_results(self):
        await self.service.request("old", ["info", str(self.path)])
        self.path.write_text("changed")
        await self.service.request("new", ["info", str(self.path)])
        self.assertEqual(self.output[-1]["data"]["text"], "changed")
        self.assertEqual(len(self.service.calls), 2)

    async def test_foreground_preempts_other_background_work(self):
        other = self.directory / "other.txt"
        other.write_text("other")
        self.service.preload([str(other)])
        await asyncio.sleep(0.005)
        await self.service.request("selected", ["info", str(self.path)])
        self.assertTrue(self.service.cancelled)
        self.assertEqual(self.output[-1]["data"]["text"], "original")

    async def test_request_joins_matching_background_conversion(self):
        self.service.preload([str(self.path)])
        await asyncio.sleep(0.005)
        await self.service.request("selected", ["info", str(self.path)])
        self.assertEqual(len(self.service.calls), 1)
        self.assertEqual(self.output[-1]["data"]["text"], "original")

    async def test_duplicate_requests_share_work_and_cancelled_requests_stay_silent(self):
        first = asyncio.create_task(self.service.request("first", ["info", str(self.path)]))
        second = asyncio.create_task(self.service.request("second", ["info", str(self.path)]))
        await asyncio.sleep(0.005)
        self.service.cancel("first")
        await asyncio.gather(first, second)
        self.assertEqual(len(self.service.calls), 1)
        self.assertEqual([item["id"] for item in self.output if "id" in item], ["second"])

    async def test_large_files_are_rejected_before_any_worker_starts(self):
        for suffix in [".mp4", ".sqlite", ".pdf", ".gif", ".docx"]:
            path = self.directory / ("large" + suffix)
            with path.open("wb") as stream:
                stream.truncate(MAX_FILE_BYTES + 1)
            self.service.preload([str(path)])
            await self.service.request(suffix, ["info", str(path)])
            self.assertIn("20 MB limit", self.output[-1]["data"]["error"])
        self.assertEqual(self.service.calls, [])

    async def test_cache_stays_bounded(self):
        cache = service.Cache(limit=60)
        cache.put(("one", "info", ()), {"text": "x" * 20})
        cache.put(("two", "info", ()), {"text": "x" * 20})
        self.assertLessEqual(cache.bytes, 60)
        self.assertIsNone(cache.get(("one", "info", ())))
        self.assertIsNotNone(cache.get(("two", "info", ())))

    async def test_full_cache_does_not_cause_background_thrashing(self):
        self.service.cache = service.Cache(limit=1)
        self.service.preload([str(self.path)])
        await asyncio.sleep(0.13)
        self.assertEqual(len(self.service.calls), 1)

    async def test_real_service_prepares_and_serves_pages_over_its_pipe(self):
        import cairo

        path = self.directory / "real.pdf"
        surface = cairo.PDFSurface(str(path), 210, 297)
        context = cairo.Context(surface)
        context.set_source_rgb(0.2, 0.4, 0.8)
        context.paint()
        surface.finish()
        process = await asyncio.create_subprocess_exec(
            sys.executable,
            str(SHELL / "preview-document.py"),
            "serve",
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            limit=16 * 1024 * 1024,
            env=dict(os.environ, XDG_CACHE_HOME=str(self.directory / "cache")),
        )
        try:
            self.assertTrue(json.loads(await process.stdout.readline())["ready"])
            process.stdin.write((json.dumps({"op": "preload", "paths": [str(path)]}) + "\n").encode())
            record = json.loads(await asyncio.wait_for(process.stdout.readline(), 5))
            self.assertEqual(record["preloaded"], str(path))
            for mode, options in [("info", []), ("page", ["0", "932"])]:
                process.stdin.write(
                    (json.dumps({"op": "request", "id": mode, "args": [mode, str(path), *options]}) + "\n").encode()
                )
                record = json.loads(await asyncio.wait_for(process.stdout.readline(), 5))
                self.assertEqual(record["id"], mode)
                self.assertNotIn("error", record["data"])
                if mode == "page":
                    self.assertTrue(record["data"]["image"].startswith("data:image/png;base64,"))
        finally:
            process.stdin.close()
            await asyncio.wait_for(process.wait(), 5)


if __name__ == "__main__":
    unittest.main()
