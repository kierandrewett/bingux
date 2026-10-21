# Bingux OS

An Arch Linux based live image whose desktop is Gnoblin (compositor) and
Bingux (shell, lock screen and login screen). Everything here builds inside a
stock `archlinux` container, so the host only needs podman or docker.

```text
distro/
  build.sh              host entry point: sources, then the container build
  container-build.sh    package builds and mkarchiso inside the container
  packages/             PKGBUILDs for the two packages Arch does not ship
    gnoblin/            placeholder; the PKGBUILD comes from the Gnoblin release tooling
    bingux/             this repository, including bingux-shell (vendored Quickshell)
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
- The container runs privileged so that `pacstrap` and `mkarchiso` can mount
  the chroot's `/proc`, `/sys` and `/dev`. Rootless podman works: the build
  script makes `arch-chroot` bind the container's `/dev` because a user
  namespace cannot mount devtmpfs.
- `--step packages` stops after the pacman repository in `distro/out/repo`;
  `--step iso` reuses it. `--step package:bingux` rebuilds one package against
  that repository, which is the quick loop for PKGBUILD changes.
- Expect an hour or more the first time: Mutter and bingux-shell (Quickshell
  built into the Bingux binary) compile from source. `JOBS` limits parallelism.
- Gnoblin's source bundle applies its Mutter patch series with `git am` in
  the Gnoblin checkout, so commit signing applies to it. If the signing key
  is not unlocked, run with signing disabled for that process only:
  `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=commit.gpgsign GIT_CONFIG_VALUE_0=false distro/build.sh ...`.
  A failed `git am` leaves `subprojects/mutter` in the Gnoblin checkout on
  the release tag with a staged patch; restore the pinned revision with
  `git -C subprojects/mutter reset --hard && git -C subprojects/mutter checkout <pin>`
  before the next run.
- The ISO step needs about 10 GB free under podman's storage (the container
  `/tmp` holds the live root). A full disk shows as `could not extract ...
(Write failed)` from pacstrap.

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
