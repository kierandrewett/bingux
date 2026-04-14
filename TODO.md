# Bingux v2 — TODO

## Sprint 1: Foundation

### 1.1 — Workspace Setup
- [ ] Initialize Cargo workspace with all crate stubs
- [ ] Set up workspace-level dependencies (serde, toml, clap, etc.)
- [ ] Configure workspace lints and rustfmt

### 1.2 — `common/bingux-common`
- [ ] Path constants (`/system/packages`, `/system/profiles`, `/system/state`, etc.)
- [ ] `PackageId` type: parse `"firefox-128.0.1-x86_64-linux"` ↔ `{name, version, arch}`
- [ ] `PackageId` display/serialize/deserialize
- [ ] `Scope` type for `@scope.package` syntax
- [ ] Error types (`BinguxError` enum)
- [ ] Logging setup (tracing)
- [ ] Unit tests for PackageId parsing (valid, invalid, edge cases)

### 1.3 — `pkg/bpkg-recipe`
- [ ] BPKGBUILD lexer/tokenizer (shell variable assignments, arrays, functions)
- [ ] `Recipe` struct (pkgscope, pkgname, pkgver, pkgarch, pkgdesc, license)
- [ ] Parse `depends=()` and `makedepends=()` arrays
- [ ] Parse `exports=()` array
- [ ] Parse `source=()` and `sha256sums=()` arrays
- [ ] Detect `build()` and `package()` functions (presence, not execution)
- [ ] `dlopen_hints=()` optional field
- [ ] Validation: required fields present, version format, arch format
- [ ] Unit tests: valid binary recipe, valid source recipe, malformed recipes

### 1.4 — `pkg/bpkg-store`
- [ ] `PackageStore` struct wrapping a root path
- [ ] `install(package_dir)` — move/copy package into store, verify manifest
- [ ] `remove(package_id)` — delete package directory from store
- [ ] `query(name)` — list all versions of a package
- [ ] `get(package_id)` — return package path if exists
- [ ] `list()` — enumerate all installed packages
- [ ] Manifest (`manifest.toml`) read/write with serde
- [ ] `files.txt` generation (walk dir, hash each file with SHA-256)
- [ ] `files.txt` verification (compare hashes)
- [ ] Unit tests with tempdir-based store

### 1.5 — `pkg/bpkg-resolve`
- [ ] `DependencyGraph` struct (adjacency list)
- [ ] `resolve(recipe)` — build graph from recipe's depends + transitive deps
- [ ] Topological sort for build order
- [ ] `library_provider(libname)` — scan store packages' lib/ dirs for a .so
- [ ] RUNPATH computation: package own lib + dep libs in resolution order
- [ ] Conflict detection: two packages exporting same binary/library name
- [ ] Unit tests: linear chain, diamond dependency, missing dep error, conflict

### 1.6 — `pkg/bpkg-patchelf`
- [ ] ELF magic byte detection (walk dir, identify ELF files)
- [ ] Parse ELF headers with `goblin`: read PT_INTERP, NEEDED entries, existing RUNPATH
- [ ] `compute_runpath(package_id, resolved_deps)` — build colon-separated path string
- [ ] `patch_interpreter(elf_path, new_interp)` — rewrite PT_INTERP
- [ ] `patch_runpath(elf_path, new_runpath)` — set/extend DT_RUNPATH
- [ ] Shebang rewriter: scan scripts, rewrite `#!/usr/bin/foo` → store path
- [ ] Verification pass: for each patched binary, resolve all NEEDED against RUNPATH
- [ ] Generate `patchelf.log` recording all changes
- [ ] Unit tests with small test ELF binaries (compile hello.c in test fixtures)

---

## Sprint 2: Sandbox & Permission Daemon

### 2.1 — `sandbox/bxc-sandbox`
- [ ] Permission category definitions (capability + mount types)
- [ ] Seccomp BPF filter generator: map categories → syscall numbers
- [ ] Safe syscall allow-list (brk, mmap, futex, clock_gettime, etc.)
- [ ] Sensitive syscall → `SECCOMP_RET_USER_NOTIF` mapping
- [ ] Listener fd creation via `seccomp(SECCOMP_SET_MODE_FILTER)`
- [ ] PID → package identity tracking
- [ ] Unit tests for filter generation

### 2.2 — `sandbox/bxc-runtime`
- [ ] `unshare(CLONE_NEWNS)` mount namespace creation
- [ ] Optional `CLONE_NEWPID`, `CLONE_NEWNET` for strict level
- [ ] Minimal mount layout: /system/packages (ro), /proc, /dev (minimal), /tmp
- [ ] Per-package home mount: `~/.config/bingux/state/<pkg>/home/` → `/users/<user>/`
- [ ] Dynamic mount injection: enter namespace via `/proc/<pid>/ns/mnt`, bind-mount
- [ ] Seccomp filter installation + listener fd handoff to bingux-gated
- [ ] Integration tests (requires Linux namespaces)

