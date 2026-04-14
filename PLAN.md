# Bingux — Implementation Plan (v2)

## Overview

Bingux is a Linux distribution where every package lives in its own isolated directory under `/system/packages/<n>-<version>-<arch>`, containing a complete virtual Unix filesystem. Binaries are patchelf'd at install time so they resolve libraries directly from the package store without containers. Privileged operations are intercepted at runtime and surfaced to the user via graphical permission prompts (macOS-style). The system is fully atomic — updates install new package versions alongside old ones and swap a single symlink to activate them.

The system is designed to self-host (dogfood) from early on: bpkg, bxc, bsys, and the permission daemon are all bpkg packages themselves, built and managed through the same pipeline.

-----

## Core Concepts

**Package Store**: `/system/packages/` — every installed package is a self-contained directory with a full FHS layout (`bin/`, `lib/`, `etc/`, …) inside it.

**Patchelf'd Binaries**: After a package is built, all ELF binaries and shared libraries are patched so their `RPATH`/`RUNPATH` and interpreter (`PT_INTERP`) point directly into the package store. A Firefox binary's dynamic linker might be `/system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2` and its RUNPATH might list `/system/packages/glibc-2.39-x86_64-linux/lib:/system/packages/gtk3-3.24-x86_64-linux/lib:...`. No containers needed for normal execution.

**Sandboxed Execution**: Packages run inside a lightweight namespace sandbox (mount + optionally pid/net). The sandbox starts restrictive — packages get very little by default.

**Runtime Permission Prompting**: When a sandboxed process attempts a privileged operation (file access outside its package, network, GPU, audio, etc.), execution is paused via seccomp `SECCOMP_RET_USER_NOTIF` and a graphical prompt is shown to the user. The user can Allow Once, Allow Always (persisted per-user-per-package), or Deny. This is the macOS Gatekeeper/TCC model brought to Linux.

**Volatile by Default**: All packages — both system and user — are **volatile** by default. They only exist for the current session (user packages: until logout; system packages: until reboot) and are cleaned up automatically. To keep a package permanently, you explicitly keep it with `--keep` (or `bpkg keep <pkg>` / `bsys keep <pkg>` after the fact). This keeps the system clean: trying out a tool or testing a system-wide upgrade doesn't permanently alter your environment. Boot-essential packages (kernel, glibc, init, bxc, bingux-gated, etc.) are installed with `bsys add --keep` during system setup and should never be made volatile.

**System Composition**: The "running system" has two layers of composition. The **system profile** is a global generation of symlinks composed by root — it defines which packages and versions are available system-wide (boot-critical packages, shared libraries, system services). It contains both volatile and persisted system packages. Each **user profile** is a per-user generation that selects which versions of which packages that user wants active, and can add additional packages the user has installed for themselves. User profiles also track both volatile and persisted packages.

**Multi-version**: Multiple versions of any package can be installed simultaneously in the store (e.g. `firefox-128.0.1` and `firefox-129.0`). The system profile picks a default version. Users can override with a different version in their user profile. Running `firefox` resolves through the user's profile first, then falls back to the system profile.

**Multi-user**: Permissions are per-user-per-package. User A granting Firefox camera access has no effect on user B. Each user has their own permission database, their own user profile (active package versions and user-installed packages), and their own generation history.

**Atomicity**: Updating a package installs a new version alongside the old one. Profile symlinks are swapped in one `rename(2)` operation. Rollback = point symlinks back. Both system and user profiles are independently atomic.

**Repositories**: Packages are namespaced by repository using `@scope.package` syntax. The default repo is `bingux` (the official repo), so `bpkg add firefox` is shorthand for `bpkg add @bingux.firefox`. Third-party repos can be added — `bpkg add @brave.brave-browser` installs Brave's browser from Brave's repository. Every repo is the same thing: an index of `.bgx` packages. There are no "binary repos" vs "recipe repos" — every repo serves `.bgx` files. The recipe inside the `.bgx` determines how it was built. Packages compiled from source have a `-src` suffix in their name (e.g. `firefox-src`); binary packages (the default) just download and package a pre-built binary.

**Git-Backed Configuration**: Both `home.toml` (user config at `/users/<user>/.config/bingux/config/`) and `system.toml` (system config at `/system/config/`) are stored in git repositories on persistent volumes. Every `bpkg add --keep`, `bpkg pin`, or `bpkg grant` is a git commit. This means full history, diffing, branching, and remote sync come for free. The repo layout is flexible: you can have separate repos for system and each user, a single monorepo for the entire machine, or a system repo that references external user config URLs — the tools read from fixed paths regardless of how the git repos are structured.

**Build Containers & Runtime Containers**: Every BPKGBUILD recipe runs inside an isolated build container. The build container starts from a clean base with only the declared dependencies mounted. Whatever the recipe changes (files created, binaries installed) is captured as a diff — that diff becomes the package content. The package then runs in a separate runtime container (the sandbox) with only its own files visible plus dynamically granted mounts. Build and runtime are fully separated: the build environment is thrown away after the `.bgx` is produced.

**User Directories**: Each user's home directory is `/users/<username>/` — a top-level directory, not buried under `/home/`. It's a normal home directory. Bingux managed state (profiles, permissions, per-package homes) lives in `~/.config/bingux/`, mirroring the `/system/` structure. `/home` is a hidden symlink to `/users` for compatibility.

**Self-hosting**: The entire Bingux toolchain (bpkg, bsys, bxc, bingux-gated, etc.) is packaged as bpkg packages. The system can rebuild itself.

