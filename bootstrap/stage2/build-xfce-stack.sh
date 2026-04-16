#!/bin/bash
# Build the complete GTK3 + XFCE stack for Bingux
#
# This builds ~36 packages in dependency order using host tools.
# Packages are installed to /tmp/bingux-bootstrap-store/<name>-<ver>-x86_64-linux/
#
# Usage: bash build-xfce-stack.sh [start-from-pkg-number]

set -euo pipefail

STORE="/tmp/bingux-bootstrap-store"
CACHE="/tmp/bingux-bootstrap-cache"
BUILD="/tmp/bingux-xfce-build"
JOBS="$(nproc)"

mkdir -p "$CACHE" "$BUILD"

# ── Helper functions ─────────────────────────────────────────────────

download() {
    local url="$1"
    local fname="${url##*/}"
    local dest="$CACHE/$fname"
    if [ ! -f "$dest" ]; then
        echo "  Downloading $fname..." >&2
        /usr/bin/curl -fSL --retry 3 -o "$dest" "$url"
    fi
    printf '%s' "$dest"
}

extract() {
    local archive="$1" dir="$BUILD/$2"
    /usr/bin/rm -rf "$dir"
    /usr/bin/mkdir -p "$dir"
    /usr/bin/tar xf "$archive" -C "$dir" --strip-components=1
    echo "$dir"
}

# Collect all pkg-config paths from the store
make_pkgconfig_path() {
    local paths=""
    for d in "$STORE"/*/lib/pkgconfig "$STORE"/*/lib64/pkgconfig "$STORE"/*/share/pkgconfig; do
        [ -d "$d" ] && paths="${paths:+$paths:}$d"
    done
    echo "$paths"
}

# Common environment for all builds
setup_env() {
    export PKG_CONFIG_PATH="$(make_pkgconfig_path)"
    export CFLAGS="-O2 -pipe"
    export CXXFLAGS="-O2 -pipe"
    # Ensure store libs are found before host libs for autotools builds
    local ldpaths=""
    for d in "$STORE"/glib-src-*/lib "$STORE"/cairo-src-*/lib "$STORE"/pango-src-*/lib \
             "$STORE"/gdk-pixbuf-src-*/lib "$STORE"/gtk3-src-*/lib \
             "$STORE"/harfbuzz-src-*/lib "$STORE"/freetype-shared-src-*/lib \
             "$STORE"/fontconfig-shared-src-*/lib "$STORE"/fribidi-src-*/lib \
             "$STORE"/libepoxy-src-*/lib "$STORE"/at-spi2-core-src-*/lib \
             "$STORE"/libxfce4util-src-*/lib "$STORE"/xfconf-src-*/lib \
             "$STORE"/libxfce4ui-src-*/lib "$STORE"/garcon-src-*/lib \
             "$STORE"/exo-src-*/lib "$STORE"/libnotify-src-*/lib \
             "$STORE"/libgudev-src-*/lib "$STORE"/vte-src-*/lib \
             "$STORE"/graphene-src-*/lib "$STORE"/pixman-glibc-*/lib \
             "$STORE"/wayland-src-*/lib "$STORE"/xkbcommon-src-*/lib \
             "$STORE"/libpng-glibc-*/lib "$STORE"/libffi-glibc-*/lib \
             "$STORE"/zlib-glibc-*/lib "$STORE"/expat-glibc-*/lib \
             "$STORE"/dbus-src-*/lib; do
        [ -d "$d" ] && ldpaths="${ldpaths:+$ldpaths }-L$d"
    done
    # Add both -L and -Wl,-rpath-link so transitive deps resolve from store
    local rpaths=""
    for d in "$STORE"/glib-src-*/lib "$STORE"/cairo-src-*/lib "$STORE"/pango-src-*/lib \
             "$STORE"/gdk-pixbuf-src-*/lib "$STORE"/gtk3-src-*/lib \
             "$STORE"/harfbuzz-src-*/lib "$STORE"/freetype-shared-src-*/lib \
             "$STORE"/fontconfig-shared-src-*/lib "$STORE"/fribidi-src-*/lib \
             "$STORE"/libepoxy-src-*/lib "$STORE"/at-spi2-core-src-*/lib \
             "$STORE"/libxfce4util-src-*/lib "$STORE"/xfconf-src-*/lib \
             "$STORE"/libxfce4ui-src-*/lib "$STORE"/garcon-src-*/lib \
             "$STORE"/exo-src-*/lib "$STORE"/libnotify-src-*/lib \
             "$STORE"/vte-src-*/lib "$STORE"/graphene-src-*/lib \
             "$STORE"/pixman-glibc-*/lib "$STORE"/wayland-src-*/lib \
             "$STORE"/xkbcommon-src-*/lib "$STORE"/libpng-glibc-*/lib \
             "$STORE"/libffi-glibc-*/lib "$STORE"/zlib-glibc-*/lib; do
        [ -d "$d" ] && rpaths="${rpaths:+$rpaths }-Wl,-rpath-link,$d"
    done
    export LDFLAGS="$ldpaths $rpaths"
    unset LD_LIBRARY_PATH
    # Use host tools only — store binaries are patchelf'd for Bingux glibc
    # Dependencies are found via pkg-config, not blanket -I/-L flags
    export PATH="/usr/bin:/usr/sbin:/usr/local/bin:/home/kieran/.local/bin"
    export CC=gcc
    export CXX=g++
    # Prevent pkg-config from finding host system libs
    export PKG_CONFIG_LIBDIR=""
}

