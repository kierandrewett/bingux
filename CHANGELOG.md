# Bingux Changelog

## v0.1.0 — Bootstrap Release

### Core System
- Linux kernel 6.12.8 compiled from source (musl GCC, 2.2MB)
- BusyBox 1.37.0 compiled from source (403 applets, static)
- GNU Bash 5.2.21 compiled from source (1.7MB, static)
- Dash 0.5.12 compiled from source (POSIX shell)
- GNU Make 4.4.1 compiled from source (static)
- patchelf 0.18.0 (NixOS, static)

### Libraries
- zlib 1.3.1 compiled from source (compression verified)
- ncurses 6.5 compiled from source (terminal library)
- readline 8.2 compiled from source (depends on ncurses)

### Toolchains
- musl GCC 11.2.1 (fully static, from musl.cc)
- glibc GCC 12.3 (from bootlin, packaged as BPKGBUILD)

### Package Manager (bpkg)
- add/rm/keep/unkeep/pin/unpin/upgrade
- list with volatile/kept/pinned status
- search against repo index
- info from store manifests
- grant/revoke permissions
- init (generates home.toml from current state)
- home apply/diff/status (declarative config)
- repo list/add/rm/sync
- .bgx file install support

### System Manager (bsys)
- build (full pipeline: parse → fetch → compile → patchelf → install)
- apply (compose generation with dispatch table)
- rollback (atomic generation switching)
- history/diff (generation comparison)
- gc (garbage collection with dry-run)
- export (--all + --index for repo generation)
- keep/unkeep with boot-essential protection
- add/rm with system.toml integration

### Sandbox Runtime (bxc)
- inspect (sandbox level + seccomp profile)
- perms (show/reset permissions)
- ls (per-package home contents)
- mounts (sandbox mount set display)
- run (dispatch table resolution + exec)
- shell (package-aware shell)
- ps (process listing)

### Build Pipeline
- BPKGBUILD recipe format (shell-like DSL)
- Binary recipes (download + package)
- Source recipes (build() + package())
- Multi-file C compilation
- Dependency resolution
- Automatic store PATH during builds
- patchelf integration (PT_INTERP + RUNPATH rewriting)
- .bgx archive export/import/verify
- Repository index (index.toml)

### Permission System
- Per-user per-package TOML permission files
- Capability permissions (gpu, audio, camera, network, etc.)
- Mount permissions (~/Downloads:rw syntax)
- D-Bus proxy with per-package filtering
- bingux-gated daemon architecture
- bingux-prompt dialog (GTK4-ready, TTY fallback)

### System Configuration
- system.toml → /etc/ file generation
- Init-agnostic service backend (systemd/dinit/s6)
- Two-layer profile PATH (user → system)
- Generation-based atomic updates + rollback

### Infrastructure
- 21 Rust crates in workspace
- 425+ Rust unit/integration tests
- 54+ BPKGBUILD recipes
- QEMU boot tests with automated verification
- Self-hosting validated (build + compose + dispatch)
- qcow2 persistent disk support
- CI pipeline (build → test → ISO → QEMU → self-host)

### C-Source Packages (compiled from BPKGBUILDs)
- hello-c-src, sysinfo, calc, bingux-httpd, lc
- which, env, tee, xargs, basename/dirname

### Downloaded Packages (21 real-world tools)
- jq, ripgrep, fd, bat, eza, delta, zoxide, fzf, dust
- starship, lazygit, bottom, yq, hexyl, hyperfine, sd, bandwhich
- neovim, curl, python, nodejs

### Additional Source-Compiled Packages (Bootstrap Session)
- GNU Coreutils 9.5 (106 utilities, 80s build)
- GNU grep 3.11 (25s)
- GNU sed 4.9 (21s)
- GNU gawk 5.3.1 (15s)
- GNU tar 1.35 (30s)
- GNU gzip 1.13 (13s)
- bzip2 1.0.8 (4s)
- XZ Utils 5.6.3 (14s)
- GNU patch 2.7.6 (20s)
- GNU m4 1.4.19 (30s)
- file 5.45 (8s)
- less 661 (5s)
- bc 1.07.1 (4s)
- tree 2.1.3 (2s)
- Dash 0.5.12 (5s)
- zlib 1.3.1 (4s)
- ncurses 6.5 (60s)
- readline 8.2 (7s)
- Plus 11 custom C utilities (httpd, sysinfo, calc, etc.)

### 60-Package Milestone
- Git 2.47.1 compiled from source (zlib + NO_REGEX)
- OpenSSL 3.4.1 compiled from source (145 files, 6min)
- SQLite 3.47.2 compiled from source (readline)
- Lua 5.4.7 compiled from source (readline)
- htop 3.3.0 compiled from source (ncurses)
- flex 2.6.4 compiled from source
- bison 3.8.2 compiled from source (m4)
- expat 2.6.4 compiled from source
- PCRE2 10.44 compiled from source
- libffi 3.4.6 compiled from source
- libunistring 1.3 compiled from source
- libevent 2.1.12 compiled from source
- pigz 2.8 compiled from source (parallel gzip)
- jq 1.7.1 compiled from source (oniguruma)
- mandoc 1.14.6 compiled from source
- rsync 3.4.1 compiled from source (OpenSSL)
- GNU binutils 2.43.1 compiled from source
- GNU gettext 0.22.5 compiled from source
- GNU autoconf 2.72 compiled from source
- GNU libtool 2.5.4 compiled from source
- pkgconf 2.3.0 compiled from source

### Final Session Status (75 packages)

Complete POSIX+GNU userland compiled from source:
- Kernel: Linux 6.12.8
- Shells: Bash 5.2.21, Dash 0.5.12
- Core: GNU Coreutils 9.5 (106 utils)
- Text: grep 3.11, sed 4.9, gawk 5.3.1
- Archive: tar 1.35, gzip 1.13, bzip2 1.0.8, xz 5.6.3, cpio 2.15
- Build: GCC 11.2, Make 4.4.1, binutils 2.43.1, flex 2.6.4, bison 3.8.2
- Build system: autoconf 2.72, libtool 2.5.4, m4 1.4.19, pkgconf 2.3.0
- VCS: Git 2.47.1
- Network: wget 1.24.5 (HTTPS), rsync 3.4.1, netcat
- Crypto: OpenSSL 3.4.1
- Database: SQLite 3.47.2
- Languages: Lua 5.4.7, Rust 1.84.0 (packaged)
- Libs: zlib, ncurses, readline, expat, pcre2, libffi, libunistring, libevent
- Tools: htop, jq (from source), less, file, bc, tree, pigz, mandoc, patchelf
- Utilities: hexdump, xxd, uptime, free, nproc, mktemp, printenv, watch, yes, seq
- System: sysinit, bingux-init (C), sha256sum, hostname, pkg-list

VM validated: 28/28 checks on own kernel
CI: 437 tests, ALL PASS