### Privilege Model

```
bsys — REQUIRES ROOT (system administration):
  bsys add <pkg>               Volatile system install. Available until reboot.
  bsys add --keep <pkg>        Persistent system install. Survives reboot.
                               Boot-essential packages MUST use --keep.
  bsys rm <pkg>                Remove from system immediately, recompose.
  bsys keep <pkg>              Promote system volatile → persistent.
  bsys unkeep <pkg>            Demote system persistent → volatile.
                               (refuses for boot_essential without --force)
  bsys build <recipe>          Build + install to /system/packages/ (shared store).
  bsys upgrade [pkg|--all]     Build + install new version system-wide.
  bsys apply                   Recompose system profile.
  bsys rollback [generation]   Swap /system/profiles/current symlink.
  bsys grant <pkg> <perms>     Pre-grant for system services.
  bsys revoke <pkg> <perms>    Revoke for system services.
  bsys gc                      Garbage collect store (scans all users).

bpkg — NO ROOT NEEDED (user package management):
  bpkg add <pkg>               Volatile install. Available for this session only.
                               Package must already be built in the store.
  bpkg add --keep <pkg>        Persistent install. Survives logout/reboot.
  bpkg rm <pkg>                Remove immediately (volatile or persistent).
  bpkg rm --purge <pkg>        Remove + delete user config for package.
  bpkg keep <pkg>              Promote volatile → persistent.
  bpkg unkeep <pkg>            Demote persistent → volatile.
  bpkg upgrade [pkg|--all]     Upgrade user packages (inherits volatility).
  bpkg pin <pkg>=<ver>         Pin specific version (always persistent).
  bpkg unpin <pkg>             Remove pin, fall back to system default.
  bpkg list                    List user packages (shows [volatile]/[kept]/[pin]).
  bpkg grant <pkg> <perms>     Pre-grant permissions (own user only).
  bpkg revoke <pkg> <perms>    Revoke permissions (own user only).
  bpkg apply                   Recompose user profile.
  bpkg rollback [generation]   Roll back user profile.
  bsys build <recipe>          Build package (requires root or build daemon).

bxc — NO ROOT FOR NORMAL USE (sandbox runtime):
  bxc run <pkg> [args...]      Run package in sandbox.
  bxc run <pkg>@<ver> [args...] Run specific version.
  bxc shell <pkg>              Interactive shell in sandbox.
  bxc inspect <pkg>            Show sandbox config.
  bxc perms <pkg>              Show/edit own permissions.
  bxc ps                       List running sandboxed processes.

KEY RULES:
  - /system/packages/ is root-owned. Users cannot write to the store.
    Building a package always requires root (or the build daemon).
  - /users/<user>/ is owned by that user (UID). bpkg operates here.
  - /system/profiles/ is root-owned. bsys operates here.
  - bingux-gated runs as root (needs it for seccomp-unotify), but
    writes permissions to the *requesting user's* directory.
  - Both system (bsys) and user (bpkg) packages are volatile by default.
    Boot-essential packages are installed with `bsys add --keep` during setup.
  - Permissions outlive volatile installs. Cleaned up only by explicit revoke.

BUILD DAEMON (optional, for user-triggered builds without root):
  If a user wants a package that isn't in the store yet, they submit a
  bsys build runs the recipe in a build container (requires root),
  validates the recipe, builds in a sandbox, and installs to the store.
  The user never gets root — the daemon does the privileged work.

  # bsys build firefox            # Build from recipe (requires root)
  # or: download pre-built .bgx from a repo
    ├── Receives request
    ├── Validates recipe (trusted repo?)
    ├── Builds in sandbox
    ├── Installs to /system/packages/
    └── User can then: bpkg add firefox
```

-----

## Phase 1: Package Format & Build System (`bpkg`)

### 1.1 — Package Recipe Format (`BPKGBUILD`)

A shell-based recipe file (like PKGBUILD/ebuild) that lives in a git repo of recipes.

```bash
# /system/recipes/firefox/BPKGBUILD
# BINARY PACKAGE (default) — downloads a pre-built binary and packages it.
# This is the common case. No compilation. Fast.

pkgscope="bingux"
pkgname="firefox"
pkgver="128.0.1"
pkgarch="x86_64-linux"
pkgdesc="Mozilla Firefox web browser"
license="MPL-2.0"

depends=("glibc-2.39" "gtk3-3.24" "dbus-1.14" "pulseaudio-17.0")
# No makedepends — binary packages don't compile anything

exports=(
    "bin/firefox"
    "lib/libxul.so"
    "share/applications/firefox.desktop"
    "share/icons/hicolor/128x128/apps/firefox.png"
)

source=("https://archive.mozilla.org/pub/firefox/releases/${pkgver}/linux-x86_64/en-GB/firefox-${pkgver}.tar.bz2")
sha256sums=("abc123...")

# No build() — binary packages skip compilation entirely

package() {
    # Just move the pre-built files into the package layout
    cd "$SRCDIR/firefox"
    mkdir -p "$PKGDIR/bin" "$PKGDIR/lib" "$PKGDIR/share/applications"
    cp -a firefox "$PKGDIR/bin/"
    cp -a *.so "$PKGDIR/lib/"
    cp -a browser "$PKGDIR/lib/"
    install -Dm644 firefox.desktop "$PKGDIR/share/applications/"
}
```