pkg_installed() {
    /usr/bin/test -d "$STORE/$1"
}

meson_build() {
    local srcdir="$1" prefix="$2"
    shift 2
    meson setup _build "$srcdir" --prefix="$prefix" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        "$@" 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
}

autotools_build() {
    local srcdir="$1" prefix="$2"
    shift 2
    cd "$srcdir"
    [ -x configure ] || autoreconf -fi 2>&1 | tail -5
    ./configure --prefix="$prefix" "$@" 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
}

START_FROM="${1:-1}"
PKG_NUM=0

build_pkg() {
    PKG_NUM=$((PKG_NUM + 1))
    local name="$1" ver="$2"
    local pkgid="${name}-src-${ver}-x86_64-linux"

    if [ "$PKG_NUM" -lt "$START_FROM" ]; then
        echo "[$PKG_NUM] SKIP $name-$ver (before start point)"
        return 0
    fi

    if pkg_installed "$pkgid"; then
        echo "[$PKG_NUM] ALREADY BUILT: $name-$ver"
        return 0
    fi

    echo ""
    echo "================================================================"
    echo "[$PKG_NUM] Building: $name $ver"
    echo "================================================================"

    setup_env
}

# ── TIER 1: Base libraries ───────────────────────────────────────────

# 1. GLib
build_pkg "glib" "2.82.4"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "glib-src-2.82.4-x86_64-linux"; then
    DEST="$STORE/glib-src-2.82.4-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/glib/2.82/glib-2.82.4.tar.xz)" glib)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dtests=false -Dintrospection=disabled \
        -Dlibmount=disabled -Dman-pages=disabled \
        -Ddtrace=false -Dsystemtap=false \
        -Dglib_debug=disabled 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed glib to $DEST"
fi

# 2. shared-mime-info
build_pkg "shared-mime-info" "2.4"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "shared-mime-info-src-2.4-x86_64-linux"; then
    DEST="$STORE/shared-mime-info-src-2.4-x86_64-linux"
    SRC=$(extract "$(download https://gitlab.freedesktop.org/xdg/shared-mime-info/-/archive/2.4/shared-mime-info-2.4.tar.gz)" shared-mime-info)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Dupdate-mimedb=false 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed shared-mime-info"
fi

# 3. gobject-introspection (needed by some packages, skip for now - use -Dintrospection=disabled)

# 4. fribidi
build_pkg "fribidi" "1.0.16"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "fribidi-src-1.0.16-x86_64-linux"; then
    DEST="$STORE/fribidi-src-1.0.16-x86_64-linux"
    SRC=$(extract "$(download https://github.com/fribidi/fribidi/releases/download/v1.0.16/fribidi-1.0.16.tar.xz)" fribidi)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dtests=false -Ddocs=false 2>&1 | tail -10
    ninja -C _build -j"$JOBS" 2>&1 | tail -5
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed fribidi"
fi

# 5. harfbuzz (against existing freetype, without harfbuzz initially)
build_pkg "harfbuzz" "10.1.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "harfbuzz-src-10.1.0-x86_64-linux"; then
    DEST="$STORE/harfbuzz-src-10.1.0-x86_64-linux"
    SRC=$(extract "$(download https://github.com/harfbuzz/harfbuzz/releases/download/10.1.0/harfbuzz-10.1.0.tar.xz)" harfbuzz)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dglib=enabled -Dfreetype=disabled \
        -Dcairo=disabled -Dicu=disabled \
        -Dgobject=disabled -Dintrospection=disabled \
        -Dtests=disabled -Ddocs=disabled 2>&1 | tail -10
    ninja -C _build -j"$JOBS" 2>&1 | tail -5
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed harfbuzz"
fi

