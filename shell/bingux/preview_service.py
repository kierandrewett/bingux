"""Persistent preview cache with cancellable foreground and background workers."""
import asyncio
from collections import OrderedDict
import json
import math
import os
import signal
from pathlib import Path
import sys
from preview_limits import check_file

CACHE_BYTES = 64 * 1024 * 1024
WARM_WIDTH = 1024


def identity(filename):
    path = Path(filename)
    stat = check_file(path)
    wal = Path(filename + '-wal')
    wal_stat = wal.stat() if wal.is_file() else None
    return (filename, stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns,
            (wal_stat.st_size, wal_stat.st_mtime_ns) if wal_stat else None)


def request_key(args):
    mode, filename, *options = args
    if mode not in ('info', 'page', 'table'):
        raise ValueError('Unknown preview request.')
    if mode == 'page':
        options = [int(options[0]), min(2400, max(128, math.ceil(int(options[1]) / 128) * 128))]
    return (identity(filename), mode, tuple(options))


class Cache:
    def __init__(self, limit=CACHE_BYTES):
        self.entries = OrderedDict()
        self.bytes = 0
        self.limit = limit

    def get(self, key):
        candidate = key
        if candidate not in self.entries and key[1] == 'page':
            sizes = [other for other in self.entries if other[:2] == key[:2]
                     and other[2][0] == key[2][0] and other[2][1] >= key[2][1]]
            if sizes:
                candidate = min(sizes, key=lambda other: other[2][1])
        if candidate not in self.entries:
            return None
        self.entries.move_to_end(candidate)
        return self.entries[candidate][0]

    def put(self, key, value):
        if 'error' in value:
            return
        size = len(json.dumps(value).encode())
        if size > self.limit:
            return
        if key in self.entries:
            self.bytes -= self.entries.pop(key)[1]
        while self.entries and self.bytes + size > self.limit:
            self.bytes -= self.entries.popitem(last=False)[1][1]
        self.entries[key] = (value, size)
        self.bytes += size


