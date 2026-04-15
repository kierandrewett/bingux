# Bingux v2 — TODO

## Completed (Sprints 1-11 + Bootstrap)

All 11 sprints from the original plan are implemented. See git log for details.
425 Rust tests passing, 412 commits, 49 BPKGBUILD recipes.

### Bootstrap milestones achieved:
- [x] Linux kernel 6.12.8 compiled from source (musl GCC)
- [x] BusyBox 1.37.0 compiled from source (403 applets)
- [x] GNU Make 4.4.1 compiled from source
- [x] patchelf 0.18.0 (static binary)
- [x] C programs compile inside QEMU VM
- [x] Multi-file C projects build (separate compilation + linking)
- [x] bsys build with build() function compiles C from BPKGBUILDs
- [x] Self-hosted ISO boots on own kernel
- [x] glibc toolchain packaged (GCC 12.3 + glibc)
- [x] patchelf rewrites ELF PT_INTERP + RUNPATH to store paths

---

## Today's Plan (until 10am BST)

### Phase A: Build more core C packages from source
- [ ] Build `sed` from source (GNU sed or a minimal implementation)
- [ ] Build `grep` from source (minimal C implementation)
- [ ] Build `tar` from source (minimal implementation or download libarchive)
- [ ] Build `which` from source (trivial C program)
- [ ] Build `env` from source (trivial C program)
- [ ] Build `tee` from source
- [ ] Build `xargs` from source (minimal)
- [ ] Build `basename`/`dirname` from source

### Phase B: Improve the kernel for full Bingux support
- [ ] Add CONFIG_VIRTIO_NET, CONFIG_E1000 to kernel (for network in VM)
- [ ] Add CONFIG_EXT4_FS, CONFIG_BTRFS_FS for real disk support
- [ ] Add CONFIG_MODULES for loadable module support
- [ ] Add CONFIG_CGROUPS for container/sandbox support
- [ ] Add CONFIG_SECCOMP for the permission system
- [ ] Rebuild kernel with all needed configs
- [ ] Test: bsys/bpkg work on the improved kernel

### Phase C: Wire patchelf into the build pipeline end-to-end
- [ ] When patchelf is in the store, automatically add to PATH during builds
- [ ] After package() completes, if binary is dynamically linked, patch it
- [ ] Test: build a dynamically-linked C program, patchelf it, run it
- [ ] Verify RUNPATH points to store dependency paths

### Phase D: Create a qcow2 persistent disk for the VM
- [ ] Create a 2GB qcow2 disk image
- [ ] Format with btrfs (@system, @users subvolumes)
- [ ] Mount in the VM init script
- [ ] Persist the package store to disk
- [ ] Test: install a package, reboot, package still there

### Phase E: Improve bsys build for real-world packages
- [ ] Download and compile `zlib` from source (real library dependency)
- [ ] Download and compile `ncurses` from source
- [ ] Download and compile `readline` from source
- [ ] Build `bash` from source using zlib + readline + ncurses
- [ ] Test bash runs inside the VM

### Phase F: Integration testing
- [ ] Run full CI pipeline (build-and-test.sh)
- [ ] Run all QEMU smoke tests
- [ ] Run bootstrap chain validation
- [ ] Verify all 425+ Rust tests pass
- [ ] Update test counts in docs

### Phase G: Documentation and cleanup
- [ ] Update PLAN.md with current status
- [ ] Clean up any remaining TODO stubs in CLI handlers
- [ ] Add CHANGELOG.md with milestone entries
- [ ] Final git push
