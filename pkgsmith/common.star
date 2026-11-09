# What the pkgsmith scripts of Bingux share: the identity of every package, the dependencies
# by distribution, the install scripts and the licence expressions.
#
# docs/packaging.md describes the pipeline. In short: scripts/build-prefix.sh builds all of Bingux
# into a prefix, and scripts/build-packages.sh runs pkgsmith.star here on that prefix. A prefix is
# built on one distribution and holds that distribution's Qt and C libraries, so the dependencies
# of rpm, deb and archlinux below name the packages of Fedora 43, Debian 13 and Arch Linux.
#
# Build arguments (scripts/build-packages.sh sets them):
#   VERSION     the version as pkgsmith spells it: 0.1.0, or 0.1.1-nightly.20261031.142.gabc1234
#   ARCH        amd64 or arm64; the architecture of the prefix (default: this machine)
#   QT_MINOR    the Qt minor version the shell was built against, such as 6.10. Qt's private API
#               changes with every minor release, so the shell needs that minor and no other
#   RPM_DIST    the dist tag of the rpm release, such as .fc43 (default: none)
#   DISTRO      the name in the file of the tar package, such as fedora43 (default: the host's)

VERSION = arg("VERSION", "0.1.0")
ARCH = arg("ARCH", host()["arch"])
QT_MINOR = arg("QT_MINOR", "")
RPM_DIST = arg("RPM_DIST", "")
DISTRO = arg("DISTRO", host()["distro"])
RELEASE = "1"

# The formats of every script, in the order they are written. There is no apk: Bingux needs systemd
# and logind (bingux.target and the user units), which Alpine does not ship, and the prefix links
# glibc and the Qt of the distribution. docs/packaging.md says what an apk would need.
FORMATS = ["rpm", "deb", "archlinux", "tar"]

# The licence expression of each package: the project's GPL, the licences of the crates linked into
# its programs and, for `bingux`, the LGPL of Quickshell inside the shell binary.
# `python3 scripts/cargo-licenses.py --summary` computes them and `--check` compares them with this table
# (make package-check).
LICENSES = {
    "bingux": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND CDLA-Permissive-2.0 AND GPL-3.0-or-later AND ISC AND LGPL-3.0-or-later AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-files": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-calculator": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND CDLA-Permissive-2.0 AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-weather": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND CDLA-Permissive-2.0 AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-text-editor": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-preview": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-settings": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-calendar": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-taxonomy": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
    "bingux-media": "Apache-2.0 AND BSD-2-Clause AND BSD-3-Clause AND CDLA-Permissive-2.0 AND GPL-3.0-or-later AND ISC AND MIT AND Unicode-3.0 AND Zlib",
}

# The applications. Each is its own package: a program, a desktop entry, an icon, AppStream metadata, a
# man page and its Fluent catalogues, and nothing else. `id` is the AppStream component id; `domain` names
# the app's catalogues (`/usr/share/bingux/locale/<language>/<domain>.ftl`). bingux-ui's own catalogue
# (`bingux-ui.ftl`) is shared by every app and ships in `bingux`, which each app depends on.
APPS = {
    "bingux-settings": {"id": "dev.drewett.bingux.Settings", "domain": "settings", "extra": ["/usr/share/bingux/search-apps/bingux-settings.json"]},
    "bingux-files": {"id": "dev.drewett.bingux.Files", "domain": "files", "extra": []},
    "bingux-calculator": {"id": "dev.drewett.bingux.Calculator", "domain": "calculator", "extra": []},
    "bingux-weather": {"id": "dev.drewett.bingux.Weather", "domain": "weather", "extra": []},
    "bingux-text-editor": {"id": "dev.drewett.bingux.TextEditor", "domain": "text-editor", "extra": []},
    "bingux-preview": {"id": "dev.drewett.bingux.Preview", "domain": "preview", "extra": []},
    "bingux-calendar": {"id": "dev.drewett.bingux.Calendar", "domain": "calendar", "extra": []},
    "bingux-taxonomy": {"id": "dev.drewett.bingux.Taxonomy", "domain": "taxonomy", "extra": []},
    "bingux-media": {"id": "dev.drewett.bingux.Media", "domain": "media", "extra": []},
}

