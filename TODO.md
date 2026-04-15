# Bingux v2 — TODO

## Sprint 1: Foundation

### 1.1 — Workspace Setup
- [x] Initialize Cargo workspace with all crate stubs
- [x] Set up workspace-level dependencies (serde, toml, clap, etc.)
- [x] Configure workspace lints and rustfmt

### 1.2 — `common/bingux-common`
- [x] Path constants (`/system/packages`, `/system/profiles`, `/system/state`, etc.)
- [x] `PackageId` type: parse `"firefox-128.0.1-x86_64-linux"` ↔ `{name, version, arch}`
- [x] `PackageId` display/serialize/deserialize
- [x] `Scope` type for `@scope.package` syntax
- [x] Error types (`BinguxError` enum)
- [x] Logging setup (tracing)
- [x] Unit tests for PackageId parsing (valid, invalid, edge cases)

### 1.3 — `pkg/bpkg-recipe`
- [x] BPKGBUILD lexer/tokenizer (shell variable assignments, arrays, functions)
- [x] `Recipe` struct (pkgscope, pkgname, pkgver, pkgarch, pkgdesc, license)
- [x] Parse `depends=()` and `makedepends=()` arrays
- [x] Parse `exports=()` array
- [x] Parse `source=()` and `sha256sums=()` arrays
- [x] Detect `build()` and `package()` functions (presence, not execution)
- [x] `dlopen_hints=()` optional field
- [x] Validation: required fields present, version format, arch format
- [x] Unit tests: valid binary recipe, valid source recipe, malformed recipes

### 1.4 — `pkg/bpkg-store`
- [x] `PackageStore` struct wrapping a root path
- [x] `install(package_dir)` — move/copy package into store, verify manifest
- [x] `remove(package_id)` — delete package directory from store
- [x] `query(name)` — list all versions of a package
- [x] `get(package_id)` — return package path if exists
- [x] `list()` — enumerate all installed packages
- [x] Manifest (`manifest.toml`) read/write with serde
- [x] `files.txt` generation (walk dir, hash each file with SHA-256)
- [x] `files.txt` verification (compare hashes)
- [x] Unit tests with tempdir-based store

### 1.5 — `pkg/bpkg-resolve`
- [x] `DependencyGraph` struct (adjacency list)
- [x] `resolve(recipe)` — build graph from recipe's depends + transitive deps
- [x] Topological sort for build order
- [x] `library_provider(libname)` — scan store packages' lib/ dirs for a .so
- [x] RUNPATH computation: package own lib + dep libs in resolution order
- [x] Conflict detection: two packages exporting same binary/library name
- [x] Unit tests: linear chain, diamond dependency, missing dep error, conflict

### 1.6 — `pkg/bpkg-patchelf`
- [x] ELF magic byte detection (walk dir, identify ELF files)
- [x] Parse ELF headers with `goblin`: read PT_INTERP, NEEDED entries, existing RUNPATH
- [x] `compute_runpath(package_id, resolved_deps)` — build colon-separated path string
- [x] `patch_interpreter(elf_path, new_interp)` — rewrite PT_INTERP
- [x] `patch_runpath(elf_path, new_runpath)` — set/extend DT_RUNPATH
- [x] Shebang rewriter: scan scripts, rewrite `#!/usr/bin/foo` → store path
- [x] Verification pass: for each patched binary, resolve all NEEDED against RUNPATH
- [x] Generate `patchelf.log` recording all changes
- [x] Unit tests with small test ELF binaries (compile hello.c in test fixtures)

---

## Sprint 2: Sandbox & Permission Daemon

### 2.1 — `sandbox/bxc-sandbox`
- [x] Permission category definitions (capability + mount types)
- [x] Seccomp BPF filter generator: map categories → syscall numbers
- [x] Safe syscall allow-list (brk, mmap, futex, clock_gettime, etc.)
- [x] Sensitive syscall → `SECCOMP_RET_USER_NOTIF` mapping
- [x] Listener fd creation via `seccomp(SECCOMP_SET_MODE_FILTER)`
- [x] PID → package identity tracking
- [x] Unit tests for filter generation

### 2.2 — `sandbox/bxc-runtime`
- [x] `unshare(CLONE_NEWNS)` mount namespace creation
- [x] Optional `CLONE_NEWPID`, `CLONE_NEWNET` for strict level
- [x] Minimal mount layout: /system/packages (ro), /proc, /dev (minimal), /tmp
- [x] Per-package home mount: `~/.config/bingux/state/<pkg>/home/` → `/users/<user>/`
- [x] Dynamic mount injection: enter namespace via `/proc/<pid>/ns/mnt`, bind-mount
- [x] Seccomp filter installation + listener fd handoff to bingux-gated
- [x] Integration tests (requires Linux namespaces)

