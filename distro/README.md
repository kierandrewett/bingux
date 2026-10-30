# Bingux OS

A live image of Bingux OS: Arch Linux packages underneath, with Gnoblin
Bingux (shell, lock screen and login screen). Everything here builds inside a
stock `archlinux` container, so the host only needs podman or docker.

```text
distro/
  build.sh              host entry point: sources, then the container build
  container-build.sh    package builds and mkarchiso inside the container
  publish-repo.sh       uploads the pacman repository to a GitHub release
  packages/             PKGBUILDs for the packages Arch does not ship
    gnoblin/            placeholder; the PKGBUILD comes from the Gnoblin release tooling
    bingux/             this repository, including bingux-shell (vendored Quickshell)
    bingux-os/          image defaults: login screen, wallpaper, skel, repository
  profile/              archiso overlay on top of the releng profile
  sources/              generated inputs (ignored)
  out/                  packages, repository and the ISO (ignored)
```

## Build

```sh
distro/build.sh --gnoblin ~/dev/gnoblin
```

- The Gnoblin checkout must contain greeter mode (`gnoblin --greeter`), which
  is on the `feat/greeter-mode` branch until it reaches Gnoblin's `main`, and
  the session-lock fixes (`a05f29ae`, `2317ff42`, `4f33ee52`): without them the
  lock screen stays black and unlocking crashes the compositor.
- Both checkouts must be committed: the Bingux archive comes from
  `scripts/make-source-archive.sh` and the Gnoblin bundle from Gnoblin's
  `scripts/build-release-assets.sh --source-only`, which archive `HEAD`.
  `make-source-archive.sh` refuses a Bingux checkout with uncommitted tracked
  edits. `BINGUX_ARCHIVE_FROM_HEAD=1` overrides that: it archives `HEAD`, ignores
  the edits, and prints a warning with the commit id. Use it only when the
  edits belong to other work in a shared checkout; the built image then matches
  `HEAD`, not the working tree. The submodule pin check still applies.
- The container runs privileged so that `pacstrap` and `mkarchiso` can mount
  the chroot's `/proc`, `/sys` and `/dev`. Rootless podman works: the build
  script makes `arch-chroot` bind the container's `/dev` because a user
  namespace cannot mount devtmpfs.
- `--step packages` stops after the pacman repository in `distro/out/repo`;
  `--step iso` reuses it. `--step package:NAME` (`package:bsm` regenerates the Bingux archive like `package:bingux`) rebuilds one package against
  that repository, which is the quick loop for PKGBUILD changes. A step
  regenerates only the sources it uses, so `package:bingux-os` needs neither
  a clean tree nor Gnoblin.
- Do not edit `container-build.sh` while a build runs: bash reads it as it
  goes.
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
- The build stages the live root and the compile trees in the container's
  `/tmp`, several GB, which `build.sh` mounts as a 20 GB tmpfs
  (`BINGUX_BUILD_TMPFS` sets the size; `0` uses the container's disk instead,
  which needs about 10 GB free). A full disk shows as `could not extract ...
(Write failed)` from pacstrap or `Image size exceeds free space` from
  xorriso.

The image lands in `distro/out/bingux-os-<date>-x86_64.iso`.

## What the image does

The login screen, the desktop after signing in and the lock screen (from the headless
QEMU runs of the tests below):

![The Bingux login screen](../docs/images/bingux-os-login.webp)
![The Bingux desktop after signing in](../docs/images/bingux-os-desktop.webp)
![The Bingux lock screen](../docs/images/bingux-os-lock.webp)

- `greetd` is the display manager; it starts `bingux-greeter-session`, so the
  first thing on screen is the Bingux login screen on a Gnoblin greeter
  compositor. The account is `live` with password `live`, created by
  `bingux-os-firstboot.service` before greetd starts, together with its
  greeter wallpaper snapshot.
- Signing in starts a Gnoblin session with the Bingux shell and the default
  wallpaper. `gnoblinctl session lock` shows the lock screen (the password
  unlocks it) and `gnoblinctl session logout` returns to the login screen.
- The `bingux-os` package holds everything that makes this Bingux OS rather
  than Arch with two extra packages, and it is the same package the installer
  puts on disk, so the live image and an installed system do not drift apart:
  the greetd configuration, the `init.lua` in `/etc/skel` that loads
  `/usr/share/gnoblin/conf.d/*.lua` and the user's own `conf.d/`, the
  default wallpaper (generated at build time, so no picture is checked in)
  with a gsettings override that selects it, the repository configuration, and
  an install hook that enables greetd, NetworkManager and the global
  `bingux.target`.
- The Bingux OS pacman repository travels on the image in
  `/opt/bingux-os/repo`, so installing needs no network for those packages.
- The image names itself Bingux OS: `/etc/os-release`, `/etc/issue`, the motd,
  the boot menus (GRUB, syslinux, systemd-boot) and the Plymouth boot splash
  (theme `bingux`: black, with the Bingux mark, the product name and a spinner). The `bingux-os` package
  ships the release files and the alpm hook `bingux-os-release.hook`, which puts
  them in place after every transaction that touches `filesystem` or
  `bingux-os`; `ID_LIKE=arch` keeps Arch tooling working. The package adds the
  `plymouth` mkinitcpio hook after `kms`.
