#!/usr/bin/env python3
"""Exercise installed bingux-searchd with isolated lazy and eager providers."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


TIMEOUT = 5
DEFAULT_BINARY = Path(
    os.environ.get("BINGUX_SEARCHD_BIN", os.path.expanduser("~/.local/share/bingux/bin/bingux-searchd"))
)


def write_json(path, value):
    path.write_text(json.dumps(value) + "\n")


def wait_for(predicate, description):
    deadline = time.monotonic() + TIMEOUT
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.01)
    raise AssertionError(f"timed out waiting for {description}")


def provider_script(path, eager):
    if eager:
        path.write_text(
            "#!/usr/bin/env python3\n"
            "import os\n"
            "from pathlib import Path\n"
            "with Path(os.environ['FIXTURE_ATTEMPTS']).open('a') as output:\n"
            "    output.write('attempt\\n')\n"
        )
    else:
        path.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "for line in sys.stdin:\n"
            "    request = json.loads(line)\n"
            "    if request['type'] == 'hello':\n"
            "        Path(os.environ['FIXTURE_PROVIDER_PID']).write_text(f'{os.getpid()} {Path(f\"/proc/{os.getpid()}/stat\").read_text().split()[21]}\\n')\n"
            "        response = {'protocolVersion': 1, 'type': 'hello', 'accepted': True}\n"
            "    elif request['type'] == 'query':\n"
            "        response = {'protocolVersion': 1, 'type': 'results', 'queryId': request['queryId'], 'complete': True, 'results': [{'resultId': 'fixture-result', 'kind': 'action', 'title': 'Fixture result', 'subtitle': request['query'], 'icon': 'system-search', 'score': 0.9}]}\n"
            "    elif request['type'] == 'activate':\n"
            "        response = {'protocolVersion': 1, 'type': 'activated', 'activationId': request['activationId']}\n"
            "    else:\n"
            "        continue\n"
            "    print(json.dumps(response), flush=True)\n"
        )
    path.chmod(0o700)


def config_for(manifest):
    return {
        "protocolVersion": 1,
        "commands": {
            "applicationLauncher": ["/usr/bin/true"],
            "fileOpener": ["/usr/bin/true"],
            "clipboard": ["/usr/bin/true"],
        },
        "disabledProviders": ["applications", "files"],
        "fileRoots": [],
        "providerManifestPaths": [str(manifest)],
    }


def provider_manifest(script, startup):
    return {
        "kind": "bingux.search-provider",
        "protocolVersion": 1,
        "id": "fixture-provider",
        "displayName": "Fixture provider",
        "command": [str(script)],
        "startup": startup,
        "priority": 100,
        "timeoutMs": 200,
    }


def start_daemon(binary, base, config, extra_env, log):
    runtime = base / "runtime"
    runtime.mkdir(mode=0o700)
    config_path = base / "search.json"
    write_json(config_path, config)
    environment = (
        os.environ
        | {
            "HOME": str(base / "home"),
            "XDG_CONFIG_HOME": str(base / "config"),
            "XDG_RUNTIME_DIR": str(runtime),
        }
        | extra_env
    )
    (base / "home").mkdir()
    (base / "config").mkdir()
    daemon = subprocess.Popen(
        [str(binary), "--config", str(config_path)],
        env=environment,
        stdout=subprocess.DEVNULL,
        stderr=log,
        text=True,
    )
    address = runtime / "bingux/search-v1.sock"
    try:
        wait_for(lambda: address.exists() or daemon.poll() is not None, "search socket")
        if daemon.poll() is not None:
            raise AssertionError(f"daemon exited with {daemon.returncode}")
    except BaseException:
        stop_daemon(daemon)
        raise
    return daemon, address


def stop_daemon(daemon):
    if daemon.poll() is None:
        daemon.terminate()
        try:
            daemon.wait(timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            daemon.kill()
            daemon.wait(timeout=TIMEOUT)


def provider_switches(pid):
    values = []
    for task in (Path("/proc") / str(pid) / "task").iterdir():
        if (task / "comm").read_text().strip() != "bingux-search-p":
            continue
        status = (task / "status").read_text().splitlines()
        values.append(int(next(line.split()[1] for line in status if line.startswith("voluntary_ctxt_switches:"))))
    return values


def process_identity(path):
    try:
        return Path(f"/proc/{path}/stat").read_text().split()[21]
    except FileNotFoundError:
        return None


def provider_alive(marker):
    pid, start_time = marker.read_text().split()
    return process_identity(pid) == start_time


def read_until(connection, buffered, request_id, predicate):
    deadline = time.monotonic() + TIMEOUT
    while time.monotonic() < deadline:
        try:
            chunk = connection.recv(65536)
        except TimeoutError:
            continue
        if not chunk:
            raise AssertionError("daemon closed the query socket")
        buffered.extend(chunk)
        while b"\n" in buffered:
            line, _, remainder = buffered.partition(b"\n")
            buffered[:] = remainder
            record = json.loads(line)
            if record.get("requestId") == request_id and predicate(record):
                return record
    raise AssertionError(f"timed out waiting for {request_id}")


def lazy_lifecycle(binary, base, log):
    base.mkdir()
    script = base / "lazy-provider.py"
    manifest = base / "lazy-provider.json"
    marker = base / "lazy-provider.pid"
    provider_script(script, eager=False)
    write_json(manifest, provider_manifest(script, "lazy"))
    daemon = None
    connection = None
    try:
        daemon, address = start_daemon(binary, base, config_for(manifest), {"FIXTURE_PROVIDER_PID": str(marker)}, log)
        before = provider_switches(daemon.pid)
        time.sleep(0.3)
        after = provider_switches(daemon.pid)
        assert len(before) == len(after) == 2, (before, after)
        assert all(current <= previous + 1 for previous, current in zip(before, after)), (before, after)

        connection = socket.socket(socket.AF_UNIX)
        connection.settimeout(0.1)
        connection.connect(str(address))
        buffered = bytearray()
        connection.sendall(b'{"protocolVersion":1,"type":"query","requestId":"query","query":"fixture","limit":20}\n')
        results = []
        while True:
            record = read_until(connection, buffered, "query", lambda event: "results" in event)
            results.extend(record["results"])
            if record["complete"]:
                break
        result = next(item for item in results if item["providerId"] == "fixture-provider")
        assert result["title"] == "Fixture result", result
        connection.sendall(
            (
                json.dumps(
                    {"protocolVersion": 1, "type": "activate", "requestId": "activate", "resultId": result["resultId"]}
                )
                + "\n"
            ).encode()
        )
        read_until(connection, buffered, "activate", lambda event: event["type"] == "activated")
    finally:
        if connection is not None:
            connection.close()
        if daemon is not None:
            stop_daemon(daemon)
        if marker.exists():
            wait_for(lambda: not provider_alive(marker), "lazy provider shutdown")


def eager_retries(binary, base, log):
    base.mkdir()
    script = base / "eager-provider.py"
    manifest = base / "eager-provider.json"
    attempts = base / "attempts"
    provider_script(script, eager=True)
    write_json(manifest, provider_manifest(script, "eager"))
    daemon = None
    try:
        daemon, _address = start_daemon(binary, base, config_for(manifest), {"FIXTURE_ATTEMPTS": str(attempts)}, log)
        wait_for(lambda: attempts.exists() and len(attempts.read_text().splitlines()) >= 2, "eager provider retry")
    finally:
        if daemon is not None:
            stop_daemon(daemon)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--log", type=Path, default=Path("/var/tmp/bingux-external-provider-lifecycle.log"))
    args = parser.parse_args()
    assert args.binary.is_file() and os.access(args.binary, os.X_OK), args.binary
    args.log.parent.mkdir(parents=True, exist_ok=True)
    with (
        args.log.open("w", encoding="utf-8") as log,
        tempfile.TemporaryDirectory(prefix="bingux-external-provider-") as directory,
    ):
        binary_hash = hashlib.sha256(args.binary.read_bytes()).hexdigest()
        log.write(f"binary={args.binary} sha256={binary_hash}\n")
        log.flush()
        base = Path(directory)
        lazy_lifecycle(args.binary, base / "lazy", log)
        eager_retries(args.binary, base / "eager", log)
    print(f"EXTERNAL_PROVIDER_LIFECYCLE_PASS sha256={binary_hash} log={args.log}")


if __name__ == "__main__":
    main()