### 2.3 — `gate/bingux-gated`
- [x] Seccomp listener fd event loop (epoll)
- [x] Syscall decoder: nr + args → permission category
- [x] UID resolution from `/proc/<pid>/status`
- [x] Permission DB reader: load TOML from `~/.config/bingux/permissions/<pkg>.toml`
- [x] Permission DB writer: persist grants/denials to TOML
- [x] In-memory permission cache (avoid re-reading TOML on hot path)
- [x] D-Bus client: send prompt request to bingux-prompt, await response
- [x] TTY fallback prompter (when no display available)
- [x] Permission inheritance on upgrade (keyed by name, not version)
- [x] Unix socket for bxc-shim → gated handshake (pass listener fd)
- [x] Integration tests with mock sandbox

### 2.4 — `gate/bingux-prompt`
- [x] GTK4/Adwaita application skeleton
- [x] D-Bus server: listen for prompt requests from bingux-gated
- [x] Permission dialog UI: icon, package name, resource, path
- [x] Three buttons: Deny / Allow Once / Always Allow
- [x] "Dangerous" category: only Deny / Allow Once (no permanent grant)
- [x] Timeout: 30s → auto-Deny
- [x] Keyboard shortcuts: Enter = Allow Once, Escape = Deny
- [x] Return response to gated via D-Bus

---

## Sprint 3: System Composition

### 3.1 — `system/bsys-compose`
- [x] System generation builder: read DB, build symlink/shim dir
- [x] For sandbox != "none": create bxc-shim hardlinks in bin/
- [x] For sandbox == "none": create direct symlinks to patchelf'd binaries
- [x] lib/ symlinks to exported shared libraries
- [x] share/ symlinks (applications/, icons/)
- [x] `.dispatch.toml` generation (argv[0] → package + sandbox mapping)
- [x] Atomic swap: `rename(2)` of current symlink
- [x] User profile generation (same logic, different paths, no root)
- [x] Generation metadata: timestamp, package list, config hash

### 3.2 — `system/bsys-config`
- [x] `system.toml` parser (full schema: system, packages, services, permissions, etc.)
- [x] `/etc/` file generation from config: hostname, locale, timezone, keymap
- [x] `/etc/passwd` + `/etc/group` generation from `/users/` directory
- [x] systemd unit symlink generation from `[services].enable`
- [x] Firewall rules generation from `[firewall]`
- [x] Package config merge: store defaults + system overrides

### 3.3 — `sandbox/bxc-shim`
- [x] Multi-call binary: detect binary name from `argv[0]`
- [x] Dispatch table reader: user table → system table fallback
- [x] `@version` explicit syntax: `pkg@ver` → resolve directly from store
- [x] Sandbox level routing: none → exec, minimal/standard/strict → bxc-runtime
- [x] UID resolution for per-user dispatch path

### 3.4 — `system/bingux-init`
- [x] Mount persistent volumes: /system (@system), /users (@users)
- [x] Mount ephemeral: /etc (tmpfs), /run (tmpfs), /tmp (tmpfs)
- [x] Read `/system/config/system.toml`
- [x] Generate all /etc/ contents from config
- [x] Create /bin, /lib symlinks to /system/profiles/current/
- [x] `switch_root` to systemd

---

## Sprint 4: CLI Frontends

### 4.1 — `cli/bpkg`
- [x] Clap-based CLI skeleton with subcommands
- [x] `bpkg add <pkg>` — volatile install (add to user profile)
- [x] `bpkg add --keep <pkg>` — persistent install (write to home.toml + git commit)
- [x] `bpkg rm <pkg>` — remove from user profile
- [x] `bpkg rm --purge <pkg>` — remove + delete per-package state
- [x] `bpkg keep <pkg>` / `bpkg unkeep <pkg>` — promote/demote volatility
- [x] `bpkg pin <pkg>=<ver>` / `bpkg unpin <pkg>` — version pinning
- [x] `bpkg upgrade [pkg|--all]` — upgrade user packages
- [x] `bpkg list` — show user packages with [volatile]/[kept]/[pin] markers
- [x] `bpkg search <query>` — search available recipes/repo index
- [x] `bpkg info <pkg>` — package details
- [x] `bpkg grant <pkg> <perms>` / `bpkg revoke <pkg> <perms>` — permission management
- [x] `bpkg apply` — recompose user profile
- [x] `bpkg rollback [generation]` — roll back user profile
- [x] `bpkg history` — list user profile generations
- [x] `bpkg init` — first-time user profile setup

