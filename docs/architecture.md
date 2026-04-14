# Bingux System Architecture

This document provides a concise overview of the Bingux system
architecture.  For the full specification see [PLAN.md](../PLAN.md).

---

## Package Store

Every installed package lives in its own directory under
`/system/packages/<name>-<version>-<arch>/`.  Each directory contains a
complete virtual Unix filesystem (`bin/`, `lib/`, `share/`, etc.) plus
a `.bpkg/` metadata directory with `manifest.toml`, `files.txt`
(integrity hashes), and `patchelf.log`.

Multiple versions of any package can coexist in the store
simultaneously.  The store is root-owned and immutable once a package is
sealed.

---

## Patchelf Strategy

Binaries in the store cannot rely on `/usr/lib` or `/lib` because those
paths do not exist in the traditional sense.  After a package is built,
every ELF binary and shared library is patched:

- **PT_INTERP** is rewritten to point at the exact glibc in the store,
  e.g. `/system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2`.
- **RUNPATH** is set to a colon-separated list of library directories
  drawn from the package itself and all its dependencies.

Resolution order for RUNPATH entries:

1. The package's own `lib/` directory (bundled libs win).
2. Direct dependencies' `lib/` directories (in `depends=()` order).
3. Transitive dependencies (depth-first resolution).

Scripts have their shebangs rewritten to store paths as well
(`#!/usr/bin/python3` becomes
`#!/system/packages/python-3.12-x86_64-linux/bin/python3`).

For libraries loaded at runtime via `dlopen()`, packages can declare
`dlopen_hints` in their recipe.  The permission daemon also intercepts
`open()` calls for `.so` files outside permitted locations.

---

## Sandbox Levels

Every package runs inside a lightweight namespace sandbox managed by
`bxc`.  Four levels are available:

| Level      | Namespaces          | Notes                                   |
|------------|---------------------|-----------------------------------------|
| `none`     | (no sandbox)        | Only for boot-essential packages        |
| `minimal`  | mount               | Filesystem isolation only               |
| `standard` | mount + pid         | Default for most packages               |
| `strict`   | mount + pid + net   | Network denied unless explicitly granted|

The sandbox mounts the package's store directory as the root view and
adds dynamic bind-mounts (home directories, devices) based on the
user's permission grants.

---

## Permission Prompting

Bingux uses a macOS-style runtime permission model.  Nothing is granted
by default; every privileged operation triggers a user prompt.

### Flow

```
sandboxed process
  -> syscall trapped by seccomp (SECCOMP_RET_USER_NOTIF)
  -> kernel pauses the thread
  -> bingux-gated receives notification via listener fd
  -> checks per-user per-package permission database
     -> "allow" : SECCOMP_USER_NOTIF_FLAG_CONTINUE
     -> "deny"  : respond with -EACCES
     -> absent  : send D-Bus request to bingux-prompt (GUI)
                   user clicks Deny / Allow Once / Always Allow
                   persist if "Always", respond to kernel
```

### Components

- **bxc-shim** -- installs the seccomp filter and hands the listener fd
  to `bingux-gated` before exec'ing the real binary.
- **bingux-gated** -- the permission daemon (runs as root).  Receives
  seccomp notifications, decodes syscalls, queries the permission
  database, and dispatches prompts.
- **bingux-prompt** -- GTK4/Adwaita dialog that presents the prompt to
  the user and returns the response over D-Bus.
- **bingux-settings** -- GUI for browsing and revoking permissions.

### Permission Types

**Capability permissions** control access to hardware, network, display,
IPC, and dangerous operations (gpu, audio, camera, net:outbound,
display, clipboard, dbus:session, etc.).

**Mount permissions** control which host filesystem paths are
bind-mounted into the sandbox.  Syntax: `source:grants` or
`source:dest:grants`.  Operations: `r`, `w`, `list`, `exec`.  Deny
with `deny(op)`.

Permissions are keyed by package *name*, not full ID, so they survive
version upgrades.

---

## Two-Layer Composition

The running system has two independent layers of profile composition:

### System Profile (`/system/profiles/`)

Managed by root via `bsys`.  Defines which packages and versions are
available system-wide: boot-critical packages, shared libraries, system
services.

```
/system/profiles/
  current -> generation-42/
  generation-42/
    bin/        symlinks to default package versions
    lib/        exported shared libraries
    share/      .desktop files, icons, etc.
    .dispatch.toml   system default dispatch table
```