class Service:
    def __init__(self, command, emit):
        self.command = command
        self.emit = emit
        self.cache = Cache()
        self.requests = {}
        self.jobs = {}
        self.foreground = asyncio.Semaphore(2)
        self.warm_paths = []
        self.warm_task = None
        self.warm_path = None
        self.warmed = OrderedDict()
        self.closed = False

    async def run(self, args, background=False):
        process = await asyncio.create_subprocess_exec(*self.command, *map(str, args),
            stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL,
            preexec_fn=(lambda: os.nice(10)) if background else None)
        try:
            output, _ = await asyncio.wait_for(process.communicate(), timeout=28)
            return json.loads(output)
        except asyncio.TimeoutError:
            return {'error': 'This file took too long to prepare for preview.'}
        finally:
            if process.returncode is None:
                process.terminate()
                try:
                    await asyncio.wait_for(process.wait(), timeout=1)
                except asyncio.TimeoutError:
                    process.kill()
                    await process.wait()

    async def fetch(self, key):
        filename = key[0][0]
        if self.warm_task and not self.warm_task.done():
            if self.warm_path == filename:
                # Join an already running conversion instead of duplicating it.
                try:
                    await asyncio.shield(self.warm_task)
                except asyncio.CancelledError:
                    if asyncio.current_task().cancelling():
                        raise
                cached = self.cache.get(key)
                if cached is not None:
                    return cached
            else:
                self.warm_task.cancel()
        async with self.foreground:
            result = await self.run([key[1], filename, *key[2]])
        if identity(filename) != key[0]:
            return {'error': 'The file changed while its preview was loading. Select it again.'}
        self.cache.put(key, result)
        return result

    async def request(self, request_id, args):
        try:
            key = request_key(args)
            result = self.cache.get(key)
            if result is None:
                job = self.jobs.get(key)
                if job is None:
                    job = asyncio.create_task(self.fetch(key))
                    self.jobs[key] = job
                self.requests[request_id] = (key, asyncio.current_task())
                result = await asyncio.shield(job)
            self.emit({'id': request_id, 'data': result})
        except asyncio.CancelledError:
            pass
        except (ValueError, OSError, IndexError, TypeError) as error:
            self.emit({'id': request_id, 'data': {'error': str(error)}})
        finally:
            self.requests.pop(request_id, None)
            for key, job in list(self.jobs.items()):
                if not any(entry[0] == key for entry in self.requests.values()):
                    if not job.done():
                        job.cancel()
                    self.jobs.pop(key, None)
            self.start_warming()

    def cancel(self, request_id):
        entry = self.requests.get(request_id)
        if entry:
            entry[1].cancel()

    def preload(self, paths):
        self.warm_paths = list(dict.fromkeys(path for path in paths if isinstance(path, str) and path.startswith('/')))[:6]
        # Revisit evicted entries only when navigation changes the preload plan;
        # continuously trying to fit every candidate would thrash a full cache.
        for stamp, successful in list(self.warmed.items()):
            if successful and stamp[0] in self.warm_paths and self.cache.get((stamp, 'info', ())) is None:
                del self.warmed[stamp]
        if self.warm_task and self.warm_path not in self.warm_paths:
            # A visible request that joined this warm job still needs its result.
            if not any(entry[0][0][0] == self.warm_path for entry in self.requests.values()):
                self.warm_task.cancel()
        self.start_warming()

    def start_warming(self):
        if self.closed or self.requests or (self.warm_task and not self.warm_task.done()):
            return
        for filename in self.warm_paths:
            try:
                stamp = identity(filename)
            except (ValueError, OSError):
                continue
            if stamp in self.warmed:
                continue
            self.warm_path = filename
            self.warm_task = asyncio.create_task(self.warm(filename, stamp))
            self.warm_task.add_done_callback(lambda _: self.start_warming())
            break

    async def warm(self, filename, stamp):
        try:
            result = await self.run(['warm', filename, str(WARM_WIDTH)], background=True)
            if identity(filename) != stamp:
                return
            if result.get('info'):
                self.cache.put((stamp, 'info', ()), result['info'])
                for page, data in enumerate(result.get('pages', [])):
                    self.cache.put((stamp, 'page', (page, WARM_WIDTH)), data)
                self.emit({'preloaded': filename})
            self.warmed[stamp] = bool(result.get('info'))
            while len(self.warmed) > 64:
                self.warmed.popitem(last=False)
        except asyncio.CancelledError:
            pass
        except (ValueError, OSError):
            self.warmed[stamp] = False

    async def close(self):
        self.closed = True
        tasks = [entry[1] for entry in self.requests.values()] + list(self.jobs.values())
        if self.warm_task:
            tasks.append(self.warm_task)
        for task in tasks:
            task.cancel()
        await asyncio.gather(*tasks, return_exceptions=True)


async def serve(command):
    def emit(record):
        print(json.dumps(record), flush=True)
    service = Service(command, emit)
    current = asyncio.current_task()
    for signum in (signal.SIGTERM, signal.SIGINT):
        asyncio.get_running_loop().add_signal_handler(signum, current.cancel)
    reader = asyncio.StreamReader(limit=65536)
    protocol = asyncio.StreamReaderProtocol(reader)
    await asyncio.get_running_loop().connect_read_pipe(lambda: protocol, sys.stdin)
    emit({'ready': True})
    try:
        while line := await reader.readline():
            try:
                request = json.loads(line)
                if request.get('op') == 'preload':
                    service.preload(request.get('paths', []))
                elif request.get('op') == 'cancel':
                    service.cancel(request.get('id'))
                elif request.get('op') == 'request':
                    asyncio.create_task(service.request(request['id'], request['args']))
            except (ValueError, KeyError, TypeError):
                continue
    except asyncio.CancelledError:
        pass
    finally:
        await service.close()
