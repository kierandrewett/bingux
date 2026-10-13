# Control Centre service lifecycle fixture

Run the helper lifecycle check after changing
`shell/bingux/control-centre-services.py`:

```sh
python3 tests/control-centre-services-lifecycle.test.py
```

Set `BINGUX_CONTROLS_SOURCE` to test an installed helper instead of the source
copy. The fixture starts that helper through its JSON protocol, replaces only
its expensive snapshot read with a counter, and uses GSettings' private memory
backend.

It checks that inactive startup and DND changes do not probe VPN or
power services, while activation and an active DND action do. A same-process
GSettings update also proves the DND change signal remains live without
changing host settings.
