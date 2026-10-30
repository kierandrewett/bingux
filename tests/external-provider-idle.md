# Check idle search providers

Run these checks after changing the external provider worker loop:

```sh
cargo test --locked --manifest-path services/searchd/Cargo.toml external::tests::
python3 tests/external-provider-lifecycle.py --binary ~/.local/share/bingux/bin/bingux-searchd
```

The Linux idle-worker check samples voluntary thread context switches before
sending shutdown. The command lifecycle check wakes an unused lazy worker with
a query, an activation, and shutdown. The deadline check verifies that pending
requests still expire while the provider is unavailable.

The installed-daemon fixture uses private runtime and configuration directories.
It checks successful lazy-provider queries and activation, idle waiting,
eager-provider retries, and process cleanup without changing desktop services.

In a disposable desktop session, start the installed search daemon with a
configuration containing an unused lazy provider. Record the running binary's
hash and that provider thread's context switches, then issue a search through
the shell. Idle providers should wait for commands. Providers with live child
processes retain timed polling for responses, failures and deadlines.

Linux truncates thread names to 15 characters. Identify the provider worker
before sampling; its name may collide with the program reaper's name.
