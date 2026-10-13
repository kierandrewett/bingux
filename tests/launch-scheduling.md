# Launch scheduling checks

Run these bounded checks without a compositor or VM:

```sh
python3 tests/app-launch-priority.py
python3 tests/direct-app-launch.py
python3 tests/dock-desktop-action.py
python3 tests/search-reaper-idle.py
```

## Desktop entries

The launch fixture uses private desktop entries and an unclassified C child.
It records actual nice and scheduler policy for ordinary, desktop-action and
manager-rejection fallback launches, and preserves positive nice. To require
the negative-nice regression, run it from an already boosted parent with
`--require-boost`. Exit 77 means that boost was unavailable, not a pass.

The dock-action fixture also needs Node.js, PyGObject, a C compiler and
`dbus-run-session`. It tests the production menu handler with an action ID
that differs from its translated label and begins with a dash. The helper's
`--action=ID` selects an ID from the desktop entry's `Actions` list;
`--new-window` remains a separate, mutually exclusive option.

Private desktop entries verify GIO field codes, working directory and
manager-rejection fallback. A private D-Bus service records `ActivateAction`.
Invalid actions must fail without running the default command. No host
session bus, application or user-manager unit is used.

## Calendar and Settings

The direct-launch fixture needs Python with PyGObject, Node.js and a C compiler.
It executes the JavaScript handlers from `ApplicationLauncher.qml` and
`ControlCentre.qml`, then runs their argument vectors against private C
executables. It also runs `calendar-launch.py` with its D-Bus locale and
readiness calls stubbed. No real application, display or user-manager unit
is used.

It covers Settings, the Calendar header, prepare/follow-up commands and the
event helper's date/UUID commands. A simulated syscall boundary makes negative,
zero, positive and denied priority resets deterministic. Separate child
records show actual kernel nice and scheduler policy.

The fixture also checks literal arguments, environment, working directory,
actual positive nice and a missing executable. `--output DIRECTORY` saves the
records and source hashes.

## VM verification

For kernel inheritance proof, run the direct-launch fixture in the isolated
VM from a parent already at negative nice:

```sh
python3 tests/direct-app-launch.py --require-boost --output /tmp/direct-launch-proof
```

Require exit 0 and real-case child nice 0 with the parent's scheduler policy. Exit 77
means that negative nice was unavailable. External scheduling daemons can
change priority after launch; retain the raw records when comparing runs.

After installing the changed helper and QML in the VM, use the Calendar
header, open an event, and open a Settings panel from the control centre.
Start with both applications closed so activation cannot reuse old processes.
Check the new application PIDs' nice and scheduler policy, the selected date
and event, and the Settings panel.

Also choose a named action from a running
application's dock menu and verify the requested action, including its
activation of an existing application where supported.

Keep this interactive proof separate from the fixture, which substitutes the
QML execution and D-Bus boundaries.

## Search child reaper

The reaper fixture compiles the production reaper with Rust's standard library.
It checks idle task wakeups, new-child delivery, reaping and shutdown with a
live child. It proves this worker's lifecycle, not desktop frame pacing.
