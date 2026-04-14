# Internals: seccomp-unotify

Technical deep dive into how Bingux uses `SECCOMP_RET_USER_NOTIF` to
intercept privileged syscalls and surface permission prompts.

---

## SECCOMP_RET_USER_NOTIF Mechanism

Linux 5.0 introduced `SECCOMP_RET_USER_NOTIF`, a seccomp return action
that pauses the trapped thread and sends a notification to a supervisor
process.  The supervisor reads the notification, inspects the syscall
arguments, decides whether to allow or deny it, and sends a response.
The kernel then either continues the syscall or returns an error.

Key kernel structures:

```c
struct seccomp_notif {
    __u64 id;           // unique notification ID
    __u32 pid;          // PID of the trapped thread
    __u32 flags;
    struct seccomp_data data;  // syscall nr + args
};

struct seccomp_notif_resp {
    __u64 id;           // must match the notification
    __s32 val;          // return value (if not CONTINUE)
    __s32 error;        // errno (negative, e.g. -EACCES)
    __u32 flags;        // SECCOMP_USER_NOTIF_FLAG_CONTINUE to allow
};
```

---

## How bxc-shim Sets Up the Filter

`bxc-shim` is the thin launcher that runs before the sandboxed binary.
Its seccomp setup:

1. **Build BPF program** from the package's sandbox profile.  The
   profile classifies syscalls into:
   - **ALLOW** -- safe syscalls (read, write, mmap, brk, futex,
     clock_gettime, etc.) pass through without interception.
   - **USER_NOTIF** -- sensitive syscalls are trapped and forwarded to
     the supervisor.  These include: `openat`, `open`, `connect`,
     `bind`, `listen`, `ioctl` (on device fds), `mount`, `umount2`,
     `ptrace`, `execve`, `execveat`.
   - **KILL** -- a small set of always-forbidden syscalls (e.g.
     `reboot`).

2. **Install filter** via `seccomp(SECCOMP_SET_MODE_FILTER)`.  This
   returns a listener file descriptor.

3. **Hand off listener fd** to `bingux-gated` over a Unix socket.  The
   daemon registers the `(fd, pid, package, user)` tuple in its PID
   registry.

4. **exec()** the actual binary.  The seccomp filter is inherited.

---

## Listener fd Handoff

The listener fd must reach `bingux-gated` (which runs as root) from
`bxc-shim` (which runs as the user).  The handoff uses a Unix domain
socket with `SCM_RIGHTS` (fd passing):

```
bxc-shim                          bingux-gated
   |                                    |
   |-- connect(/run/bingux-gated.sock) -|
   |-- sendmsg(listener_fd, metadata) ->|
   |                                    |-- register(pid, pkg, user, fd)
   |-- exec(real_binary) ------------->|
```

Metadata includes the package name, user, UID, and sandbox level.

---

## Event Loop in bingux-gated

`bingux-gated` uses `epoll` to multiplex across all active listener
fds (one per sandboxed process):

```
loop {
    events = epoll_wait(epoll_fd)
    for event in events {
        notification = ioctl(listener_fd, SECCOMP_IOCTL_NOTIF_RECV)
        response = handle_event(notification)
        ioctl(listener_fd, SECCOMP_IOCTL_NOTIF_SEND, response)
    }
}
```

When a sandboxed process exits, its listener fd becomes readable with
an error, and the daemon cleans up the PID registry entry.

---

## Permission Resolution Flow

For each trapped syscall:

1. **Decode** -- map `(syscall_nr, args)` to a `PermissionRequest`
   enum (FileAccess, NetworkConnect, DeviceAccess, etc.).
2. **PID lookup** -- find the `(package, user)` for the trapped PID.
3. **Check database** -- look up the user's permission file for the
   package:
   - `allow` → respond `CONTINUE`.
   - `deny` → respond `-EACCES`.
   - absent → **prompt**.
4. **Prompt** -- send a D-Bus request to `bingux-prompt`.  The prompt
   displays the package name, the resource being requested, and three
   options.
5. **Persist** -- if the user clicks "Always Allow", write the grant
   to the TOML permission file.  "Allow Once" does not persist.

The syscall decoder handles:
- `openat` → FileAccess (path resolved from `/proc/<pid>/fd/` + dirfd)
- `connect` → NetworkConnect (address + port from sockaddr)
- `bind` / `listen` → NetworkBind / NetworkListen
- `ioctl` → DeviceAccess (categorised by device type)
- `execve` / `execveat` → ProcessExec
- `ptrace` → ProcessPtrace
- `mount` → Mount

---

## Performance Considerations

### Hot-Path Caching

Most syscalls from a running application are safe (read, write, mmap)
and hit the BPF ALLOW path with no supervisor involvement -- this adds
only nanoseconds of overhead.

For trapped syscalls, the first call for a given `(package, capability)`
pair may block on a user prompt (seconds).  Subsequent calls for the
same capability hit the in-memory permission cache in `bingux-gated`
and respond in microseconds.

### Cache Hierarchy

1. **BPF filter** -- safe syscalls never leave the kernel.
2. **In-memory HashMap** in `bingux-gated` -- previously resolved
   `(package, capability) -> allow/deny` pairs.
3. **TOML file on disk** -- loaded once per package, cached in
   `PermissionDb`.

### Prompt Deduplication

If multiple threads in the same sandbox trigger the same capability
concurrently, `bingux-gated` coalesces them into a single prompt.
Other threads waiting on the same capability are blocked until the
first prompt is resolved, then all receive the same answer.

---

## TOCTOU Risks

`SECCOMP_RET_USER_NOTIF` has an inherent time-of-check-time-of-use
gap: between when the daemon reads the syscall arguments and when the
kernel executes the syscall, the trapped thread's memory could be
modified by another thread in the same process.

### Mitigations

1. **SECCOMP_USER_NOTIF_FLAG_CONTINUE** -- when we allow a syscall, we
   tell the kernel to re-execute it from scratch.  The kernel re-reads
   the arguments from the process's registers, not from our copy.

2. **Path verification via /proc/<pid>/fd** -- for `openat` calls, the
   daemon resolves the actual path through `/proc/<pid>/fd/<dirfd>`
   rather than trusting the userspace pointer.  This is still racy but
   significantly narrows the window.

3. **Kernel SECCOMP_IOCTL_NOTIF_ID_VALID** -- before responding, the
   daemon checks that the notification is still valid (the process
   hasn't been killed or the syscall hasn't been interrupted).

4. **PID reuse** -- the daemon validates that the PID still belongs to
   the expected process by checking `/proc/<pid>/status` before
   responding.

5. **Scope of trust** -- for many permission categories (network,
   device access), the TOCTOU risk is minimal because the *category* of
   access is correct even if the specific arguments change.  The main
   risk is with file paths, which is why mount-level permissions
   (granting access to a directory) are preferred over single-file
   grants.

---

## References

- `seccomp_unotify(2)` man page
- `bxc-sandbox` crate: BPF program generation (`syscalls.rs`)
- `bingux-gated` crate: daemon event loop (`daemon.rs`), syscall
  decoder (`decoder.rs`)
