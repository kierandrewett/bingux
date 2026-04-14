# Permissions Model

Bingux uses a runtime permission system inspired by macOS TCC/Gatekeeper.
Nothing is granted by default -- every privileged operation either
triggers a user prompt or is silently denied if previously refused.

---

## Capability Permissions

Capabilities control access to hardware, network, display, IPC, and
dangerous system operations.

### Hardware & Devices

| Capability   | Controls                          |
|--------------|-----------------------------------|
| `gpu`        | `/dev/dri/*` access               |
| `audio`      | PipeWire / PulseAudio socket      |
| `camera`     | `/dev/video*`                     |
| `input`      | `/dev/input/*` (raw input)        |
| `usb`        | `/dev/bus/usb/*`                  |
| `bluetooth`  | Bluetooth stack                   |

### Network

| Capability       | Controls                       |
|------------------|--------------------------------|
| `net:outbound`   | Outgoing connections           |
| `net:listen`     | Bind and listen on ports       |
| `net:port=<N>`   | Bind a specific port           |

### Display & Session

| Capability       | Controls                       |
|------------------|--------------------------------|
| `display`        | Wayland / X11 socket           |
| `notifications`  | Desktop notifications          |
| `clipboard`      | System clipboard read/write    |
| `screenshot`     | Screen capture                 |

### IPC & System

| Capability       | Controls                       |
|------------------|--------------------------------|
| `dbus:session`   | Session D-Bus                  |
| `dbus:system`    | System D-Bus                   |
| `process:exec`   | Execute binaries outside own package |
| `process:ptrace` | Attach to other processes      |
| `keyring`        | System keyring / secrets       |

### Dangerous (always prompt, no "Always Allow")

| Capability       | Controls                       |
|------------------|--------------------------------|
| `root`           | Request root privileges        |
| `kernel:module`  | Load kernel modules            |
| `raw:net`        | Raw network sockets            |

---

## Mount Permissions

Mount permissions control which host filesystem paths are bind-mounted
into a package's sandbox.

### Syntax

```
source:grants
source:dest:grants
```

- `~/` -- paths relative to the user's real home directory.
- `/` -- absolute host paths.
- `dest` -- where the path appears inside the sandbox (defaults to
  same as source).
- `grants` -- comma-separated list of allowed operations.

### Operations

| Operation | Meaning                              |
|-----------|--------------------------------------|
| `r`       | Read file contents                   |
| `w`       | Write / create files                 |
| `list`    | Browse filenames and metadata        |
| `exec`    | Execute binaries from this mount     |

### Shorthands

| Shorthand | Expands to       |
|-----------|------------------|
| `ro`      | `list,r`         |
| `rw`      | `list,r,w`       |
| `rw+x`    | `list,r,w,exec`  |

### Explicit Deny

Use `deny(op)` to silently block an operation without prompting:

```
~/.ssh:list,deny(w)
```

This allows listing files in `~/.ssh` but silently blocks writes.

---

## Per-User Per-Package Isolation

Permissions are scoped to `(user, package_name)`.  User A granting
Firefox camera access has no effect on user B.  Each user has:

- Their own permission database at
  `~/.config/bingux/permissions/<package>.toml`
- Their own user profile and generation history
- Independent grant/revoke state

Permissions are keyed by package **name**, not the full versioned ID.
Upgrading Firefox from 128.0 to 129.0 preserves all permission grants.

---

## Runtime Prompting Flow

When a sandboxed process makes a privileged syscall:

1. The seccomp filter (`SECCOMP_RET_USER_NOTIF`) pauses the thread.
2. `bingux-gated` receives the notification via the listener fd.
3. The daemon maps the PID to a `(package, user)` pair.
4. It checks the permission database:
   - **allow** -- respond `CONTINUE`, syscall proceeds.
   - **deny** -- respond with `-EACCES`, syscall fails.
   - **absent** -- prompt the user.
5. `bingux-gated` sends a D-Bus request to `bingux-prompt`.
6. The prompt dialog shows:
   - The package name and icon
   - What resource is being requested
   - Three buttons: **Deny** / **Allow Once** / **Always Allow**
7. **Deny** -- respond `-EACCES`.
8. **Allow Once** -- respond `CONTINUE` (not persisted).
9. **Always Allow** -- persist the grant to the user's permission file,
   respond `CONTINUE`.

Dangerous capabilities (`root`, `kernel:module`, `raw:net`) never offer
"Always Allow" -- the user must approve every time.

---

## home.toml Permission Declarations

Users can pre-grant permissions declaratively in their `home.toml`:

```toml
[permissions.firefox]
allow = ["gpu", "audio", "display", "net:outbound", "clipboard"]
deny = ["camera"]
mounts = [
    "~/Downloads:list,w",
    "~/.mozilla:rw",
]

[permissions.neovim]
allow = ["display", "clipboard"]
mounts = [
    "~/src:rw",
    "~/Documents:rw",
    "~/.config/nvim:rw",
    "~/.ssh:list,deny(w)",
]
```

Running `bpkg home apply` converges the permission database to match
these declarations.

---

## system.toml Service Permissions

System services have their permissions declared in `system.toml`:

```toml
[permissions.nginx]
allow = ["net:listen", "net:outbound"]
deny = ["gpu", "audio", "display", "camera"]
mounts = [
    "/srv/www:ro",
    "/etc/letsencrypt:ro",
]

[permissions.sshd]
allow = ["net:listen"]
deny = ["gpu", "audio", "display", "camera"]
mounts = [
    "/etc/ssh:ro",
]
```

These are applied by `bsys apply` and take effect for all users.

---

## bpkg grant / revoke Commands

Permissions can also be managed imperatively from the command line:

```bash
# Grant capabilities
bpkg grant firefox gpu audio display net:outbound

# Revoke a capability
bpkg revoke firefox camera

# Grant a mount
bpkg grant firefox "~/Downloads:list,w"

# View current permissions
bxc perms firefox
```

Grants made via `bpkg grant` are persisted to the user's permission
file immediately.  They are equivalent to adding the permission to
`home.toml` and running `bpkg home apply`.

---

## Permission File Format

Each package's permissions are stored as a TOML file:

```toml
[meta]
package = "firefox"
first_prompted = "2025-01-15T14:30:00Z"

[capabilities]
gpu = "allow"
audio = "allow"
display = "allow"
camera = "deny"

[mounts]
"~/Downloads" = "list,w"
"~/.mozilla" = "rw"

[files]
"~/.ssh/id_rsa.pub" = "r"
"~/.ssh/id_rsa" = "deny(r)"
```

The three-state model: `"allow"`, `"deny"`, or absent (implies prompt).