```bash
# /system/recipes/firefox-src/BPKGBUILD
# SOURCE PACKAGE — compiles from source. Slower, full control.
# Convention: source packages end in -src

pkgscope="bingux"
pkgname="firefox-src"
pkgver="128.0.1"
pkgarch="x86_64-linux"
pkgdesc="Mozilla Firefox web browser (compiled from source)"
license="MPL-2.0"

depends=("glibc-2.39" "gtk3-3.24" "dbus-1.14" "pulseaudio-17.0")
makedepends=("rust-1.79" "cbindgen-0.26" "nodejs-20" "nasm-2.16")

exports=(
    "bin/firefox"
    "lib/libxul.so"
    "share/applications/firefox.desktop"
    "share/icons/hicolor/128x128/apps/firefox.png"
)

source=("https://archive.mozilla.org/pub/firefox/releases/${pkgver}/source/firefox-${pkgver}.source.tar.xz")
sha256sums=("def456...")

build() {
    cd "$SRCDIR/firefox-${pkgver}"
    export MOZ_OBJDIR="$BUILDDIR/obj"
    ./mach configure --prefix=/
    ./mach build
}

package() {
    cd "$SRCDIR/firefox-${pkgver}"
    DESTDIR="$PKGDIR" ./mach install
}
```

**Both produce a `.bgx` at the end.** The recipe defines the build process — binary recipes just fetch + arrange files, source recipes compile. But the output is the same: a patchelf'd package directory, archived as `.bgx`, served from a repo index. There's no distinction at the repo level.

**The `-src` convention:**

- `firefox` → binary package (downloads Mozilla's official build)
- `firefox-src` → source package (compiles from source)
- A user can choose: `bpkg add firefox` (fast, binary) or `bpkg add firefox-src` (slow, compiled)
- Both are just packages in the repo index — different names, both `.bgx` files

**Note**: There is no `container.mounts` section. Permissions are not declared ahead of time — they are requested at runtime and granted by the user.

### 1.2 — Build Tool (`bsys build`)

**Language**: Rust

Every recipe runs inside an isolated **build container**. The container starts from a clean base image (overlayfs lower layer: declared dependencies). The recipe's `fetch()`, `build()`, and `package()` functions run inside this container. Whatever changes the recipe makes to the filesystem are captured via the overlayfs upper layer — this diff becomes the package content.

```
bsys build firefox
  ├── 1. Parse /system/recipes/firefox/BPKGBUILD
  ├── 2. Resolve depends + makedepends → ensure all present in /system/packages/
  ├── 3. Create BUILD CONTAINER:
  │     ├── OverlayFS:
  │     │   lower (ro) = merged dependency packages (glibc, gtk3, dbus, ...)
  │     │   upper (rw) = empty tmpfs (this is where all changes land)
  │     │   merged    = what the recipe sees as /
  │     ├── The recipe sees a normal FHS layout:
  │     │   /usr/bin/gcc, /usr/lib/libc.so.6, etc. (from deps in lower)
  │     │   Anything the recipe writes goes to upper layer
  │     ├── Mount $SRCDIR and $BUILDDIR as rw workspace
  │     ├── Network: enabled for fetch(), disabled for build()/package()
  │     └── User: build user (non-root)
  │
  ├── 4. INSIDE BUILD CONTAINER:
  │     ├── fetch() → downloads sources to $SRCDIR
  │     ├── build() → compiles into $BUILDDIR (source packages only)
  │     └── package() → installs to $PKGDIR
  │           Binary recipes: move pre-built files into $PKGDIR
  │           Source recipes: DESTDIR="$PKGDIR" make install (or equivalent)
  │
  ├── 5. CAPTURE DIFF:
  │     ├── $PKGDIR contains everything the recipe installed
  │     ├── This is the package content — nothing else from the
  │     │   build container leaks into the package
  │     └── Build container is destroyed after this step
  │
  ├── 6. PATCHELF phase (outside container, on captured content):
  │     ├── Scan $PKGDIR for all ELF binaries and shared objects
  │     ├── For each: resolve needed libraries against depends=()
  │     ├── Set PT_INTERP → /system/packages/glibc-<ver>-<arch>/lib/ld-linux-x86-64.so.2
  │     ├── Set RUNPATH → colon-separated list of dep lib dirs
  │     └── Verify: ldd <binary> resolves all symbols
  │
  ├── 7. Archive → firefox-128.0.1-x86_64-linux.bgx (tar.bz2)
  │
  └── 8. Install: extract .bgx to /system/packages/firefox-128.0.1-x86_64-linux/
         This directory is the RUNTIME CONTAINER's root — when Firefox
         runs in its sandbox, this is what it sees as its own filesystem.
```

**Build container vs runtime container:**

- **Build container**: temporary, overlayfs, has build tools + deps mounted, network for fetch only. Destroyed after the `.bgx` is produced. The recipe can write anywhere — only `$PKGDIR` is captured.
- **Runtime container** (sandbox): the installed package directory. When the package runs, the sandbox mounts this as the package's view of the filesystem. Dependencies are resolved via patchelf'd RUNPATH, not via mount overlays. The sandbox adds dynamic mounts (home, devices, etc.) based on user permission grants.

### 1.3 — Patchelf Strategy (Detail)

This is the Nix approach adapted for Bingux's store layout.

**The problem**: A compiled `firefox` binary expects `libc.so.6` at `/lib/libc.so.6` or `/usr/lib/libc.so.6`. But in Bingux, it lives at `/system/packages/glibc-2.39-x86_64-linux/lib/libc.so.6`.

**The solution**: After `package()` completes, bpkg's patchelf phase rewrites every ELF binary:

```
Before patchelf:
  $ readelf -d firefox | grep RUNPATH
    → (empty or /usr/lib)
  $ readelf -l firefox | grep interpreter
    → /lib64/ld-linux-x86-64.so.2

After patchelf:
  $ readelf -d firefox | grep RUNPATH
    → /system/packages/firefox-128.0.1-x86_64-linux/lib:
      /system/packages/glibc-2.39-x86_64-linux/lib:
      /system/packages/gtk3-3.24-x86_64-linux/lib:
      /system/packages/zlib-1.3.1-x86_64-linux/lib:
      ...
  $ readelf -l firefox | grep interpreter
    → /system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2
```

**Library resolution order in RUNPATH**:

1. Package's own `/lib` (first — package's bundled libs win)
2. Direct dependencies' `/lib` dirs (in depends=() order)
3. Transitive dependencies (depth-first resolution)