# The paths of the prefix that belong to the application `name`, as globs of the installed system.
def app_files(name):
    app = APPS[name]
    return [
        "/usr/bin/" + name,
        "/usr/share/applications/" + name + ".desktop",
        "/usr/share/icons/hicolor/scalable/apps/" + name + ".svg",
        "/usr/share/metainfo/" + app["id"] + ".metainfo.xml",
        "/usr/share/man/man1/" + name + ".1*",
        "/usr/share/doc/" + name,
        "/usr/share/licenses/" + name,
        "/usr/share/bingux/locale/*/" + app["domain"] + ".ftl",
    ] + app["extra"]

# The folders a package owns: its own, never the shared ones (/usr/share/applications, /etc/pam.d, /usr/lib/systemd, ...),
# which other packages and the system own. Each name owns the folder and everything below it.
def own_dirs(*names):
    out = []
    for name in names:
        out += [name, name + "/**"]
    return out

APP_FILES = []
for _name in APPS:
    APP_FILES += app_files(_name)

MAINTAINER = "Kieran Drewett <kieran@drewett.dev>"
HOMEPAGE = "https://github.com/kierandrewett/bingux"

# The package versions as each format writes them, for dependencies that pin one version. This
# follows the rules of pkgsmith (docs/versions.md); tests/packaging.py compares it with `pkgsmith plan`.
def native_version(fmt):
    base, _, pre = VERSION.partition("-")
    if not pre:
        return base + "-" + RELEASE
    if fmt == "archlinux":
        # Arch drops the commit text and joins the name to the number after it.
        parts = [part for part in pre.split(".") if not (part.startswith("g") and len(part) >= 8)]
        text = parts[0] + "".join(parts[1:2]) + ("." + ".".join(parts[2:]) if len(parts) > 2 else "")
        return base + text + "-" + RELEASE
    return base + "~" + pre + "-" + RELEASE

# `bingux` with this exact version, in the dependency syntax of a format.
def pin_bingux(fmt):
    version = native_version(fmt)
    if fmt == "rpm":
        return "bingux = " + version + RPM_DIST
    if fmt == "deb":
        return "bingux (= " + version + ")"
    if fmt == "archlinux":
        return "bingux=" + version
    return None

# One entry of a dependency list in the syntax of a format. `name` and `bounds` are text; a bound
# is (operator, version) with the operators >= and <.
def versioned(fmt, name, *bounds):
    out = []
    for op, version in bounds:
        if fmt == "rpm":
            out.append(name + " " + op + " " + version)
        elif fmt == "deb":
            out.append(name + " (" + {">=": ">=", "<": "<<"}[op] + " " + version + ")")
        else:
            out.append(name + op + version)
    return out

# The Qt libraries the shell and its plugins were built against: the minor version of the build and
# no other. Without QT_MINOR the dependency is the plain name.
QT_PACKAGE = {"rpm": "qt6-qtbase", "deb": "libqt6core6t64", "archlinux": "qt6-base"}

def qt_pin(fmt):
    if not QT_MINOR:
        return [QT_PACKAGE[fmt]]
    major, _, minor = QT_MINOR.partition(".")
    next_minor = major + "." + str(int(minor) + 1)
    return versioned(fmt, QT_PACKAGE[fmt], (">=", QT_MINOR), ("<", next_minor))

# The version of a package of this repository and the fields every package has.
def identity(fmt, name, description, license_name):
    fields = {
        "name": name,
        "version": VERSION,
        "release": RELEASE + (RPM_DIST if fmt == "rpm" else ""),
        "arch": ARCH,
        "description": description,
        "maintainer": MAINTAINER,
        "homepage": HOMEPAGE,
        "license": LICENSES[license_name],
    }
    if fmt == "tar":
        fields["file"] = "{name}-{version}-{arch}-" + DISTRO + ".tar.xz"
    return fields

# Run the package-manager commands of a scriptlet only on a booted systemd system.
SYSTEMD_RUNNING = "[ -d /run/systemd/system ]"
