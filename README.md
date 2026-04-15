# Bingux

A Linux distribution where every package lives in its own isolated directory, binaries are patchelf'd to resolve libraries from the package store, and privileged operations trigger runtime permission prompts.

## Quick Start

```bash
# Build all tools
cargo build --release

# Run the demo
./scripts/demo.sh

# Build and test everything (including QEMU boot)
./scripts/build-and-test.sh

# Build a bootable ISO
./bootstrap/stage2/iso/build-production-iso.sh

# Build the self-hosted ISO (own kernel + busybox)
./bootstrap/stage2/iso/build-selfhosted-iso.sh
```

## Architecture

- **Package Store**: `/system/packages/<name>-<version>-<arch>/` — isolated directories
- **Patchelf**: Binaries have PT_INTERP + RUNPATH rewritten to point into the store
- **Sandbox**: Namespace isolation + seccomp-unotify permission prompting
- **Generations**: Atomic profile switching via symlink swap
- **Volatile by Default**: Packages are session-only unless explicitly kept

## Tools

| Tool | Purpose |
|------|---------|
| `bpkg` | User package manager (add/rm/keep/pin/search/upgrade) |
| `bsys` | System manager (build/apply/rollback/gc/export) |
| `bxc` | Sandbox runtime (run/shell/inspect/perms/ls/mounts) |

## Bootstrap Chain

```
Stage 0: musl GCC toolchain (downloaded from musl.cc)
Stage 1: busybox + GNU make (compiled from source)
Stage 2: Linux kernel 6.12.8 (compiled with musl GCC)
Stage 3: Bootable ISO with package store + generation profiles
```

## Testing

- 425+ Rust unit/integration tests
- QEMU boot tests (systemd, package lifecycle, self-hosting)
- C source compilation inside the VM
- Full CI pipeline: `./scripts/build-and-test.sh`

## Status

- Self-hosted: boots with own kernel, own busybox, own GCC
- 49 BPKGBUILD recipes (downloaded + source-compiled)
- 21 crates in the Rust workspace
- Declarative config (system.toml + home.toml)
- Permission system with D-Bus proxy
- Init-agnostic service backend (systemd/dinit/s6)