**Edge cases**:

- **dlopen()**: Some libraries are loaded at runtime via `dlopen("libcuda.so")`. These won't be in RUNPATH. Solution: the permission daemon intercepts the `open()` syscall for `.so` files in non-permitted locations and prompts. Alternatively, packages can declare `dlopen_hints=("libcuda.so=/system/packages/cuda-*/lib/")` in the recipe, and bpkg wraps them via `LD_PRELOAD` shim or patchelf.
- **Scripts**: Shebangs in scripts (`#!/usr/bin/python3`) are rewritten to `/system/packages/python-3.12-x86_64-linux/bin/python3`.
- **pkg-config / CMake find_package**: During build, these are configured to search dependency package dirs. Handled by the build sandbox environment setup.

### 1.4 — File Structure

```
/system/
├── packages/                          # The package store (immutable once sealed)
│   ├── firefox-128.0.1-x86_64-linux/
│   │   ├── .bpkg/                     # Package metadata
│   │   │   ├── manifest.toml          # Name, version, deps, exports
│   │   │   ├── files.txt              # List of all files with hashes
│   │   │   ├── signature.sig          # Optional signing
│   │   │   └── patchelf.log           # What was patched and to what
│   │   ├── bin/
│   │   │   └── firefox                # Patchelf'd — runs natively
│   │   ├── lib/
│   │   │   └── libxul.so              # Patchelf'd — RUNPATH set
│   │   ├── share/
│   │   │   ├── applications/
│   │   │   └── icons/
│   │   └── etc/                       # Package defaults (read-only)
│   │       └── firefox/
│   ├── firefox-129.0-x86_64-linux/    # Multiple versions coexist
│   │   └── ...
│   ├── glibc-2.39-x86_64-linux/
│   │   ├── lib/
│   │   │   ├── libc.so.6
│   │   │   ├── libm.so.6
│   │   │   ├── libpthread.so.6
│   │   │   └── ld-linux-x86-64.so.2  # THE dynamic linker for this glibc
│   │   └── ...
│   └── linux-6.10-x86_64-linux/
│       ├── boot/vmlinuz
│       ├── lib/modules/
│       └── ...
│
├── profiles/                          # System-level composition (managed by root)
│   ├── current -> generation-42/      # Atomic symlink — the active system profile
│   ├── generation-42/
│   │   ├── bin/                       # Symlinks/shims to default versions
│   │   │   ├── firefox -> (shim, dispatches to firefox-129.0 by default)
│   │   │   ├── bash -> /system/packages/bash-5.2-x86_64-linux/bin/bash
│   │   │   └── ...
│   │   ├── lib/                       # Symlinks to exported shared libs
│   │   │   ├── libc.so.6 -> /system/packages/glibc-2.39-x86_64-linux/lib/libc.so.6
│   │   │   └── ...
│   │   ├── share/
│   │   │   ├── applications/          # All .desktop files
│   │   │   └── icons/
│   │   ├── etc/                       # Merged default configs
│   │   └── .dispatch.toml             # System-wide default dispatch table
│   └── generation-41/
│
├── users/                             # Per-user state (one dir per user)
│   ├── kieran/
│   │   ├── profile/                   # User profile — version overrides + user-installed packages
│   │   │   ├── current -> generation-7/
│   │   │   ├── generation-7/
│   │   │   │   ├── bin/               # User's overridden/extra binaries
│   │   │   │   │   ├── firefox -> (shim, dispatches to firefox-128.0.1 — user prefers old)
│   │   │   │   │   └── my-tool -> /system/packages/my-tool-1.0-x86_64-linux/bin/my-tool
│   │   │   │   └── .dispatch.toml     # User-level dispatch overrides
│   │   │   └── generation-6/
│   │   ├── permissions/               # Per-user permission grants
│   │   │   ├── firefox.toml           # kieran's permissions for firefox
│   │   │   ├── code.toml
│   │   │   └── ...
│   │   └── config/                    # Per-user config overrides
│   │       ├── firefox/
│   │       └── ...
│   │   ├── home/                        # user's home directory
│   │   ├── profile/                     # user generations
│   │   ├── permissions/                 # permission grants
│   │   └── config/                      # config overrides
│   └── alice/
│       ├── profile/
│       │   └── current -> generation-3/
│       ├── permissions/               # alice has different permissions
│       │   └── firefox.toml           # alice denied camera, kieran allowed it
│       └── config/
│
├── recipes/                           # Recipe repository (git-managed, @bingux scope)
│   ├── firefox/BPKGBUILD
│   ├── bash/BPKGBUILD
│   └── ...
│
└── state/
    ├── db.sqlite                      # Package database (what's installed system-wide)
    └── locks/                         # Build locks
```

