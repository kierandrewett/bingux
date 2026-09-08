#!/usr/bin/env python3
"""Optional DuckDuckGo autocomplete provider; stdout is protocol JSON only."""
import hashlib
import json
import subprocess
import sys
import threading
import time
import urllib.parse
import urllib.request


def suggestions(payload, query, limit=4):
    if not isinstance(payload, list) or len(payload) < 2 or not isinstance(payload[1], list):
        return []
    seen = {query.strip().casefold()}
    result = []
    for value in payload[1]:
        if not isinstance(value, str):
            continue
        value = value.strip()
        if not value or len(value.encode()) > 512 or any(ord(char) < 32 or 127 <= ord(char) < 160 for char in value):
            continue
        if value.casefold() in seen:
            continue
        seen.add(value.casefold())
        result.append(value)
        if len(result) >= limit:
            break
    return result if limit > 0 else []


def eligible(query):
    # Don't send explicit local paths, assistant prompts or local filters.
    return (2 <= len(query) <= 200 and not query.startswith(("/", "~", "?"))
            and not any(term.lower().startswith(("ext:", "filetype:", "path:")) for term in query.split()))


def fetch(query):
    url = "https://duckduckgo.com/ac/?" + urllib.parse.urlencode({"q": query, "type": "list"})
    with urllib.request.urlopen(url, timeout=0.8) as response:
        data = response.read(32769)
    if len(data) > 32768:
        return []
    return suggestions(json.loads(data), query)


class Provider:
    def __init__(self, opener="xdg-open", fetcher=fetch, emit=None):
        self.opener = opener
        self.fetcher = fetcher
        self.emit = emit or self.write
        self.condition = threading.Condition()
        self.pending = None
        self.generation = 0
        self.targets = {}
        self.cache = {}
        self.output_lock = threading.Lock()

    def write(self, record):
        with self.output_lock:
            print(json.dumps({"protocolVersion": 1, **record}), flush=True)

    def submit(self, request):
        with self.condition:
            self.generation += 1
            self.pending = (self.generation, time.monotonic(), request)
            self.condition.notify()

    def run(self):
        while True:
            with self.condition:
                while self.pending is None:
                    self.condition.wait()
                generation, started, request = self.pending
                remaining = 0.2 - (time.monotonic() - started)
                if remaining > 0:
                    self.condition.wait(remaining)
                    continue
                self.pending = None
            query = request.get("query", "").strip()
            values = []
            if eligible(query):
                cached = self.cache.get(query)
                if cached and time.monotonic() - cached[0] < 300:
                    values = cached[1]
                else:
                    try:
                        values = self.fetcher(query)
                    except Exception:
                        values = []  # Network failure must not break local search.
                    if len(self.cache) >= 64:
                        self.cache.pop(next(iter(self.cache)))
                    self.cache[query] = (time.monotonic(), values)
            with self.condition:
                if generation != self.generation:
                    continue
                results = []
                for index, value in enumerate(values[:min(4, request.get("limit", 4))]):
                    result_id = hashlib.sha256(value.encode()).hexdigest()[:24]
                    self.targets[result_id] = value
                    results.append(dict(resultId=result_id, kind="action", title=value,
                                        subtitle="DuckDuckGo", icon="duckduckgo", score=0.09-index*0.005))
                while len(self.targets) > 128:
                    self.targets.pop(next(iter(self.targets)))
                self.emit(dict(type="results", queryId=request["queryId"], complete=True, results=results))

    def activate(self, request):
        with self.condition:
            value = self.targets.get(request.get("resultId"))
        if value is None:
            self.emit(dict(type="error", activationId=request["activationId"], code="invalid-request", message="Suggestion expired"))
            return
        try:
            subprocess.Popen([self.opener, "https://duckduckgo.com/?" + urllib.parse.urlencode({"q": value})],
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError:
            self.emit(dict(type="error", activationId=request["activationId"], code="unavailable", message="Could not open browser"))
            return
        self.emit(dict(type="activated", activationId=request["activationId"]))


def main():
    provider = Provider(sys.argv[1] if len(sys.argv) > 1 else "xdg-open")
    threading.Thread(target=provider.run, daemon=True).start()
    for line in sys.stdin:
        try:
            request = json.loads(line)
            if request.get("protocolVersion") != 1:
                continue
            if request.get("type") == "hello":
                provider.emit(dict(type="hello", accepted=True))
            elif request.get("type") == "query":
                provider.submit(request)
            elif request.get("type") == "activate":
                provider.activate(request)
        except (ValueError, KeyError, TypeError):
            continue


if __name__ == "__main__":
    main()