# 6. Rebuild freetype with harfbuzz (shared)
build_pkg "freetype-shared" "2.13.3"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "freetype-shared-src-2.13.3-x86_64-linux"; then
    DEST="$STORE/freetype-shared-src-2.13.3-x86_64-linux"
    SRC=$(extract "$(download https://download.savannah.gnu.org/releases/freetype/freetype-2.13.3.tar.xz)" freetype-shared)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --enable-shared --disable-static \
        --with-zlib=yes --with-bzip2=yes --with-png=yes \
        --with-harfbuzz=yes --without-brotli 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed freetype (shared + harfbuzz)"
fi

# 6b. Rebuild harfbuzz WITH freetype (now that freetype-shared exists)
echo "  [6b] Rebuilding harfbuzz with freetype..."
DEST="$STORE/harfbuzz-src-10.1.0-x86_64-linux"
SRC=$(extract "$(download https://github.com/harfbuzz/harfbuzz/releases/download/10.1.0/harfbuzz-10.1.0.tar.xz)" harfbuzz-rebuild)
cd "$BUILD"
rm -rf _build
setup_env
meson setup _build "$SRC" --prefix="$DEST" \
    --libdir=lib --buildtype=release \
    -Ddefault_library=shared \
    -Dglib=enabled -Dfreetype=enabled \
    -Dcairo=disabled -Dicu=disabled \
    -Dgobject=disabled -Dintrospection=disabled \
    -Dtests=disabled -Ddocs=disabled 2>&1 | tail -10
ninja -C _build -j"$JOBS" 2>&1 | tail -5
DESTDIR="" ninja -C _build install 2>&1 | tail -5
echo "  -> Rebuilt harfbuzz with freetype"

# 7. Rebuild fontconfig (shared)
build_pkg "fontconfig-shared" "2.15.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "fontconfig-shared-src-2.15.0-x86_64-linux"; then
    DEST="$STORE/fontconfig-shared-src-2.15.0-x86_64-linux"
    SRC=$(extract "$(download https://www.freedesktop.org/software/fontconfig/release/fontconfig-2.15.0.tar.xz)" fontconfig-shared)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --enable-shared --disable-static \
        --disable-docs --disable-cache-build 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install RUN_FC_CACHE_TEST=false 2>&1 | tail -5
    echo "  -> Installed fontconfig (shared)"
fi

# 8. cairo
build_pkg "cairo" "1.18.2"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "cairo-src-1.18.2-x86_64-linux"; then
    DEST="$STORE/cairo-src-1.18.2-x86_64-linux"
    SRC=$(extract "$(download https://cairographics.org/releases/cairo-1.18.2.tar.xz)" cairo)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dtests=disabled -Dgtk_doc=false \
        -Dspectre=disabled -Dsymbol-lookup=disabled 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed cairo"
fi

# 9. pango
build_pkg "pango" "1.54.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "pango-src-1.54.0-x86_64-linux"; then
    DEST="$STORE/pango-src-1.54.0-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/pango/1.54/pango-1.54.0.tar.xz)" pango)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dintrospection=disabled \
        -Dgtk_doc=false 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed pango"
fi

# 10. gdk-pixbuf
build_pkg "gdk-pixbuf" "2.42.12"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "gdk-pixbuf-src-2.42.12-x86_64-linux"; then
    DEST="$STORE/gdk-pixbuf-src-2.42.12-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/gdk-pixbuf/2.42/gdk-pixbuf-2.42.12.tar.xz)" gdk-pixbuf)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dintrospection=disabled \
        -Dgtk_doc=false -Dman=false \
        -Dtests=false -Dinstalled_tests=false \
        -Dbuiltin_loaders=all 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed gdk-pixbuf"
fi

# 11. libepoxy
build_pkg "libepoxy" "1.5.10"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "libepoxy-src-1.5.10-x86_64-linux"; then
    DEST="$STORE/libepoxy-src-1.5.10-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/libepoxy/1.5/libepoxy-1.5.10.tar.xz)" libepoxy)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dtests=false -Dx11=false \
        -Degl=yes 2>&1 | tail -10
    ninja -C _build -j"$JOBS" 2>&1 | tail -5
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed libepoxy"
fi

# 12. graphene
build_pkg "graphene" "1.10.8"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "graphene-src-1.10.8-x86_64-linux"; then
    DEST="$STORE/graphene-src-1.10.8-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/graphene/1.10/graphene-1.10.8.tar.xz)" graphene)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dtests=false -Dinstalled_tests=false \
        -Dintrospection=disabled \
        -Dgobject_types=true 2>&1 | tail -10
    ninja -C _build -j"$JOBS" 2>&1 | tail -5
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed graphene"
fi

