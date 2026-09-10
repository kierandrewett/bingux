# Contributing to Bingux

Bingux is a desktop shell. Keep changes focused on the shell, its native
helpers and the protocols between them. Machine images, host networking and
distribution profiles do not belong in this repository.

## Before you change code

Read the relevant guide in `docs/`. Keep user-facing text short and concrete.
Preserve the existing settings format unless a migration is included. Do not
commit generated build output, local configuration or core dumps.

## Checks

Use the native build entry point:

```sh
make
make check
```

`make check` builds both Rust daemons, runs their library tests, checks the
native text layout test and verifies a staged install.

When the full Qt development set is unavailable, run the checks that do not
need it and state that limitation in the change description. Useful focused
checks include:

```sh
python3 tests/standalone-install.py
python3 tests/extensions.test.py
node --test tests/desktop-layout.test.mjs tests/control-layout.test.mjs
```

## Packaging

Builds use `packaging/rpm/bingux.spec` and the source archive produced by
`make source-archive`. A package must pass a clean Fedora build and a staged
install before it is published to COPR.

Use small commits with a conventional subject and a body that explains the
user-visible result. Keep unrelated working-tree changes out of a commit.