### 4.2 — `cli/bsys`
- [x] Clap-based CLI skeleton
- [x] `bsys add <pkg>` / `bsys add --keep <pkg>` — system install
- [x] `bsys rm <pkg>` — remove from system
- [x] `bsys keep` / `bsys unkeep` (refuse for boot_essential without --force)
- [x] `bsys build <recipe>` — build from BPKGBUILD
- [x] `bsys upgrade [pkg|--all]` — upgrade system packages
- [x] `bsys apply` — recompose system profile
- [x] `bsys rollback [generation]` — roll back system profile
- [x] `bsys history` / `bsys diff <gen1> <gen2>`
- [x] `bsys list` / `bsys info <pkg>`
- [x] `bsys grant` / `bsys revoke` — system service permissions
- [x] `bsys gc` / `bsys gc --dry-run` — garbage collection

### 4.3 — `cli/bxc`
- [x] `bxc run <pkg> [args...]` — run in sandbox
- [x] `bxc run <pkg>@<ver> [args...]` — specific version
- [x] `bxc shell <pkg>` — interactive shell in sandbox
- [x] `bxc inspect <pkg>` — show sandbox config
- [x] `bxc perms <pkg>` / `bxc perms <pkg> --reset`
- [x] `bxc ps` — list running sandboxed processes
- [x] `bxc ls <pkg>` — list per-package home contents

---

## Sprint 5: Self-Hosting & Bootstrap

- [x] `.bgx` format: tar.bz2 archive creation (`bsys export`)
- [x] `.bgx` import: `bsys add ./file.bgx` / `bpkg add ./file.bgx` detect file paths
- [x] `.bgx` signature verification
- [x] `bsys export --all` — batch export
- [x] `bsys export --index <dir>` — generate index.toml from .bgx directory
- [x] Stage 0 bootstrap scripts: cross-compile static musl builds of bpkg/bxc/bsys/gated
- [x] Stage 1: self-hosted minimal rebuild (glibc, gcc, binutils, coreutils, bash)
- [x] Stage 2: full system build + export all .bgx files
- [x] Core recipes: glibc, linux, systemd, bash, coreutils, gcc, binutils, make, rust
- [x] Toolchain recipes: bpkg, bxc, bsys, bingux-gated, bingux-prompt
- [x] Self-rebuild verification test
- [x] ISO builder: xorriso + systemd-boot, package set manifest
- [x] TUI installer (ratatui): disk partitioning, package selection, user creation

---

## Sprint 6: QEMU Test Infrastructure

- [x] `testing/bingux-qemu-mcp/` — MCP server for Claude Code ↔ QEMU
- [x] `bingux_qemu_boot` tool: launch VM from ISO/qcow2
- [x] `bingux_qemu_screenshot` tool: QMP screendump → PNG
- [x] `bingux_qemu_serial_read` tool: read serial console output
- [x] `bingux_qemu_type` / `bingux_qemu_mouse` tools: input injection
- [x] `bingux_qemu_shell` tool: execute command via serial
- [x] `bingux_qemu_snapshot` tool: save/restore VM state
- [x] Smoke tests: boot check, package install, permission prompt, rollback

---

## Sprint 7-9: Package Compatibility

### Sprint 7 — Small Packages
- [x] hello, jq, ripgrep, fd, bat (static/single-binary)
- [x] tree, curl, sqlite, htop, tmux, neovim (small with shared libs)
- [x] Document edge cases in docs/internals/compat-notes.md

### Sprint 8 — Medium Packages
- [x] Python 3.12, Node.js 20 (interpreters)
- [x] nginx, PostgreSQL (daemons)
- [x] Git (deep dependency tree)
- [x] mpv, Mesa (multimedia/GPU, dlopen)
- [x] Docker/Podman (container runtime)

### Sprint 9 — Large Packages
- [x] Firefox (flagship test — GPU, audio, network, multiprocess)
- [x] VS Code / Code OSS (Electron, extensions, LSP)
- [x] GIMP (GTK3, plugins)
- [x] LibreOffice (Java, fonts, massive build)
- [x] Blender (GPU compute, Python scripting)

---

## Sprint 10: Declarative Environment

- [x] `home/bpkg-home` crate: extended home.toml parser
- [x] Delta computation (packages, dotfiles, env, services, dconf)
- [x] Dotfile linker with backup system
- [x] env.sh generator from [env] section
- [x] systemd user service manager from [services] section
- [x] dconf/gsettings writer from [dconf] section
- [x] `bpkg home apply` / `bpkg home diff` / `bpkg home status`
- [x] `bpkg home init` — snapshot current state → generate home.toml
- [x] `bpkg home export` — bundle for offline setup
- [x] `bsys home apply` — system-level equivalent
- [x] Integration test: fresh install → git clone → bpkg home apply → verify

---

## Sprint 11: Polish

- [x] XDG portal integration in bingux-gated (FileChooser, etc.)
- [x] Shell completions: bash, zsh, fish
- [x] Man pages for bpkg, bsys, bxc
- [x] `gate/bingux-settings` — permission management GUI
- [x] E2E test suite
- [x] Documentation: architecture.md, packaging-guide.md, permissions-model.md
- [x] docs/internals/: patchelf.md, seccomp-unotify.md, generations.md