- Boot: the UEFI menu (systemd-boot, `efiboot/loader/loader.conf`) and the BIOS
  menu (syslinux, `syslinux/archiso_sys.cfg`) wait 5 s; both files replace
  releng's 15 s ones. Every live entry passes `systemd.condition_needs_update=0`:
  the overlay root never has `/etc/.updated`, so without it `ldconfig.service`
  rebuilt the linker cache (about 20 s) before greetd on every boot. The image
  already holds that cache, the hwdb and the journal catalogue from pacman's
  hooks. `tests/distro-startup.py --iso ISO` checks what an image boots with.
- Still Arch, and listed here so they are not mistaken for branding bugs: the
  package names (`archinstall`, `mkinitcpio-archiso`, `python-blivet` and the
  rest of the base set), the pacman configuration and the Arch mirror list
  (`/etc/pacman.d/mirrorlist`). The `choose-mirror` tool and its
  `choose-mirror.service` keep their upstream names, and `Installation_guide`
  stays in `/usr/local/bin`. The `arch` directory inside the ISO (its
  `install_dir`) is what the tooling expects. `tests/distro-branding.py` checks
  the names the user sees for "Arch"; the exceptions above are the ones it must tolerate.

## Kiosk flow

1. Power on the image; systemd-boot (UEFI) or syslinux (BIOS) waits 5 s.
2. `greetd` starts the Gnoblin greeter session with `bingux-greeter`. Sign in
   as `live` / `live` (the live account, created before greetd starts).
3. Signing in starts a Gnoblin session with the Bingux shell. Lock with
   `gnoblinctl session lock`; log out with `gnoblinctl session logout`.
4. Install from the live session with the installer (next section), then reboot
   and sign in to the installed system.

To run the installer's own UI, see [docs/installer.md](../docs/installer.md).

## Install to disk

The live session opens the installer (`distro/installer`, see its README).
User-facing steps, every page and the storage options are in
[docs/installer.md](../docs/installer.md). From a terminal on the live image:

```sh
bingux-installer probe
sudo bingux-installer install --plan plan.json
```

`probe` prints the disks, firmware and the capabilities this build implements as
JSON. `plan.json` is a schema v2 document; the minimum is:

```json
{
  "version": 2,
  "disk": "/dev/vda",
  "confirm_erase": "/dev/vda",
  "storage": {"mode": "erase"},
  "users": [{"name": "kieran", "password": "correct horse", "admin": true}],
  "hostname": "bingux-box",
  "timezone": "Europe/London",
  "locale": "en_GB.UTF-8",
  "keymap": "uk"
}
```

`confirm_erase` must equal `disk`. `storage.mode` is `erase` in this build; the
keep-existing modes are validated but refused until implemented. The daemon,
`bingux-installer.service`, is socket-activated. UEFI and BIOS are both
supported for an erase; see the firmware table in the installer guide.

`tests/distro-install.py` runs this end to end in QEMU: it installs onto a
blank virtual disk, inspects the result, boots the disk alone and signs in.


## Package repository

pacman reads `[bingux-os]` from `/etc/pacman.conf`, which the `bingux-os`
package adds, and the servers from `/etc/pacman.d/bingux-os-mirrorlist`. On
the live image the first server is the copy on the image; an installed system
lists the published repository only:

```text
https://github.com/kierandrewett/bingux/releases/download/pacman-x86_64
```

`distro/publish-repo.sh` creates or updates that release from
`distro/out/repo` (`--dry-run` lists what it would upload). It needs `gh`
signed in with write access, and replaces the assets on every run so the
database always matches its packages. The packages are unsigned
(`SigLevel = Optional TrustAll`): anyone who can change the release can change
what installed systems update to. Sign the packages and the database before
pointing real machines at a public repository.

To host it elsewhere, serve `distro/out/repo` over HTTPS and change the
`Server` line in `/etc/pacman.d/bingux-os-mirrorlist` (and the default in
`distro/packages/bingux-os/bingux-os-mirrorlist`).

## Tests

```sh
tests/distro-boot.py      # the live image: sign in, lock, unlock, log out
tests/distro-install.py   # install to a virtual disk, boot it, sign in
tests/distro-branding.py [--iso ISO] [--root DIR]   # no "Arch" in names the user sees
```

Screenshots for the docs come from `tests/installer-shots.py` (written to
`build/installer-shots`, or `BINGUX_INSTALLER_SHOTS`), `tests/greeter-ui.py`
(`build/kiosk-shots`, or `BINGUX_GREETER_SHOTS`) and `tests/lock-ui.py`
(`BINGUX_LOCK_SHOT`). Convert the PNGs to webp before committing them under
`docs/images/`.

Both start QEMU headless (KVM when `/dev/kvm` is writable), press keys and
take screenshots through the QEMU monitor, and write their results to
`distro/out/boot-test/` and `distro/out/install-test/` (`checks.json`,
`report.txt`, PNGs). The install test needs UEFI firmware for QEMU (OVMF;
`BINGUX_DISTRO_OVMF_CODE` and `BINGUX_DISTRO_OVMF_VARS` override the paths),
the internet and about 6 GiB of temporary space. To look at the image by hand:

```sh
qemu-system-x86_64 -enable-kvm -m 4G -smp 4 \
    -device virtio-vga-gl -display gtk,gl=on \
    -cdrom distro/out/bingux-os-*.iso
```