### 2.3 — `gate/bingux-gated`
- [ ] Seccomp listener fd event loop (epoll)
- [ ] Syscall decoder: nr + args → permission category
- [ ] UID resolution from `/proc/<pid>/status`
- [ ] Permission DB reader: load TOML from `~/.config/bingux/permissions/<pkg>.toml`
- [ ] Permission DB writer: persist grants/denials to TOML
- [ ] In-memory permission cache (avoid re-reading TOML on hot path)
- [ ] D-Bus client: send prompt request to bingux-prompt, await response
- [ ] TTY fallback prompter (when no display available)
- [ ] Permission inheritance on upgrade (keyed by name, not version)
- [ ] Unix socket for bxc-shim → gated handshake (pass listener fd)
- [ ] Integration tests with mock sandbox

### 2.4 — `gate/bingux-prompt`
- [ ] GTK4/Adwaita application skeleton
- [ ] D-Bus server: listen for prompt requests from bingux-gated
- [ ] Permission dialog UI: icon, package name, resource, path
- [ ] Three buttons: Deny / Allow Once / Always Allow
- [ ] "Dangerous" category: only Deny / Allow Once (no permanent grant)
- [ ] Timeout: 30s → auto-Deny
- [ ] Keyboard shortcuts: Enter = Allow Once, Escape = Deny
- [ ] Return response to gated via D-Bus

---

## Sprint 3: System Composition

### 3.1 — `system/bsys-compose`
- [ ] System generation builder: read DB, build symlink/shim dir
- [ ] For sandbox != "none": create bxc-shim hardlinks in bin/
- [ ] For sandbox == "none": create direct symlinks to patchelf'd binaries
- [ ] lib/ symlinks to exported shared libraries
- [ ] share/ symlinks (applications/, icons/)
- [ ] `.dispatch.toml` generation (argv[0] → package + sandbox mapping)
- [ ] Atomic swap: `rename(2)` of current symlink
- [ ] User profile generation (same logic, different paths, no root)
- [ ] Generation metadata: timestamp, package list, config hash

### 3.2 — `system/bsys-config`
- [ ] `system.toml` parser (full schema: system, packages, services, permissions, etc.)
- [ ] `/etc/` file generation from config: hostname, locale, timezone, keymap
- [ ] `/etc/passwd` + `/etc/group` generation from `/users/` directory
- [ ] systemd unit symlink generation from `[services].enable`
- [ ] Firewall rules generation from `[firewall]`
- [ ] Package config merge: store defaults + system overrides

### 3.3 — `sandbox/bxc-shim`
- [ ] Multi-call binary: detect binary name from `argv[0]`
- [ ] Dispatch table reader: user table → system table fallback
- [ ] `@version` explicit syntax: `pkg@ver` → resolve directly from store
- [ ] Sandbox level routing: none → exec, minimal/standard/strict → bxc-runtime
- [ ] UID resolution for per-user dispatch path

### 3.4 — `system/bingux-init`
- [ ] Mount persistent volumes: /system (@system), /users (@users)
- [ ] Mount ephemeral: /etc (tmpfs), /run (tmpfs), /tmp (tmpfs)
- [ ] Read `/system/config/system.toml`
- [ ] Generate all /etc/ contents from config
- [ ] Create /bin, /lib symlinks to /system/profiles/current/
- [ ] `switch_root` to systemd

---

## Sprint 4: CLI Frontends

### 4.1 — `cli/bpkg`
- [ ] Clap-based CLI skeleton with subcommands
- [ ] `bpkg add <pkg>` — volatile install (add to user profile)
- [ ] `bpkg add --keep <pkg>` — persistent install (write to home.toml + git commit)
- [ ] `bpkg rm <pkg>` — remove from user profile
- [ ] `bpkg rm --purge <pkg>` — remove + delete per-package state
- [ ] `bpkg keep <pkg>` / `bpkg unkeep <pkg>` — promote/demote volatility
- [ ] `bpkg pin <pkg>=<ver>` / `bpkg unpin <pkg>` — version pinning
- [ ] `bpkg upgrade [pkg|--all]` — upgrade user packages
- [ ] `bpkg list` — show user packages with [volatile]/[kept]/[pin] markers
- [ ] `bpkg search <query>` — search available recipes/repo index
- [ ] `bpkg info <pkg>` — package details
- [ ] `bpkg grant <pkg> <perms>` / `bpkg revoke <pkg> <perms>` — permission management
- [ ] `bpkg apply` — recompose user profile
- [ ] `bpkg rollback [generation]` — roll back user profile
- [ ] `bpkg history` — list user profile generations
- [ ] `bpkg init` — first-time user profile setup

