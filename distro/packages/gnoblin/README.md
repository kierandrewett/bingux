# gnoblin

No PKGBUILD lives here. `distro/build.sh` runs Gnoblin's own release tooling
(`scripts/build-release-assets.sh --source-only`) in the Gnoblin checkout,
which writes the source bundle and a matching generated PKGBUILD into
`distro/sources/`. `container-build.sh` copies that PKGBUILD here, points its
`source=` at the local bundle and builds it.
