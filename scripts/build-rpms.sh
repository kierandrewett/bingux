#!/usr/bin/env bash
# Build every Bingux RPM from the source archive with rpmbuild.
#
# usage: scripts/build-rpms.sh [VERSION [TOPDIR]]
#
# VERSION   defaults to 0.1.0 (the Makefile's VERSION); a nightly build passes its RPM version, such as
#           0.1.1~nightly.20261031.5.gabc1234 (scripts/pkgrepo version)
# TOPDIR    defaults to /var/tmp/BinguxPkg; rpmbuild works in TOPDIR/rpmbuild and the
#           packages land in TOPDIR/rpmbuild/RPMS and TOPDIR/rpmbuild/SRPMS
#
# Environment:
#   BINGUX_ARCHIVE_WORKTREE=1   archive the working tree instead of HEAD (a local
#                               packaging check of uncommitted work; never publish)
#   BINGUX_RPM_CHECK_DEPS=1     let rpmbuild check the BuildRequires (default: --nodeps,
#                               for hosts that build with their own toolchain)
#   BINGUX_KEEP_BUILD=1         keep the unpacked build tree and build root
#   CARGO_TARGET                share a Cargo target directory between builds
#   JOBS                        parallel jobs for make and Cargo (default: 4)
#   SOURCE_DATE_EPOCH           build time stamp (default: the commit time of HEAD)
#
# The build time stamp and file times come from SOURCE_DATE_EPOCH, so the same
# commit gives the same archive; the RPM headers follow the spec changelog.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version="${1:-0.1.0}"
top="${2:-/var/tmp/BinguxPkg}"
topdir="$top/rpmbuild"

command -v rpmbuild >/dev/null || {
    echo "build-rpms: rpmbuild is not installed (dnf install rpm-build)" >&2
    exit 1
}

export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct)}"

rm -rf "$topdir"
mkdir -p "$topdir"/{SOURCES,SPECS,BUILD,BUILDROOT,RPMS,SRPMS}

bash "$root/scripts/make-source-archive.sh" "$version" "$topdir/SOURCES/bingux-${version}.tar.xz" >/dev/null
# The spec in the tree names the release in the tree. A nightly build gives VERSION (0.1.1~nightly.20261031.5.gabc1234),
# which sorts below the next release and above the last one; the archive above has the same name as that version.
sed -e "s/^Version:.*/Version:        ${version}/" "$root/packaging/rpm/bingux.spec" >"$topdir/SPECS/bingux.spec"

options=(-ba --define "_topdir $topdir" --define "_smp_build_ncpus ${JOBS:-4}")
[[ "${BINGUX_RPM_CHECK_DEPS:-0}" == 1 ]] || options+=(--nodeps)
[[ "${BINGUX_KEEP_BUILD:-0}" == 1 ]] || options+=(--clean)
rpmbuild "${options[@]}" "$topdir/SPECS/bingux.spec"

echo
echo "Packages in $topdir:"
find "$topdir/RPMS" "$topdir/SRPMS" -name '*.rpm' | sort
