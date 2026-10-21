#!/usr/bin/env bash
# Bingux OS live image. Overlaid on archiso's releng profile by
# distro/container-build.sh, which supplies the boot loader configuration.
# shellcheck disable=SC2034

iso_name="bingux-os"
iso_label="BINGUX_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="Bingux OS <https://github.com/kierandrewett/bingux>"
iso_application="Bingux OS live image"
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="arch"
buildmodes=('iso')
bootmodes=('bios.syslinux.mbr' 'bios.syslinux.eltorito'
    'uefi-ia32.systemd-boot.esp' 'uefi-x64.systemd-boot.esp'
    'uefi-ia32.systemd-boot.eltorito' 'uefi-x64.systemd-boot.eltorito')
arch="x86_64"
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"
airootfs_image_tool_options=('-comp' 'xz' '-Xbcj' 'x86' '-b' '1M' '-Xdict-size' '1M')
bootstrap_tarball_compression=('zstd' '-c' '-T0' '--auto-threads=logical' '--long' '-19')
file_permissions=(
    ["/etc/shadow"]="0:0:400"
    ["/etc/gshadow"]="0:0:400"
    ["/root"]="0:0:750"
    ["/usr/local/bin/bingux-os-firstboot"]="0:0:755"
    ["/usr/local/bin/bingux-os-install"]="0:0:755"
)
