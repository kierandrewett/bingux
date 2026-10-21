# Bingux OS

An Arch Linux based live image whose desktop is Gnoblin (compositor) and
Bingux (shell, lock screen and login screen). Everything here builds inside a
stock `archlinux` container, so the host only needs podman or docker.

```text
distro/
  build.sh              host entry point: sources, then the container build
  container-build.sh    package builds and mkarchiso inside the container
  packages/             PKGBUILDs for the three packages Arch does not ship
    quickshell/         Quickshell 0.2.1 with the Bingux runtime patches
    gnoblin/            placeholder; the PKGBUILD comes from the Gnoblin release tooling
    bingux/             this repository
  profile/              archiso overlay on top of the releng profile
  sources/              generated inputs (ignored)
  out/                  packages, repository and the ISO (ignored)
```

## Build

```sh
distro/build.sh --gnoblin ~/dev/gnoblin
```

- Both checkouts must be committed: the Bingux archive comes from
  `scripts/make-source-archive.sh` and the Gnoblin bundle from Gnoblin's
  `scripts/build-release-assets.sh --source-only`, which archive `HEAD`.
- The container runs privileged because `mkarchiso` needs loop devices.
- `--step packages` stops after the pacman repository in `distro/out/repo`;
  `--step iso` reuses it.
- Expect an hour or more the first time: Mutter and Quickshell compile from
  source. `JOBS` limits parallelism.

The image lands in `distro/out/bingux-os-<date>-x86_64.iso`.

## What the image does

- `greetd` is the display manager; it starts `bingux-greeter-session`, so the
  first thing on screen is the Bingux login screen on a Gnoblin greeter
  compositor. The account is `live` with password `live`, created by
  `bingux-os-firstboot.service` before greetd starts, together with its
  greeter wallpaper snapshot.
- Signing in starts a Gnoblin session with the Bingux shell. New accounts get
  `~/.config/gnoblin/init.lua` from `/etc/skel`, which loads
  `/usr/share/gnoblin/conf.d/*.lua` (the Bingux integration) and the user's
  own `conf.d/`.
- `archinstall` is included for installing to disk. Create the user with a
  password there; the installed system uses the same greetd configuration.
- The default wallpaper is generated at build time
  (`/usr/share/backgrounds/bingux-os/default.jpg`), so no picture is checked
  in. Replace it in `container-build.sh` or ship your own.

## Boot test

```sh
qemu-system-x86_64 -enable-kvm -m 4G -smp 4 \
    -device virtio-vga-gl -display gtk,gl=on \
    -cdrom distro/out/bingux-os-*.iso
```

The login screen must appear with the `live` account selected; `live` signs
in to the Bingux desktop. `tests/distro-boot.py` automates the first check in
a headless QEMU with a serial console and a VNC screenshot.
