# Internals: Patchelf

Technical deep dive into how Bingux patches ELF binaries to resolve
libraries directly from the package store.

---

## Why Patchelf Is Needed

Traditional Linux distributions place shared libraries in well-known
paths (`/usr/lib`, `/lib64`) and use `ldconfig` to maintain a cache.
Bingux packages each live in their own directory under
`/system/packages/`, so a Firefox binary cannot find `libc.so.6` at the
conventional path.

Rather than using containers or `LD_LIBRARY_PATH` at runtime, Bingux
rewrites the binaries themselves at install time.  This is the same
strategy Nix uses, adapted for the Bingux store layout.

---

## PT_INTERP

Every dynamically linked ELF binary has a `PT_INTERP` program header
that names the dynamic linker (e.g. `/lib64/ld-linux-x86-64.so.2`).
The kernel reads this path and loads the specified interpreter before
the program starts.

Bingux rewrites `PT_INTERP` to point at the exact glibc version in the
store:

```
Before: /lib64/ld-linux-x86-64.so.2
After:  /system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2
```

This ensures the binary always uses the correct glibc, regardless of
what (if anything) exists at `/lib64`.

---

## RUNPATH

`RUNPATH` (or the older `RPATH`) is an ELF dynamic section entry that
tells the dynamic linker where to search for shared libraries before
falling back to system defaults.

Bingux sets `RUNPATH` to a colon-separated list of store library
directories computed from the package's dependency graph.

### Computation

Given a package with `depends=("glibc-2.39" "gtk3-3.24" "zlib-1.3.1")`:

```
RUNPATH =
  /system/packages/firefox-128.0.1-x86_64-linux/lib:      # own libs first
  /system/packages/glibc-2.39-x86_64-linux/lib:            # direct dep 1
  /system/packages/gtk3-3.24-x86_64-linux/lib:             # direct dep 2
  /system/packages/zlib-1.3.1-x86_64-linux/lib:            # direct dep 3
  /system/packages/glib-2.80-x86_64-linux/lib:             # transitive (via gtk3)
  ...
```

### Resolution Order

1. **Package's own `lib/`** -- bundled libraries take priority.
2. **Direct dependencies** -- in the order declared in `depends=()`.
3. **Transitive dependencies** -- depth-first traversal of each direct
   dependency's own dependency tree.

Duplicates are removed (first occurrence wins).  This mirrors how the
dynamic linker processes `RUNPATH` entries left to right.

---

## dlopen Edge Cases

Some libraries are loaded at runtime via `dlopen("libfoo.so")` rather
than being listed as `NEEDED` entries.  These will not be found via
`RUNPATH` alone because the application passes an unqualified name.

### dlopen_hints

Recipes can declare hints:

```bash
dlopen_hints=("libcuda.so=/system/packages/cuda-*/lib/")
```

During the patchelf phase, these are used to:
1. Add the hinted directory to `RUNPATH`.
2. Optionally generate an `LD_PRELOAD` shim that intercepts `dlopen`
   and rewrites the path.

### Permission Daemon Interception

When a sandboxed process calls `open()` for a `.so` file outside its
permitted locations, the seccomp filter traps the call.
`bingux-gated` can resolve the library to the correct store path and
allow the access, or prompt the user.

---

## Shebang Rewriting

Interpreted scripts (Python, Bash, Perl, etc.) start with a shebang
line:

```
#!/usr/bin/python3
```

Bingux rewrites these to point at the dependency in the store:

```
#!/system/packages/python-3.12-x86_64-linux/bin/python3
```

The scanner identifies shebangs by checking the first two bytes of each
regular file for `#!`, then maps the interpreter path to a dependency.

---

## Verification

After patching, the patchelf phase runs a verification pass:

1. **ldd-equivalent check** -- for each patched binary, walk the
   `NEEDED` entries and confirm every library resolves to a real file
   via the computed `RUNPATH`.
2. **Symbol check** -- optionally verify that all undefined symbols in
   the binary exist in the resolved libraries.
3. **PT_INTERP existence** -- confirm the rewritten interpreter path
   exists on disk.

Verification failures are logged as warnings.  Hard failures (missing
interpreter) abort the build.

All patches are recorded in `.bpkg/patchelf.log` with before/after
values for auditing.

---

## Implementation

The `bpkg-patchelf` crate provides:

| Module     | Purpose                                          |
|------------|--------------------------------------------------|
| `scanner`  | Walk package dir, detect ELF files by magic bytes|
| `analyzer` | Read ELF headers (NEEDED, RUNPATH, PT_INTERP)   |
| `planner`  | Compute the patch plan (new RUNPATH, new interp) |
| `shebang`  | Detect and rewrite script shebangs               |
| `log`      | Generate `.bpkg/patchelf.log`                    |

The crate uses `goblin` for ELF parsing.  Actual binary modification is
done by invoking `patchelf` (the NixOS tool) or by direct ELF section
editing.