# 13. at-spi2-core
build_pkg "at-spi2-core" "2.54.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "at-spi2-core-src-2.54.0-x86_64-linux"; then
    DEST="$STORE/at-spi2-core-src-2.54.0-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/at-spi2-core/2.54/at-spi2-core-2.54.0.tar.xz)" at-spi2-core)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dintrospection=disabled \
        -Dx11=disabled -Ddocs=false \
        -Dsystemd_user_dir=/tmp/unused 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed at-spi2-core"
fi

# 14-16: icon themes and iso-codes (data packages)
build_pkg "hicolor-icon-theme" "0.17"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "hicolor-icon-theme-src-0.17-x86_64-linux"; then
    DEST="$STORE/hicolor-icon-theme-src-0.17-x86_64-linux"
    SRC=$(extract "$(download https://icon-theme.freedesktop.org/releases/hicolor-icon-theme-0.17.tar.xz)" hicolor-icon-theme)
    cd "$SRC"
    ./configure --prefix="$DEST" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed hicolor-icon-theme"
fi

build_pkg "iso-codes" "4.17.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "iso-codes-src-4.17.0-x86_64-linux"; then
    DEST="$STORE/iso-codes-src-4.17.0-x86_64-linux"
    SRC=$(extract "$(download https://salsa.debian.org/iso-codes-team/iso-codes/-/archive/v4.17.0/iso-codes-v4.17.0.tar.gz)" iso-codes)
    cd "$SRC"
    # iso-codes is data-only, just copy
    mkdir -p "$DEST/share/iso-codes/json" "$DEST/share/pkgconfig"
    cp data/*.json "$DEST/share/iso-codes/json/" 2>/dev/null || cp iso_*.json "$DEST/share/iso-codes/json/" 2>/dev/null || true
    # Create a pkgconfig file
    cat > "$DEST/share/pkgconfig/iso-codes.pc" <<PCEOF
prefix=$DEST
datarootdir=\${prefix}/share
datadir=\${datarootdir}
Name: iso-codes
Description: ISO code lists and translations
Version: 4.17.0
PCEOF
    echo "  -> Installed iso-codes"
fi

# ── TIER 2: GTK3 ────────────────────────────────────────────────────

# 17. GTK3
build_pkg "gtk3" "3.24.43"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "gtk3-src-3.24.43-x86_64-linux"; then
    DEST="$STORE/gtk3-src-3.24.43-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/gtk+/3.24/gtk+-3.24.43.tar.xz)" gtk3)
    cd "$BUILD"
    rm -rf _build
    setup_env
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dx11_backend=false \
        -Dwayland_backend=true \
        -Dbroadway_backend=false \
        -Dintrospection=false \
        -Ddemos=false -Dtests=false \
        -Dexamples=false -Dgtk_doc=false \
        -Dman=false \
        -Dcolord=no \
        -Dprint_backends=file 2>&1 | tail -30
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed GTK3 (Wayland-only)"
fi

# 18. libnotify
build_pkg "libnotify" "0.8.3"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "libnotify-src-0.8.3-x86_64-linux"; then
    DEST="$STORE/libnotify-src-0.8.3-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/libnotify/0.8/libnotify-0.8.3.tar.xz)" libnotify)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dintrospection=disabled \
        -Dgtk_doc=false -Dtests=false \
        -Dman=false -Ddocbook_docs=disabled 2>&1 | tail -10
    ninja -C _build -j"$JOBS" 2>&1 | tail -5
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed libnotify"
fi

# 19. libgudev
build_pkg "libgudev" "238"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "libgudev-src-238-x86_64-linux"; then
    DEST="$STORE/libgudev-src-238-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/libgudev/238/libgudev-238.tar.xz)" libgudev)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dintrospection=disabled \
        -Dtests=disabled -Dvapi=disabled 2>&1 | tail -10
    ninja -C _build -j"$JOBS" 2>&1 | tail -5
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed libgudev"
fi

# 20. vte (for xfce4-terminal)
build_pkg "vte" "0.74.2"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "vte-src-0.74.2-x86_64-linux"; then
    DEST="$STORE/vte-src-0.74.2-x86_64-linux"
    SRC=$(extract "$(download https://download.gnome.org/sources/vte/0.74/vte-0.74.2.tar.xz)" vte)
    cd "$BUILD"
    rm -rf _build
    meson setup _build "$SRC" --prefix="$DEST" \
        --libdir=lib --buildtype=release \
        -Ddefault_library=shared \
        -Dgir=false \
        -Dgtk3=true -Dgtk4=false \
        -Ddocs=false -Dvapi=false \
        -Dgnutls=false -Dicu=false \
        -D_systemd=false 2>&1 | tail -20
    ninja -C _build -j"$JOBS" 2>&1 | tail -10
    DESTDIR="" ninja -C _build install 2>&1 | tail -5
    echo "  -> Installed vte"
fi

# ── TIER 3: XFCE Core Libraries ─────────────────────────────────────

# 21. libxfce4util
build_pkg "libxfce4util" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "libxfce4util-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/libxfce4util-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/libxfce4util/4.20/libxfce4util-4.20.0.tar.bz2)" libxfce4util)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-introspection --disable-vala 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed libxfce4util"
fi

# 22. xfconf
build_pkg "xfconf" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "xfconf-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/xfconf-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/xfconf/4.20/xfconf-4.20.0.tar.bz2)" xfconf)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-introspection --disable-vala \
        --disable-gsettings-backend 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed xfconf"
fi

# 23. libxfce4ui
build_pkg "libxfce4ui" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "libxfce4ui-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/libxfce4ui-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/libxfce4ui/4.20/libxfce4ui-4.20.0.tar.bz2)" libxfce4ui)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-introspection --disable-vala \
        --disable-x11 --enable-wayland \
        --disable-tests --disable-gtk-doc 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed libxfce4ui"
fi

# 24. garcon
build_pkg "garcon" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "garcon-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/garcon-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/garcon/4.20/garcon-4.20.0.tar.bz2)" garcon)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-introspection 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed garcon"
fi

# 25. exo
build_pkg "exo" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "exo-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/exo-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/exo/4.20/exo-4.20.0.tar.bz2)" exo)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed exo"
fi

# ── TIER 4: XFCE Applications ───────────────────────────────────────

# 26. thunar
build_pkg "thunar" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "thunar-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/thunar-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/thunar/4.20/thunar-4.20.0.tar.bz2)" thunar)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-introspection --disable-x11 --enable-wayland 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed thunar"
fi

# 27. xfce4-panel
build_pkg "xfce4-panel" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "xfce4-panel-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/xfce4-panel-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/xfce4-panel/4.20/xfce4-panel-4.20.0.tar.bz2)" xfce4-panel)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-introspection --disable-vala \
        --disable-x11 --enable-wayland 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed xfce4-panel"
fi

# 28. xfce4-settings
build_pkg "xfce4-settings" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "xfce4-settings-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/xfce4-settings-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/xfce4-settings/4.20/xfce4-settings-4.20.0.tar.bz2)" xfce4-settings)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-x11 --enable-wayland \
        --disable-xrandr --disable-xcursor 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed xfce4-settings"
fi

# 29. xfce4-session
build_pkg "xfce4-session" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "xfce4-session-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/xfce4-session-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/xfce4-session/4.20/xfce4-session-4.20.0.tar.bz2)" xfce4-session)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-x11 --enable-wayland \
        --disable-polkit 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed xfce4-session"
fi

# 30. xfdesktop
build_pkg "xfdesktop" "4.20.0"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "xfdesktop-src-4.20.0-x86_64-linux"; then
    DEST="$STORE/xfdesktop-src-4.20.0-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/xfce/xfdesktop/4.20/xfdesktop-4.20.0.tar.bz2)" xfdesktop)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-x11 --enable-wayland \
        --disable-desktop-icons --disable-notifications 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed xfdesktop"
fi

# 31. xfce4-terminal
build_pkg "xfce4-terminal" "1.1.3"
if [ "$PKG_NUM" -ge "$START_FROM" ] && ! pkg_installed "xfce4-terminal-src-1.1.3-x86_64-linux"; then
    DEST="$STORE/xfce4-terminal-src-1.1.3-x86_64-linux"
    SRC=$(extract "$(download https://archive.xfce.org/src/apps/xfce4-terminal/1.1/xfce4-terminal-1.1.3.tar.bz2)" xfce4-terminal)
    cd "$SRC"
    setup_env
    ./configure --prefix="$DEST" --disable-static --disable-debug \
        --disable-x11 --enable-wayland 2>&1 | tail -10
    make -j"$JOBS" 2>&1 | tail -5
    make install 2>&1 | tail -5
    echo "  -> Installed xfce4-terminal"
fi

echo ""
echo "================================================================"
echo "  XFCE Stack Build Complete!"
echo "================================================================"
echo ""
echo "Packages built: $PKG_NUM"
echo "Store: $STORE"
