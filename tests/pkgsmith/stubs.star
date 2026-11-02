# Stand-in packages for the install check of scripts/verify-packages.sh. They hold one text file each and
# nothing else; they exist so a clean container can resolve what the real packages depend on or replace.
#
#   KIND=gnoblin   gnoblin and xdg-desktop-portal-gnoblin: the compositor and portal that `bingux` depends on.
#                  They are published from repo.gnoblin.org by Gnoblin's own CI.
#   KIND=legacy    the packages that `bingux` replaces (bingux-config, bingux-audio-effects, bingux-preview-host):
#                  installing them first proves that the upgrade removes them.
#
#   pkgsmith build -f tests/pkgsmith/stubs.star --prefix tests/pkgsmith --build-arg KIND=gnoblin -F rpm

KIND = arg("KIND", "gnoblin")
VERSION = arg("VERSION", "0.1.0")
ARCH = arg("ARCH", host()["arch"])

NAMES = {
    "gnoblin": ["gnoblin", "xdg-desktop-portal-gnoblin"],
    "legacy": ["bingux-config", "bingux-audio-effects", "bingux-preview-host"],
}[KIND]

for name in NAMES:
    for fmt in ["rpm", "deb", "archlinux"]:
        reset()
        expose(file(here() + "/stub.txt", "/usr/share/bingux-stubs/" + name))
        package(
            fmt,
            name = name,
            version = "0.0.1" if KIND == "legacy" else VERSION,
            arch = ARCH,
            description = "A stand-in for " + name + " in the install check of Bingux",
            license = "GPL-3.0-or-later",
            maintainer = "Kieran Drewett <kieran@drewett.dev>",
        )