### 4.2 — `cli/bsys`
- [ ] Clap-based CLI skeleton
- [ ] `bsys add <pkg>` / `bsys add --keep <pkg>` — system install
- [ ] `bsys rm <pkg>` — remove from system
- [ ] `bsys keep` / `bsys unkeep` (refuse for boot_essential without --force)
- [ ] `bsys build <recipe>` — build from BPKGBUILD
- [ ] `bsys upgrade [pkg|--all]` — upgrade system packages
- [ ] `bsys apply` — recompose system profile
- [ ] `bsys rollback [generation]` — roll back system profile
- [ ] `bsys history` / `bsys diff <gen1> <gen2>`
- [ ] `bsys list` / `bsys info <pkg>`
- [ ] `bsys grant` / `bsys revoke` — system service permissions
- [ ] `bsys gc` / `bsys gc --dry-run` — garbage collection

### 4.3 — `cli/bxc`
- [ ] `bxc run <pkg> [args...]` — run in sandbox
- [ ] `bxc run <pkg>@<ver> [args...]` — specific version
- [ ] `bxc shell <pkg>` — interactive shell in sandbox
- [ ] `bxc inspect <pkg>` — show sandbox config
- [ ] `bxc perms <pkg>` / `bxc perms <pkg> --reset`
- [ ] `bxc ps` — list running sandboxed processes
- [ ] `bxc ls <pkg>` — list per-package home contents

---

## Sprint 5: Self-Hosting & Bootstrap

- [ ] `.bgx` format: tar.bz2 archive creation (`bsys export`)
- [ ] `.bgx` import: `bsys add ./file.bgx` / `bpkg add ./file.bgx` detect file paths
- [ ] `.bgx` signature verification
- [ ] `bsys export --all` — batch export
- [ ] `bsys export --index <dir>` — generate index.toml from .bgx directory
- [ ] Stage 0 bootstrap scripts: cross-compile static musl builds of bpkg/bxc/bsys/gated
- [ ] Stage 1: self-hosted minimal rebuild (glibc, gcc, binutils, coreutils, bash)
- [ ] Stage 2: full system build + export all .bgx files
- [ ] Core recipes: glibc, linux, systemd, bash, coreutils, gcc, binutils, make, rust
- [ ] Toolchain recipes: bpkg, bxc, bsys, bingux-gated, bingux-prompt
- [ ] Self-rebuild verification test
- [ ] ISO builder: xorriso + systemd-boot, package set manifest
- [ ] TUI installer (ratatui): disk partitioning, package selection, user creation

---

## Sprint 6: QEMU Test Infrastructure

- [ ] `testing/bingux-qemu-mcp/` — MCP server for Claude Code ↔ QEMU
- [ ] `bingux_qemu_boot` tool: launch VM from ISO/qcow2
- [ ] `bingux_qemu_screenshot` tool: QMP screendump → PNG
- [ ] `bingux_qemu_serial_read` tool: read serial console output
- [ ] `bingux_qemu_type` / `bingux_qemu_mouse` tools: input injection
- [ ] `bingux_qemu_shell` tool: execute command via serial
- [ ] `bingux_qemu_snapshot` tool: save/restore VM state
- [ ] Smoke tests: boot check, package install, permission prompt, rollback

---

## Sprint 7-9: Package Compatibility

### Sprint 7 — Small Packages
- [ ] hello, jq, ripgrep, fd, bat (static/single-binary)
- [ ] tree, curl, sqlite, htop, tmux, neovim (small with shared libs)
- [ ] Document edge cases in docs/internals/compat-notes.md

### Sprint 8 — Medium Packages
- [ ] Python 3.12, Node.js 20 (interpreters)
- [ ] nginx, PostgreSQL (daemons)
- [ ] Git (deep dependency tree)
- [ ] mpv, Mesa (multimedia/GPU, dlopen)
- [ ] Docker/Podman (container runtime)

### Sprint 9 — Large Packages
- [ ] Firefox (flagship test — GPU, audio, network, multiprocess)
- [ ] VS Code / Code OSS (Electron, extensions, LSP)
- [ ] GIMP (GTK3, plugins)
- [ ] LibreOffice (Java, fonts, massive build)
- [ ] Blender (GPU compute, Python scripting)

---

## Sprint 10: Declarative Environment

- [ ] `home/bpkg-home` crate: extended home.toml parser
- [ ] Delta computation (packages, dotfiles, env, services, dconf)
- [ ] Dotfile linker with backup system
- [ ] env.sh generator from [env] section
- [ ] systemd user service manager from [services] section
- [ ] dconf/gsettings writer from [dconf] section
- [ ] `bpkg home apply` / `bpkg home diff` / `bpkg home status`
- [ ] `bpkg home init` — snapshot current state → generate home.toml
- [ ] `bpkg home export` — bundle for offline setup
- [ ] `bsys home apply` — system-level equivalent
- [ ] Integration test: fresh install → git clone → bpkg home apply → verify

---

## Sprint 11: Polish

- [ ] XDG portal integration in bingux-gated (FileChooser, etc.)
- [ ] Shell completions: bash, zsh, fish
- [ ] Man pages for bpkg, bsys, bxc
- [ ] `gate/bingux-settings` — permission management GUI
- [ ] E2E test suite
- [ ] Documentation: architecture.md, packaging-guide.md, permissions-model.md
- [ ] docs/internals/: patchelf.md, seccomp-unotify.md, generations.md