### Tasks for Phase 1

```
1. Create crate: bingux-common
   - Path constants (/system/packages, /system/profiles, etc.)
   - Package ID parsing: "firefox-128.0.1-x86_64-linux" ↔ {name, version, arch}
   - Error types, logging setup

2. Create crate: bpkg-recipe
   - Parser for BPKGBUILD format (shell-like DSL)
   - Struct types: Recipe, Dependency, Export
   - Validation logic

3. Create crate: bpkg-store
   - PackageStore struct managing /system/packages/
   - Install/remove/query operations
   - Manifest (TOML) read/write
   - File integrity checking via files.txt hashes

4. Create crate: bpkg-build
   - Build orchestrator
   - Dependency resolution (topological sort)
   - Sandbox creation for build
   - Source fetching with checksum verification
   - Build/package phase execution

5. Create crate: bpkg-patchelf
   - ELF binary scanner (walk package dir, detect ELF via magic bytes)
   - Dependency library resolver: for each NEEDED entry, find which dep provides it
   - RUNPATH computation (package's own lib + dep libs in resolution order)
   - PT_INTERP rewriting (point to correct glibc's ld-linux)
   - Shebang rewriter for scripts
   - Verification pass: run ldd equivalent, ensure no unresolved symbols
   - Logging: record all patches to .bpkg/patchelf.log

6. Create binary: bpkg
   - CLI frontend (clap)
   - Subcommands: build, install, remove, list, info, search
```

-----

## Phase 2: Permission Daemon & Runtime Security (`bingux-gated`)

This is the core novel feature. Instead of declaring permissions in the recipe, the system intercepts privileged operations at runtime and asks the user.

