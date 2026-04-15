# Calamares GUI Installer for Bingux

Future plan for a graphical installer using [Calamares](https://calamares.io/).

## Build Dependencies

Calamares requires a significant toolchain to compile from source:

| Dependency     | Version | Notes                                    |
|---------------|---------|------------------------------------------|
| Qt6           | >= 6.5  | ~2GB source, hours to compile from scratch |
| KDE Frameworks| >= 6.0  | KPMcore (partition manager), KCoreAddons  |
| Python3       | >= 3.8  | For Python-based modules                  |
| PyQt6         |         | Python Qt bindings for scripted pages     |
| yaml-cpp      |         | YAML config parsing                       |
| libparted     |         | Disk partition manipulation               |
| polkit        |         | Privilege escalation for disk operations  |
| CMake         | >= 3.16 | Build system                              |
| Boost         |         | Boost.Python for module bridge            |
| libpwquality  |         | Password strength checking                |
| libatasmart   |         | S.M.A.R.T. disk info                      |

Total estimated build time from source (no binary caches): **4-6 hours** on a modern machine.

## Calamares Modules

### Standard modules to use as-is

- **partition** -- Disk partitioning GUI (uses KPMcore). Configure to create GPT with EFI + root.
- **mount** -- Mount target partitions to `/tmp/bingux-install/`.
- **unpackfs** -- Extract squashfs root image to target.
- **bootloader** -- GRUB EFI installation. Configure for `--target=x86_64-efi`.
- **users** -- Create user accounts, set passwords, configure sudo/wheel.
- **locale** -- Locale, language selection.
- **keyboard** -- Keyboard layout selection.
- **timezone** -- Timezone map selector.
- **finished** -- Completion screen with reboot option.

### Bingux-specific custom module

A custom Calamares module (`modules/bingux-setup/`) is needed to handle Bingux-specific setup that standard modules do not cover:

```yaml
# modules/bingux-setup.conf
---
type: "job"
name: "bingux-setup"
interface: "process"
command: "/usr/lib/calamares/modules/bingux-setup/bingux-setup.sh"
```

The custom module must:

1. **Create Bingux directory layout**:
   - `/io` -- device nodes (devtmpfs mount point)
   - `/system/packages` -- package store
   - `/system/profiles` -- generation profiles
   - `/system/config` -- system.toml and related config
   - `/system/state/persistent` -- package DB, build locks
   - `/system/state/ephemeral` -- tmpfs runtime state
   - `/system/kernel/proc`, `/system/kernel/sys` -- pseudo-fs mounts
   - `/system/boot`, `/system/modules` -- kernel and modules
   - `/system/recipes` -- recipe repository
   - `/users/` -- per-user home directories

2. **Generate system.toml** from Calamares config variables:
   - Hostname from the `users` module
   - Locale/timezone/keymap from respective modules
   - Package list from selected package sets
   - User accounts from the `users` module

3. **Run `bsys apply`** to compose the initial generation:
   - Creates `/system/profiles/1/` with dispatch table and symlinks
   - Sets `current` symlink
   - Generates `/etc` files from system.toml

4. **Install `bingux_hide` kernel module** if available:
   - Copy to `/system/modules/bingux_hide.ko`
   - Configure modprobe for boot-time loading

5. **Write init configuration**:
   - Ensure `/init` (bingux-init.sh) is present and executable
   - Configure boot mode (systemd handoff or standalone)

## Module Execution Order

```yaml
# settings.conf
sequence:
  - show:
      - welcome
      - locale
      - keyboard
      - partition
      - users
      - summary
  - exec:
      - partition
      - mount
      - unpackfs
      - bingux-setup     # custom: directory layout + system.toml + bsys apply
      - bootloader
      - users
      - umount
  - show:
      - finished
```

## Branding

Calamares supports custom branding via `branding/bingux/branding.desc`:

```yaml
componentName: bingux
welcomeStyleCalamares: true
strings:
    productName: Bingux
    shortProductName: Bingux
    version: 2
    shortVersion: 2
    versionedName: Bingux v2
    shortVersionedName: Bingux v2
    bootloaderEntryName: Bingux
    productUrl: https://github.com/kierandrewett/bingux
images:
    productLogo: "bingux-logo.svg"
    productIcon: "bingux-icon.svg"
slideshow: "show.qml"
```

## Alternatives to Building from Source

1. **Use Calamares from host distro packages**: If building the ISO on Fedora/Arch, package Calamares and its dependencies as .bgx packages using pre-built binaries from the host. This avoids the multi-hour Qt6 build.

2. **Static/AppImage build**: Build Calamares once on a CI machine, package as a single self-contained binary or AppImage to include on the ISO.

3. **Defer to TUI installer**: The shell-based TUI installer (`bingux-install.sh`) covers all functionality needed for initial releases. Calamares is a nice-to-have for desktop users but not critical path.

## Estimated Effort

| Task                          | Effort      |
|------------------------------|-------------|
| Qt6 from source              | 2-4 hours build |
| KDE Frameworks from source   | 1-2 hours build |
| Calamares build              | 15-30 min   |
| Custom bingux-setup module   | 1-2 days dev |
| Branding + slideshow         | 1 day       |
| Integration testing          | 1-2 days    |
| **Total**                    | **~1 week** |

## Recommendation

Ship with the TUI installer for v2.0. Plan Calamares for v2.1+ when the desktop environment (GNOME/Wayland) is integrated and Qt6 is available as a Bingux package.
