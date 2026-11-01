#!/usr/bin/env bash
# Install what scripts/build-prefix.sh needs, on Fedora, Debian or Arch. Run it as root in the
# build container or on the build machine:
#
#   scripts/install-build-deps.sh fedora|debian|arch
#
# One list per distribution, here and nowhere else: CI and a local container build read the same
# lines. The programs use `if let` chains, which need Rust 1.88 (Cargo.toml says 1.85, which the code does
# not meet). Fedora 43 and Arch ship a newer Rust; Debian 13 ships 1.85, so Rust comes from
# trixie-backports there.
set -euo pipefail

distro="${1:-}"

case "$distro" in
    fedora)
        # xdg-utils and wl-clipboard are runtime tools: the installer writes their paths into the
        # search configuration, so they must be present when the prefix is installed.
        dnf -y --setopt=install_weak_deps=False install \
            appstream cargo clang cmake cli11-devel curl desktop-file-utils gcc gcc-c++ git glib2-devel \
            gstreamer1-devel gstreamer1-plugins-base-devel gtk4-devel jemalloc-devel libadwaita-devel \
            libarchive-devel libdrm-devel libei-devel libinput-devel libseat-devel libxcb-devel \
            libxkbcommon-devel make mesa-libEGL-devel mesa-libGL-devel mesa-libgbm-devel ninja-build \
            pam-devel pipewire-devel pkgconf-pkg-config poppler-glib-devel polkit-devel \
            pulseaudio-libs-devel python3 qt6-qtbase-devel qt6-qtbase-private-devel \
            qt6-qtdeclarative-devel qt6-qtshadertools-devel \
            qt6-qtsvg-devel qt6-qttools-devel qt6-qtwayland-devel rust \
            spirv-tools systemd-devel util-linux wayland-devel wayland-protocols-devel wl-clipboard \
            cairo-devel fontconfig-devel freetype-devel xdg-utils binutils patch tar xz
        dnf clean all
        ;;
    debian)
        export DEBIAN_FRONTEND=noninteractive
        echo "deb http://deb.debian.org/debian trixie-backports main" >/etc/apt/sources.list.d/trixie-backports.list
        apt-get update -qq
        # curl comes from backports too: cargo needs the newer libcurl there, and curl of trixie wants the
        # libcurl of trixie. skia-bindings runs curl to download its prebuilt Skia archive.
        apt-get install -y -qq --no-install-recommends -t trixie-backports rustc cargo curl ca-certificates
        apt-get install -y -qq --no-install-recommends \
            appstream binutils build-essential clang cmake desktop-file-utils git \
            libadwaita-1-dev libarchive-dev libcairo2-dev libcli11-dev libclang-dev libdrm-dev libegl-dev \
            libei-dev libfontconfig-dev libfreetype-dev libgbm-dev libgl-dev libglib2.0-dev \
            libgstreamer-plugins-base1.0-dev libgstreamer1.0-dev libgtk-4-dev libinput-dev \
            libjemalloc-dev libpam0g-dev libpipewire-0.3-dev libpoppler-glib-dev libpolkit-agent-1-dev \
            libpulse-dev libseat-dev libsystemd-dev libudev-dev libwayland-dev libxcb1-dev libxkbcommon-dev ninja-build \
            pkgconf python3 qmake6 qt6-base-dev qt6-base-private-dev qt6-declarative-dev \
            qt6-declarative-private-dev qt6-shadertools-dev qt6-svg-dev qt6-tools-dev \
            qt6-tools-dev-tools qt6-wayland-dev qt6-wayland-private-dev spirv-tools util-linux \
            wayland-protocols wl-clipboard xdg-utils xz-utils
        apt-get clean
        ;;
    arch)
        pacman -Syu --noconfirm --needed \
            appstream base-devel cargo clang cli11 cmake curl desktop-file-utils git glib2 gst-plugins-base \
            gstreamer gtk4 jemalloc libadwaita libarchive libdrm libei libinput libpulse \
            libxcb libxkbcommon mesa ninja pam pipewire pkgconf poppler-glib polkit python qt6-base \
            qt6-declarative qt6-shadertools qt6-svg qt6-tools qt6-wayland seatd spirv-tools \
            systemd-libs util-linux wayland wayland-protocols wl-clipboard xdg-utils cairo fontconfig \
            freetype2 xz
        pacman -Scc --noconfirm
        ;;
    *)
        echo "usage: scripts/install-build-deps.sh fedora|debian|arch" >&2
        exit 2
        ;;
esac
