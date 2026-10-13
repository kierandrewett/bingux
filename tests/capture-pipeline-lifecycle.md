# Capture pipeline lifecycle check

From the Bingux checkout, run:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tests/capture-pipeline-lifecycle.test.py
```

Install Python 3, PyGObject, GStreamer and its core elements, plus the GTK and
GStreamer introspection dependencies used by the capture backend. This check
does not need a desktop session, portal, encoder or Quickshell. It does not
record the screen, access the clipboard or change settings.

The fixture calls the backend's pipeline startup, message dispatch and cleanup
methods. It substitutes a real, bounded GStreamer pipeline that sends four
bytes from a fake source to a file sink at `/dev/null`.

The five checks cover:

- Six completed jobs: weak notifications must confirm destruction of six
  pipelines and six buses.
- An error followed by another job: both pipelines and buses must be released.
- Cancellation: an old bus must stop calling the backend, and repeated cleanup
  must succeed.
- Failure to enter the playing state: the installed bus watch must be removed.
- A parse failure or unused worker: cleanup must succeed without a bus watch.

To check a particular installed backend, set its absolute path:

```sh
BINGUX_CAPTURE_BACKEND_TEST_SOURCE="$HOME/.local/share/bingux/share/bingux/shell/capture_backend.py" \
PYTHONDONTWRITEBYTECODE=1 python3 tests/capture-pipeline-lifecycle.test.py
```

The default is the checkout's `shell/bingux/capture_backend.py`. Imports resolve
beside the selected backend. The override changes only which file this fixture
tests; it does not affect the running capture service.

A passing run proves that these bus watches and pipelines are released. It does
not prove real recording, encoder or PipeWire buffer lifetime, measure GPU
memory, or identify the cause of a historical out-of-memory incident. Use the
live checks in [Capture](../docs/capture.md#verify-capture) for recording and
screenshot behavior.