### User Profile (`/system/users/<user>/profile/`)

Managed per-user via `bpkg`.  Can add packages, override versions, and
maintain independent generation history.

```
/system/users/kieran/profile/
  current -> generation-7/
  generation-7/
    bin/        user overrides + user-installed binaries
    .dispatch.toml   user-level dispatch overrides
```

When resolving a binary, the user profile is checked first, then the
system profile.  Both profiles are independently atomic.

---

## Volatile-by-Default Model

All packages -- both system and user -- are **volatile** by default.
User packages exist only for the current session (until logout); system
packages exist only until reboot.  To keep a package permanently:

```
bpkg add --keep firefox    # persistent from the start
bpkg keep firefox          # promote an already-installed volatile package
```

Boot-essential packages (kernel, glibc, init, bxc, bingux-gated) are
installed with `bsys add --keep` during system setup and must never be
made volatile.

Volatile packages are cleaned up automatically at the appropriate
boundary (logout or reboot).  Permissions outlive volatile installs and
are only cleaned up by explicit `revoke`.

---

## Ephemeral Root

The root filesystem (`/etc`, `/run`, etc.) is regenerated at boot from
`system.toml`.  There is no mutable root partition that accumulates
state.  The system configuration lives in a git-backed `system.toml`
file at `/system/config/system.toml`.

User configuration follows the same pattern: `home.toml` at
`~/.config/bingux/config/home.toml` declares packages, environment
variables, dotfile links, and permission pre-grants.  Running
`bpkg home apply` converges the user's environment to match the
declaration.

---

## Git-Backed Configuration

Both `system.toml` and each user's `home.toml` are stored in git
repositories on persistent volumes.  Every `bpkg add --keep`,
`bpkg pin`, or `bpkg grant` is a git commit.  This provides:

- Full history of every configuration change
- Diffing between any two points in time
- Branching for experimentation
- Remote sync for backup or multi-machine setups

The repo layout is flexible: separate repos per user, a single monorepo
for the whole machine, or a system repo that references external user
config URLs.  The tools read from fixed paths regardless of git
structure.

---

## Crate Map

| Crate             | Purpose                                    |
|-------------------|--------------------------------------------|
| bingux-common     | Shared types, path constants, error types  |
| bpkg-recipe       | BPKGBUILD recipe parser                    |
| bpkg-store        | Package store operations                   |
| bpkg-resolve      | Dependency resolution                      |
| bpkg-patchelf     | ELF patching (PT_INTERP, RUNPATH, shebang) |
| bpkg-build        | Build orchestrator                         |
| bpkg-repo         | Repository index and sync                  |
| bpkg-home         | home.toml convergence engine               |
| bxc-sandbox       | Seccomp profile and namespace setup        |
| bxc-runtime       | Runtime dispatch and version resolution    |
| bxc-shim          | Thin launcher that sets up sandbox + exec  |
| bingux-gated      | Permission daemon (seccomp listener)       |
| bingux-prompt     | GTK4 permission dialog                     |
| bingux-settings   | Permission browser/editor GUI              |
| bsys-compose      | System profile generation builder          |
| bsys-config       | system.toml parser and validator           |
| bingux-init       | Early boot init logic                      |
| bpkg (cli)        | User package manager CLI                   |
| bsys (cli)        | System manager CLI (root)                  |
| bxc (cli)         | Sandbox runtime CLI                        |

---

## Repositories

Packages are namespaced by scope: `@bingux.firefox` (official),
`@brave.brave-browser` (third-party).  Every repository is an index of
`.bgx` archives -- there is no distinction between "binary repos" and
"recipe repos" at the repo level.

Binary packages download pre-built binaries; source packages (by
convention named with a `-src` suffix) compile from source.  Both
produce the same `.bgx` output format.

---

## Build Pipeline

```
bsys build <recipe>
  1. Parse BPKGBUILD
  2. Resolve depends + makedepends
  3. Create build container (overlayfs: deps as lower, empty upper)
  4. Run fetch() -> build() -> package() inside container
  5. Capture $PKGDIR as package content
  6. Patchelf phase (outside container)
  7. Archive to .bgx
  8. Install to /system/packages/
```

Build containers are isolated (overlayfs, restricted network, non-root
user).  The runtime sandbox is completely separate -- it uses the
installed package directory with dynamic mounts based on permissions.
