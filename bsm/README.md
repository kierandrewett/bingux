# bsm: the Bingux Session Manager

One persistent system process that owns the display and the seat, shows the
greeter and hosts one Gnoblin per signed-in user. Start with
`docs/bsm/architecture.md`. Each directory is one crate.

| Directory | Cargo package | Role |
| --------- | ------------- | ---- |
| `core/` | `bsm` | The `bsm` program and the `bsmctl` command line client. Holds the wire protocol XML in `protocol/`. |
| `manager/` | `bsm-manager` | The session manager core and its I/O runtime. |
| `compositor/` | `bsm-compositor` | The compositor core. |
| `host/` | `bsm-host` | The bridge between bsm and one hosted Gnoblin. |
| `helper/` | `bsm-helper` | The privileged PAM and session helper. |
| `appearance/` | `bsm-appearance` | Account discovery and greeter appearance publishing. |
| `proto/` | `bsm-proto` | Shared types and wire formats. |
