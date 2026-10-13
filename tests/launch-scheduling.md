# Launch scheduling checks

Run these bounded checks without a compositor or VM:

```sh
python3 tests/app-launch-priority.py
python3 tests/search-reaper-idle.py
```

The launch fixture uses private desktop entries and an unclassified C child.
It records actual nice and scheduler policy for ordinary, desktop-action and
manager-rejection fallback launches, and preserves positive nice. To require
the negative-nice regression, run it from an already boosted parent with
`--require-boost`. Exit 77 means that boost was unavailable, not a pass.

The reaper fixture compiles the production reaper with Rust's standard library.
It checks idle task wakeups, new-child delivery, reaping and shutdown with a
live child. It proves this worker's lifecycle, not desktop frame pacing.
