# Internals: Generations

How Bingux composes, swaps, and rolls back system and user profiles
atomically.

---

## Overview

A **generation** is a snapshot of which packages (and versions) are
active at a given point in time.  Generations are immutable once built.
Activating a generation is a single atomic `rename(2)` of a symlink.

There are two independent generation chains:

- **System generations** at `/system/profiles/` (managed by `bsys`).
- **User generations** at `/system/users/<user>/profile/` (managed by
  `bpkg`).

---

## System Generations

```
/system/profiles/
  current -> generation-42/     # atomic symlink
  generation-42/
    bin/
      firefox -> (dispatch shim)
      bash -> /system/packages/bash-5.2-x86_64-linux/bin/bash
    lib/
      libc.so.6 -> /system/packages/glibc-2.39-x86_64-linux/lib/libc.so.6
    share/
      applications/             # merged .desktop files
      icons/
    etc/                        # merged default configs
    .dispatch.toml              # system default dispatch table
    generation.toml             # metadata: id, timestamp, package list
```

Each generation directory contains symlinks (or dispatch shims) for
every exported binary, library, and data file from the packages in that
generation.

---

## User Generations

```
/system/users/kieran/profile/
  current -> generation-7/
  generation-7/
    bin/
      firefox -> (dispatch shim, overrides system default to 128.0.1)
      my-tool -> /system/packages/my-tool-1.0-x86_64-linux/bin/my-tool
    .dispatch.toml              # user-level dispatch overrides
    generation.toml
```

User generations can:
- Override which version of a system package the user sees.
- Add packages that only this user has installed.
- Omit packages -- falling back to the system generation.

---

## Dispatch Tables

The `.dispatch.toml` file maps command names to package store paths and
sandbox levels:

```toml
[[entries]]
name = "firefox"
package_id = "firefox-129.0-x86_64-linux"
binary = "bin/firefox"
sandbox = "standard"

[[entries]]
name = "bash"
package_id = "bash-5.2-x86_64-linux"
binary = "bin/bash"
sandbox = "none"
```

When a user runs `firefox`, the runtime resolver:
1. Checks the user's `.dispatch.toml` first.
2. Falls back to the system's `.dispatch.toml`.
3. Launches the binary with the specified sandbox level.

This allows users to pin different versions without affecting other
users or the system default.

---

## Atomic Symlink Swap

Activating a new generation is a single atomic operation:

```rust
// Pseudocode
let new_link = profiles_dir.join("current.tmp");
std::os::unix::fs::symlink(new_gen_dir, &new_link)?;
std::fs::rename(&new_link, profiles_dir.join("current"))?;
```

The `rename(2)` syscall atomically replaces the `current` symlink.
Any process that already resolved the old symlink continues running
with the old generation.  New process lookups see the new generation
immediately.

This is the same strategy Nix and GNU Guix use for atomic profile
switches.

---

## Building a Generation

The `bsys-compose` crate's `GenerationBuilder` constructs a new
generation:

1. **Collect packages** -- read the declared package list from
   `system.toml` (system) or the user's profile state.
2. **Resolve versions** -- for each package, pick the version to use
   (pinned, latest, or overridden).
3. **Build export list** -- for each package, read `exports` from the
   manifest and compute the set of symlinks to create.
4. **Create directory** -- write the generation directory with all
   symlinks, the dispatch table, and `generation.toml`.
5. **Swap symlink** -- atomically point `current` to the new
   generation.

Each generation records:
- A monotonically increasing generation ID.
- A Unix timestamp.
- The SHA-256 hash of the configuration that produced it.
- The list of packages with their IDs and sandbox levels.

---

## Rollback

Rolling back is simply re-pointing the `current` symlink:

```bash
# System rollback
bsys rollback           # previous generation
bsys rollback 40        # specific generation

# User rollback
bpkg rollback
bpkg rollback 5
```

The old generation directory still exists on disk (it was never
deleted), so rollback is instant -- no rebuilding required.

---

## Garbage Collection

Over time, old generations accumulate.  `bsys gc` cleans up:

1. **Identify live generations** -- the `current` symlink for every
   user and for the system profile.
2. **Identify live packages** -- the union of all packages referenced
   by live generations.
3. **Remove dead generations** -- delete generation directories that
   are not live and not within the keep window (by default, the last 5
   generations are kept).
4. **Remove dead packages** -- delete package store entries not
   referenced by any live generation.

`bsys gc --dry-run` shows what would be removed without deleting
anything.

Garbage collection scans all users, so it requires root.

---

## Generation Metadata Format

Each generation stores a `generation.toml`:

```toml
id = 42
timestamp = 1720000000
config_hash = "sha256:abc123..."

[[packages]]
id = "firefox-129.0-x86_64-linux"
sandbox = "standard"

[[packages]]
id = "bash-5.2-x86_64-linux"
sandbox = "none"

[[packages]]
id = "glibc-2.39-x86_64-linux"
sandbox = "none"
```

---

## Relationship Between System and User Generations

System and user generations are independent:

- `bsys apply` creates a new system generation.  Existing user
  generations are not modified.
- `bpkg apply` creates a new user generation.  The system generation
  is not modified.
- A user generation can reference packages that only exist in the
  system generation (no duplication).
- Version resolution: user profile -> system profile -> error.

This two-layer model means a system administrator can upgrade system
packages without disrupting user-specific version pins, and users can
experiment freely without affecting the system or other users.