### 2.1 — Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                        User launches firefox                      │
│                                                                    │
│  bxc-shim reads dispatch table                                     │
│    → creates sandbox (mount namespace + seccomp-unotify filter)    │
│    → exec firefox (patchelf'd binary runs natively in sandbox)     │
│                                                                    │
│  Firefox calls open("/users/kieran/Downloads/file.pdf", O_RDONLY)   │
│    → seccomp filter: //users/*/home/ not yet permitted for this package    │
│    → SECCOMP_RET_USER_NOTIF → kernel pauses the thread             │
│    → bingux-gated receives notification via seccomp listener fd    │
│                                                                    │
│  ┌──────────────────────────────────────────────────────────┐      │
│  │  bingux-gated (permission daemon, runs as root)          │      │
│  │                                                          │      │
│  │  1. Receives: pid=12345, syscall=openat,                 │      │
│  │     args=("/users/kieran/Downloads/file.pdf", O_RDONLY)   │      │
│  │  2. Looks up: pid 12345 → package firefox-128.0.1        │      │
│  │  3. Checks permission DB:                                │      │
│  │     /users/kieran/.config/bingux/permissions/firefox.toml│      │
│  │     → ~/Downloads not in mount set → PROMPT NEEDED       │      │
│  │  4. Sends D-Bus request to bingux-prompt (GUI)           │      │
│  │                                                          │      │
│  │  ┌──────────────────────────────────────────────────┐    │      │
│  │  │  bingux-prompt (GUI process)                     │    │      │
│  │  │                                                  │    │      │
│  │  │  ╔══════════════════════════════════════╗        │    │      │
│  │  │  ║  Firefox wants to access             ║        │    │      │
│  │  │  ║  your Home directory (read)          ║        │    │      │
│  │  │  ║                                      ║        │    │      │
│  │  │  ║  Path: ~/Downloads/file.pdf          ║        │    │      │
│  │  │  ║                                      ║        │    │      │
│  │  │  ║  [Deny] [Allow Once] [Always Allow]  ║        │    │      │
│  │  │  ╚══════════════════════════════════════╝        │    │      │
│  │  └──────────────────────────────────────────────────┘    │      │
│  │                                                          │      │
│  │  5. User clicks "Always Allow"                           │      │
│  │  6. Persist: add "~/Downloads:list,r" to permission file │      │
│  │  7. Respond to seccomp notif: CONTINUE                   │      │
│  │  8. Kernel resumes firefox's openat() → succeeds         │      │
│  └──────────────────────────────────────────────────────────┘      │
└──────────────────────────────────────────────────────────────────┘
```

### 2.2 — Permission & Mount Model

Permissions control what a sandboxed package can access. There are two kinds: **capability permissions** (hardware, network, display, IPC) and **mount permissions** (filesystem paths bind-mounted into the per-package home).

**Default: everything prompts.** You only declare what you GRANT. Anything not explicitly allowed triggers a runtime prompt. Anything explicitly denied is silently blocked.

#### Capability Permissions

```
HARDWARE & DEVICES
  gpu                Access /dev/dri/*
  audio              Access PipeWire/PulseAudio socket
  camera             Access /dev/video*
  input              Access /dev/input/* (raw input)
  usb                Access /dev/bus/usb/*
  bluetooth          Access bluetooth stack

NETWORK
  net:outbound       Make outgoing connections
  net:listen         Bind and listen on ports
  net:port=8080      Bind specific port

DISPLAY & SESSION
  display            Access Wayland/X11 socket
  notifications      Send desktop notifications
  clipboard          Read/write system clipboard
  screenshot         Capture screen content

IPC & SYSTEM
  dbus:session       Access session D-Bus
  dbus:system        Access system D-Bus
  process:exec       Execute binaries outside own package
  process:ptrace     Attach to other processes
  keyring            Access system keyring/secrets

DANGEROUS (always prompt, no "Always Allow")
  root               Request root privileges
  kernel:module      Load kernel modules
  raw:net            Raw network sockets
```

#### Mount Permissions

```
Syntax: "source:grants" or "source:dest:grants"

  ~/        paths relative to real home
  /         absolute host paths
  dest      where it appears in sandbox (default: same as source)
  grants    comma-separated list of allowed operations

Operations:
  r         read file contents
  w         write/create files
  list      browse filenames and metadata (readdir)
  exec      execute binaries from this mount

Explicit deny:
  deny(op)  silently block — no prompt, just EACCES

Shorthands:
  ro    = list,r
  rw    = list,r,w
  rw+x  = list,r,w,exec
```

### 2.3 — Permission Configuration

#### User-Level (home.toml)

```toml
# /users/kieran/.config/bingux/config/home.toml (excerpt)

[mounts]
global = [
    "~/Downloads:list",
    "~/Documents:list",
    "~/Pictures:list",
    "~/Music:list",
    "~/Videos:list",
    "/mnt/nas/shared:~/Shared:ro",
]

[permissions.firefox]
allow = ["gpu", "audio", "display", "net:outbound", "clipboard", "notifications", "dbus:session"]
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
    "~/.local/share/nvim:rw",
    "~/.ssh:list,deny(w)",
]

[permissions.spotify]
allow = ["gpu", "audio", "display", "net:outbound", "notifications"]
```

#### System-Level (system.toml)

```toml
# /system/config/system.toml (excerpt)

[mounts]
global = []

[permissions.nginx]
allow = ["net:listen", "net:outbound"]
deny = ["gpu", "audio", "display", "camera"]
mounts = [
    "/srv/www:ro",
    "/etc/letsencrypt:ro",
]

[permissions.postgresql]
allow = ["net:listen"]
deny = ["gpu", "audio", "display", "camera", "net:outbound"]
mounts = []

[permissions.sshd]
allow = ["net:listen"]
deny = ["gpu", "audio", "display", "camera"]
mounts = [
    "/etc/ssh:ro",
]
```

### 2.4 — Permission Inheritance on Upgrade

Permissions are keyed by package *name*, not full ID. Upgrading Firefox from 128.0.1 to 129.0 keeps all permissions for every user.

### 2.5 — The Seccomp-unotify Mechanism

Linux 5.0+ supports `SECCOMP_RET_USER_NOTIF`, which pauses the syscall and sends a notification to a supervisor process.

```
Sandbox setup (in bxc-shim):
  1. Create seccomp filter:
     - Default: SECCOMP_RET_USER_NOTIF for sensitive syscalls
     - openat/open → notify (file access)
     - connect/bind/listen → notify (network)
     - ioctl on specific devices → notify (GPU, audio, etc.)
     - mount/umount → notify
     - ptrace → notify
     - ALLOW for: read/write/mmap/brk/futex/clock_gettime/... (safe syscalls)
  2. Install filter via seccomp(SECCOMP_SET_MODE_FILTER)
  3. Pass listener fd to bingux-gated via unix socket
  4. exec() the actual binary

bingux-gated event loop:
  for each notification on listener fd:
    1. Read: struct seccomp_notif { id, pid, data: { nr, args } }
    2. Decode syscall: which permission category does this map to?
    3. Resolve UID from /proc/<pid>/status → map to username
    4. Check permission DB:
       - "allow" → respond SECCOMP_USER_NOTIF_FLAG_CONTINUE
       - "deny" → respond with -EACCES
       - "prompt" or missing → D-Bus to bingux-prompt, wait for response
    5. User response → persist if "Always", respond to kernel
```

### 2.6 — bingux-prompt (GUI)

GTK4/Adwaita dialog: package icon + name, resource requested, Deny / Allow Once / Always Allow.

### Tasks for Phase 2

```
7. Create crate: bingux-gated
   - Seccomp listener fd event loop (epoll-based)
   - Syscall decoder: map syscall nr + args → permission category
   - Permission database reader/writer (TOML)
   - In-memory permission cache
   - D-Bus client: send prompt requests to bingux-prompt
   - TTY fallback prompter

8. Create crate: bingux-prompt
   - GTK4/Adwaita permission dialog
   - D-Bus server: receive prompt requests from bingux-gated
   - Three-button response: Deny / Allow Once / Always Allow
   - Timeout handling (30s → Deny)

9. Create crate: bingux-settings
   - Per-package permission browser/editor
   - Permission revocation
   - Global policy toggles

10. Create crate: bxc-sandbox
    - Seccomp filter generator (BPF programs from category definitions)
    - Safe syscall allow-list
    - Sensitive syscall → SECCOMP_RET_USER_NOTIF mapping
    - Listener fd creation and handoff
    - PID tracking: map sandbox pid → package identity
```

-----

## Phase 3: Container / Sandbox Runtime (`bxc`)

### 3.1 — Execution Model

```
User runs: firefox
  ├── Shell resolves: /system/profiles/current/bin/firefox
  │     → actually a bxc-shim hardlink
  │
  ├── bxc-shim:
  │     1. Read argv[0] → look up dispatch table
  │     2. unshare(CLONE_NEWNS | CLONE_NEWPID)
  │     3. Minimal mount layout:
  │        - /system/packages/ → bind-mount (ro)
  │        - /proc → mount proc
  │        - /dev → minimal devtmpfs (null, zero, urandom only)
  │        - /tmp → fresh tmpfs
  │     4. Install seccomp filter
  │     5. Pass listener fd to bingux-gated
  │     6. exec the patchelf'd binary
  │
  └── Dynamic mount injection:
        When user grants read access:
        1. bingux-gated responds CONTINUE to seccomp
        2. Enters sandbox mount namespace via /proc/<pid>/ns/mnt
        3. Bind-mounts the path into sandbox
        4. Future access works without prompting
```

### 3.2 — Sandbox Levels

```
"none"      → system-critical (glibc, kernel, init, bxc, bingux-gated)
"minimal"   → trusted CLI tools (coreutils, bash, git)
"standard"  → GUI apps, most userland (firefox, vscode)
"strict"    → untrusted / third-party
```

### 3.3 — Launcher Shim

Multi-call binary (like busybox). Each exported binary is a hardlink to `bxc-shim`.

Dispatch resolution: user dispatch table -> system dispatch table.

### Tasks for Phase 3

```
11. Create crate: bxc-runtime
    - Sandbox creation: mount namespace, optional pid/net namespace
    - Minimal mount layout
    - Dynamic mount injection
    - Seccomp filter installation + listener fd creation

12. Create crate: bxc-shim
    - Multi-call binary (argv[0] dispatch)
    - Layered dispatch table reader (user → system fallback)
    - @version explicit syntax
    - Sandbox level routing

13. Create crate: bpkg-resolve
    - Dependency graph resolver
    - RUNPATH computation
    - Library provider lookup
    - Conflict detection
    - Cache in /system/state/db.sqlite
```

-----

## Phase 4: System Composition (`bsys`)

### 4.1 — Two-Layer Profile Model

System profile (`/system/profiles/`): Managed by root. Base system.
User profiles (`~/.config/bingux/profiles/`): Managed by each user. Overrides + personal packages.

### 4.2 — System Profile Generation

```
bsys apply
  ├── Read DB → packages + default versions
  ├── Build generation directory (shims, symlinks, dispatch table)
  ├── Atomic swap: ln -sfn generation-43 /system/profiles/current
  └── Done.
```

### 4.3 — Per-Package Home Directories

Every package gets its own home directory per user:
```
~/.config/bingux/state/<pkg>/home/
```

This IS the package's home. The sandbox mounts it as `/users/<user>/` inside the container.

### 4.4 — Ephemeral Root

Only `/system` and `/users` persist. Everything else regenerated from system.toml on boot.

```
PERSISTENT: /system/, /users/
EPHEMERAL: /etc/, /bin, /lib, /run/, /tmp/, /proc/, /sys/, /dev/
```

### 4.5 — binguxfs Kernel Module

Hides FHS dirs from `ls /`. User sees only `/system` and `/users`. Hidden dirs still accessible by path.

### Tasks for Phase 4

```
14. Create crate: bsys-compose
    - System + user profile generation
    - Dispatch table generation
    - Atomic swap

15. Create crate: bingux-init (initramfs)
    - Mount persistent: /system, /users
    - Mount ephemeral: /etc, /run, /tmp
    - Generate /etc/ from system.toml
    - switch_root

16. Create crate: bpkg-session (user login handler)
    - Read home.toml → compose user profile
    - Dotfile linking, env generation, dconf, services

17. Create: bxc per-package home management
    - Create/manage per-package homes
    - Bind mount computation from permissions
    - Volatile cleanup

18. Create crate: bsys-config
    - system.toml → /etc/ file generation
    - Package config merge

19. Create: binguxfs kernel module
    - Filter getdents at root mountpoint only

20. Extend bpkg:
    - add/keep/unkeep lifecycle
    - pin/unpin
    - home.toml auto-update + git commit
```

-----

## Phase 5: Atomic Updates, Rollback & GC

### 5.1 — Update Flow

Install new version alongside old. Update DB default. Compose new generation. Permissions carry over (keyed by name).

### 5.2 — Rollback

```
bsys rollback [generation]
bpkg rollback [generation]
```

System and user rollbacks are independent.

### 5.3 — Garbage Collection

Volatile cleanup is free (tmpfs). `bsys gc` scans all references and deletes unreferenced packages.

### Tasks for Phase 5

```
21. Extend bpkg/bsys: upgrade, downgrade
22. Implement rollback for both layers
23. Implement bsys gc
24. Generation metadata (timestamp, package list, reason)
```

-----

## Phase 6: Self-Hosting (Dogfooding)

### 6.1 — .bgx Package Format

tar.bz2 archive of the package directory. Same commands for install from repo, scope, or file path.

### 6.2 — Bootstrap Chain

```
Stage 0: Cross-compile on any Linux host (static musl builds)
Stage 1: Self-hosted minimal (rebuild with stage0 bpkg)
Stage 2: Full system (everything built, export as .bgx)
Stage 3: ISO (bundle .bgx packages into bootable ISO)
```

### 6.3 — TUI Installer (ratatui)

Welcome → Disk partitioning → Package set selection → Install → User creation → Reboot.

### Tasks for Phase 6

```
25. Implement .bgx format (export, import, verify)
26. Bootstrap scripts (stage0, stage1, stage2)
27. Core recipes (glibc, gcc, coreutils, linux, systemd)
28. ISO builder (xorriso + systemd-boot)
29. TUI installer (ratatui)
```

-----

## Phase 7: Polish & Integration

### 7.1 — Repository System

`@scope.package` syntax. Default scope = bingux. Repos configured in system.toml / home.toml. Every repo serves `.bgx` files via index.toml.

### 7.2 — CLI Design

bpkg (user, no root), bsys (system, root), bxc (sandbox runtime). Same verb vocabulary: add, rm, keep, unkeep, upgrade, apply, rollback, history, list, info, grant, revoke.

### Tasks for Phase 7

```
30. Repository data model + index.toml
31. CLI polish (help, colours, progress bars)
32. Shell completions (bash, zsh, fish)
33. XDG portal integration
34. Man pages
35. Integration + E2E tests
```

-----

## Phase 8: Extended Home Configuration (`bpkg home`)

### 8.1 — Commands

```
bpkg home apply [path]    Converge full environment to match home.toml
bpkg home diff            Dry run — show what would change
bpkg home status          Current state vs declared state
bpkg home init            Generate home.toml from current state
bpkg home export          Bundle for offline setup
bsys home apply [path]    System-level equivalent
```

### Tasks for Phase 8

```
36. Create crate: bpkg-home
    - Extended home.toml parser (dotfiles, env, services, dconf)
    - Delta computation, dotfile linker, env.sh generator
    - bpkg home apply/diff/status/init/export
37. Integration testing (fresh install → git clone → apply → verify)
```

-----

## Repository Layout

```
bingux/
├── Cargo.toml                          # Workspace root
├── common/bingux-common/               # Path constants, PackageId, errors
├── pkg/
│   ├── bpkg-recipe/                    # BPKGBUILD parser
│   ├── bpkg-store/                     # Package store CRUD
│   ├── bpkg-resolve/                   # Dependency graph
│   ├── bpkg-patchelf/                  # ELF patching
│   ├── bpkg-build/                     # Build orchestrator
│   └── bpkg-repo/                      # Repository system
├── home/bpkg-home/                     # Declarative environment
├── sandbox/
│   ├── bxc-sandbox/                    # Seccomp filter generator
│   ├── bxc-runtime/                    # Namespace setup
│   └── bxc-shim/                       # Multi-call launcher
├── gate/
│   ├── bingux-gated/                   # Permission daemon
│   ├── bingux-prompt/                  # GTK4 permission dialog
│   └── bingux-settings/               # Permission management GUI
├── system/
│   ├── bsys-compose/                   # Generation builder
│   ├── bsys-config/                    # Config management
│   └── bingux-init/                    # Initramfs early-boot
├── cli/
│   ├── bpkg/                           # Package manager CLI
│   ├── bsys/                           # System manager CLI
│   └── bxc/                            # Container CLI
├── seccomp/                            # Filter definitions
├── recipes/                            # BPKGBUILDs
│   ├── toolchain/
│   ├── core/
│   ├── build/
│   ├── desktop/
│   └── compat/
├── bootstrap/                          # Bootstrap chain
│   ├── stage0/
│   ├── stage1/
│   └── stage2/iso/
├── testing/
│   ├── bingux-qemu-mcp/               # MCP server for QEMU
│   ├── vm/
│   ├── smoke-tests/
│   └── fixtures/
├── tests/
└── docs/
```

-----

## Key Dependencies (Rust crates)

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing |
| `serde` / `toml` | Manifest & config serialisation |
| `nix` | Linux syscall wrappers |
| `rusqlite` | Package database |
| `sha2` | File integrity hashing |
| `reqwest` | Source downloading |
| `indicatif` | Progress bars |
| `ratatui` | TUI installer |
| `gtk4-rs` / `libadwaita-rs` | GTK4 for bingux-prompt |
| `zbus` | D-Bus (pure Rust) |
| `seccompiler` | Seccomp BPF filter generation |
| `goblin` | ELF parsing for patchelf |
| `walkdir` | Filesystem traversal |
| `tempfile` | Build sandboxes |
| `tokio` | Async runtime |
| `image` | PPM→PNG (QEMU screendump) |

-----

## Implementation Sprints

**Sprint 1 — Foundation** (common/, pkg/)
1. bingux-common, bpkg-recipe, bpkg-store, bpkg-resolve, bpkg-patchelf

**Sprint 2 — Sandbox + Permission Daemon** (sandbox/, gate/)
2. bxc-sandbox, bxc-runtime, bingux-gated, bingux-prompt

**Sprint 3 — System Composition** (system/, sandbox/)
3. bsys-compose, bsys-config, bxc-shim, bingux-init

**Sprint 4 — Package Manager CLI + Updates** (cli/)
4. bpkg CLI, bsys CLI, bxc CLI

**Sprint 5 — Self-Hosting + Bootstrap** (recipes/, bootstrap/)
5. Toolchain recipes, bootstrap stages, .bgx format, ISO builder, TUI installer

**Sprint 6 — QEMU Test Infrastructure** (testing/)
6. QEMU MCP server, smoke tests, screenshot verification

**Sprint 7-9 — Package Compatibility** (recipes/compat/)
7. Small packages (hello, jq, ripgrep, curl, htop, neovim)
8. Medium packages (python, nodejs, nginx, postgresql, git, mpv, mesa)
9. Large packages (firefox, vscode, gimp, libreoffice, blender)

**Sprint 10 — Declarative Environment** (home/)
10. bpkg-home, bpkg home apply/diff/status/init/export

**Sprint 11 — Polish**
11. XDG portals, shell completions, man pages, bingux-settings, docs
