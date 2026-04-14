# Packaging Guide

How to write a BPKGBUILD recipe for Bingux.

---

## Recipe Types

There are two kinds of recipe:

- **Binary recipes** download a pre-built binary and package it.  This
  is the common case -- fast, no compilation.  The package name is the
  bare name (e.g. `firefox`).
- **Source recipes** compile from source.  By convention these use a
  `-src` suffix (e.g. `firefox-src`).  They include a `build()`
  function and declare `makedepends`.

Both produce the same `.bgx` archive output.

---

## Recipe Format

A BPKGBUILD is a shell-like file.  It declares metadata as variable
assignments and provides shell functions for the build and package
phases.

### Required Fields

| Field       | Type   | Description                              |
|-------------|--------|------------------------------------------|
| `pkgscope`  | string | Repository scope (e.g. `"bingux"`)       |
| `pkgname`   | string | Package name                             |
| `pkgver`    | string | Package version                          |
| `pkgarch`   | string | Target architecture (e.g. `"x86_64-linux"`) |
| `pkgdesc`   | string | One-line description                     |
| `license`   | string | SPDX license identifier                  |
| `depends`   | array  | Runtime dependencies                     |
| `exports`   | array  | Files to expose in the profile           |
| `source`    | array  | Source URLs                              |
| `sha256sums`| array  | Checksums for sources (`"SKIP"` to skip) |

### Optional Fields

| Field          | Type  | Description                              |
|----------------|-------|------------------------------------------|
| `makedepends`  | array | Build-time-only dependencies (source)    |
| `dlopen_hints` | array | Runtime dlopen library hints             |

---

## Dependencies

### depends

Runtime dependencies.  These must already be in the package store when
the package is installed.  They are used to compute the RUNPATH for
patchelf.

```bash
depends=("glibc-2.39" "gtk3-3.24" "dbus-1.14" "pulseaudio-17.0")
```

Dependencies are specified as `name-version`.  The version is the major
version constraint -- the resolver finds the best match in the store.

### makedepends

Build-time-only dependencies (source recipes only).  These are mounted
into the build container but are not carried into the runtime package.

```bash
makedepends=("rust-1.79" "cbindgen-0.26" "nodejs-20" "nasm-2.16")
```

---

## Exports

The `exports` array lists the files that should be exposed in the
system/user profile.  Only exported files get symlinks in the profile's
`bin/`, `lib/`, `share/`, etc.

```bash
exports=(
    "bin/firefox"
    "lib/libxul.so"
    "share/applications/firefox.desktop"
    "share/icons/hicolor/128x128/apps/firefox.png"
)
```

Paths are relative to the package root.

---

## Functions

### package()

**Required for all recipes.**  Installs files into `$PKGDIR`.

For binary recipes this typically means downloading a tarball and
copying the pre-built files into the right layout:

```bash
package() {
    cd "$SRCDIR/firefox"
    mkdir -p "$PKGDIR/bin" "$PKGDIR/lib" "$PKGDIR/share/applications"
    cp -a firefox "$PKGDIR/bin/"
    cp -a *.so "$PKGDIR/lib/"
    install -Dm644 firefox.desktop "$PKGDIR/share/applications/"
}
```

### build()

**Source recipes only.**  Compiles the software in `$BUILDDIR`.  Runs
after sources are fetched and extracted to `$SRCDIR`.

```bash
build() {
    cd "$SRCDIR/firefox-${pkgver}"
    export MOZ_OBJDIR="$BUILDDIR/obj"
    ./mach configure --prefix=/
    ./mach build
}
```

### Environment Variables

| Variable    | Description                                      |
|-------------|--------------------------------------------------|
| `$SRCDIR`   | Directory where sources are extracted             |
| `$BUILDDIR` | Writable workspace for build artifacts            |
| `$PKGDIR`   | Output directory -- files here become the package |
| `$STORE`    | Path to `/system/packages/`                       |

---

## Variable Expansion

Shell variable expansion works inside string values.  The most common
use is `${pkgver}` in source URLs:

```bash
source=("https://github.com/BurntSushi/ripgrep/releases/download/${pkgver}/ripgrep-${pkgver}-x86_64-unknown-linux-musl.tar.gz")
```

---

## dlopen_hints

Some libraries are loaded at runtime via `dlopen()` and will not be
resolved through RUNPATH.  Declare hints so patchelf can handle them:

```bash
dlopen_hints=("libcuda.so=/system/packages/cuda-*/lib/")
```

---

## How Patchelf Works on the Output

After `package()` completes, the build system runs a patchelf phase on
every ELF binary and shared library in `$PKGDIR`:

1. **Scan** -- walk the package directory and identify ELF files by
   magic bytes.
2. **Analyse** -- for each binary, read the `NEEDED` entries to find
   which shared libraries it requires.
3. **Resolve** -- map each needed library to the dependency that
   provides it.
4. **Patch PT_INTERP** -- rewrite the dynamic linker path to the
   store's glibc (e.g.
   `/system/packages/glibc-2.39-x86_64-linux/lib/ld-linux-x86-64.so.2`).
5. **Patch RUNPATH** -- set to a colon-separated list: the package's
   own `lib/`, then each dependency's `lib/` in resolution order.
6. **Rewrite shebangs** -- scripts like `#!/usr/bin/python3` become
   `#!/system/packages/python-3.12-x86_64-linux/bin/python3`.
7. **Verify** -- run an `ldd`-equivalent check to confirm all symbols
   resolve.
8. **Log** -- record every patch to `.bpkg/patchelf.log`.

---

## Example: Binary Recipe (ripgrep)

```bash
# recipes/compat/small/ripgrep/BPKGBUILD

pkgscope="bingux"
pkgname="ripgrep"
pkgver="14.1.1"
pkgarch="x86_64-linux"
pkgdesc="Recursively search directories for a regex pattern"
license="MIT"

depends=("glibc-2.39")
exports=(
    "bin/rg"
)

source=("https://github.com/BurntSushi/ripgrep/releases/download/${pkgver}/ripgrep-${pkgver}-x86_64-unknown-linux-musl.tar.gz")
sha256sums=("SKIP")

package() {
    cd "$SRCDIR/ripgrep-${pkgver}-x86_64-unknown-linux-musl"

    mkdir -p "$PKGDIR/bin"
    mkdir -p "$PKGDIR/share/man/man1"
    mkdir -p "$PKGDIR/share/bash-completion/completions"
    mkdir -p "$PKGDIR/share/zsh/site-functions"

    install -m755 rg "$PKGDIR/bin/rg"
    install -m644 doc/rg.1 "$PKGDIR/share/man/man1/rg.1"
    install -m644 complete/rg.bash "$PKGDIR/share/bash-completion/completions/rg"
    install -m644 complete/_rg "$PKGDIR/share/zsh/site-functions/_rg"
}
```

---

## Example: Source Recipe (Firefox)

```bash
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

---

## Tips

- Keep `exports` minimal.  Only export what users and other packages
  need to see.
- Use `"SKIP"` for `sha256sums` during development; fill in real
  checksums before publishing.
- Binary recipes are strongly preferred for the official repo --
  compile-from-source recipes are offered as `-src` alternatives.
- Test your recipe locally with `bsys build <recipe>` before
  publishing.
