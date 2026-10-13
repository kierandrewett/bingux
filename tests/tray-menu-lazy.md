# Deferred tray menu regression fixture

Run from the Bingux repository:

```sh
BINGUX_QUICKSHELL=/home/kieran/.local/bin/gnoblin-quickshell python3 tests/tray-menu-lazy.py
```

The harness uses an offscreen Qt window and a private D-Bus session. A guarded
timer calls QtTest's `qtest_run()` after the window is shown. QtTest executes
the suite's setup, assertions, and cleanup. The report must name every passing
case and show zero failures and skipped cases; an exit code of zero alone
does not pass the harness.

In its temporary source copy, the harness changes two bindings in `Tray.qml`:
the menu handle comes from the fixture's `testHandle`, and the menu receives
`testActions` as its action rows. It checks both adaptation sites before
launching. `TrayMenu.qml` and `BarTooltip.qml` are replaced with lightweight
components. Stable QObjects emit row and handle changes, and a timer delivers
the delayed rows. QtTest synthesizes mouse and keyboard input against the
real tray delegate and Loader.

The six cases cover initial anchor bindings, cold and prewarmed empty requests
waiting for rows, keyboard activation, hover without opening, retained reopening,
Escape and an outside pointer click while pending, handle identity replacement,
and delegate removal. Successful output ends with:

```text
PASS: 6 tray menu QML cases; 0 failures; 0 skipped
```

This proves the private fixture's assertions. It does not run the real
`QsMenuOpener`, native popup surfaces, or a DBusMenu provider. Native provider
behavior needs the [native tray fixture](tray-menu-native.md) in a disposable
desktop. Measure native process PSS and startup separately; this harness
does not establish a performance gain.
