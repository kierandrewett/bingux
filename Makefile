# Standalone build. Package managers can override PREFIX and DESTDIR.
PREFIX ?= /usr/local
DESTDIR ?=
BUILD_DIR ?= build
QMAKE ?= qmake6
CARGO ?= cargo
PKG_CONFIG ?= pkg-config
QMLDIR ?= $(PREFIX)/lib/bingux/qml
LADSPADIR ?= $(PREFIX)/lib/ladspa
# Where Cargo puts its output; package builders can point this at a shared target.
CARGO_TARGET ?= $(abspath $(BUILD_DIR))/cargo
# bingux-dsp is built with panic=abort (a plugin must not unwind into its host)
# while the programs unwind. Cargo does not keep the two builds of a shared
# dependency such as libc apart in one target directory ("the crate `libc`
# requires panic strategy `abort`"), so the plugin library builds in its own.
DSP_TARGET ?= $(CARGO_TARGET)-dsp
# Work and results of `make rpm-package` and `make arch-package`.
PACKAGE_DIR ?= /var/tmp/BinguxPkg
USER_PREFIX ?= $(if $(XDG_DATA_HOME),$(XDG_DATA_HOME),$(HOME)/.local/share)/bingux
JOBS ?= 2
VERSION ?= 0.1.0
OUTPUT ?= dist/bingux-$(VERSION).tar.xz
SHELL := /bin/bash
ROOT := $(abspath .)

.PHONY: all native daemons apps icons icons-check doctor doctor-build check install install-user uninstall-user install-apps uninstall-apps rpm-package rpm-check arch-package licenses source-archive
all: native daemons

native: $(BUILD_DIR)/bingux-image-clipboard $(BUILD_DIR)/bingux-frame
	mkdir -p $(BUILD_DIR)/text $(BUILD_DIR)/effects $(BUILD_DIR)/wayland-sync
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/packages/bingux-text-layout/text-layout.pro && $(MAKE)
	cd $(BUILD_DIR)/effects && $(QMAKE) $(ROOT)/packages/bingux-effects/effects.pro && $(MAKE)
	cd $(BUILD_DIR)/wayland-sync && $(QMAKE) $(ROOT)/packages/bingux-wayland-sync/sync.pro && $(MAKE)
	$(CC) -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror packages/bingux-audio-meter/main.c $(CFLAGS) $(LDFLAGS) -o $(BUILD_DIR)/bingux-audio-meter $$($(PKG_CONFIG) --cflags --libs libpulse)

$(BUILD_DIR)/bingux-image-clipboard: packages/bingux-image-clipboard/main.c packages/bingux-image-clipboard/ext-data-control-v1.xml
	mkdir -p $(BUILD_DIR)
	wayland-scanner client-header packages/bingux-image-clipboard/ext-data-control-v1.xml $(BUILD_DIR)/ext-data-control-v1-client-protocol.h
	wayland-scanner private-code packages/bingux-image-clipboard/ext-data-control-v1.xml $(BUILD_DIR)/ext-data-control-v1-protocol.c
	$(CC) -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror packages/bingux-image-clipboard/main.c $(BUILD_DIR)/ext-data-control-v1-protocol.c -I$(BUILD_DIR) $(CFLAGS) $(LDFLAGS) -o $@ $$($(PKG_CONFIG) --cflags --libs wayland-client)

$(BUILD_DIR)/bingux-frame: packages/bingux-frame/paint-gtk.c packages/bingux-frame/build.sh packages/bingux-frame/vendor/client.c packages/bingux-frame/vendor/paint.h packages/bingux-frame/vendor/gnoblin-window-frame-v1.xml
	bash packages/bingux-frame/build.sh $(BUILD_DIR)

# The Rust programs. Each crate has its own Cargo.lock, so each is built on its own.
# bingux-settings, bingux-files and bingux-taxonomy are the Freya applications;
# bingux-effects and libbingux_dsp.so are the sound-effects engine and its LADSPA plugins.
# `apps` builds the applications and what Settings runs (bingux-config, the effects
# engine); `daemons` adds the search and status daemons.
apps:
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-config/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-audio-effects/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-dsp/Cargo.toml --target-dir $(DSP_TARGET)
	install -Dm755 $(DSP_TARGET)/release/libbingux_dsp.so $(CARGO_TARGET)/release/libbingux_dsp.so
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-settings/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-files-app/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-taxonomy/Cargo.toml --target-dir $(CARGO_TARGET)

daemons: apps
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-searchd/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-statusd/Cargo.toml --target-dir $(CARGO_TARGET)

# Compile the icon sources (scripts/icons/src) into the bingux-ui icons and the
# shell's copy. Needs uv: the script declares its own packages. map-fork.py first
# writes the one-line sources from scripts/icons/labels.tsv.
icons:
	scripts/icons/map-fork.py
	scripts/icons/build.py

# Sources match labels.tsv, sources are valid, output is current, and every icon
# meets the size, centre, crispness and symmetry rules (docs/bingux-ui.md, Icons).
icons-check:
	scripts/icons/map-fork.py --check
	scripts/icons/build.py --check

doctor-build:
	python3 scripts/check-dependencies.py --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --pkg-config '$(PKG_CONFIG)'

doctor:
	python3 scripts/check-dependencies.py --install-user --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --pkg-config '$(PKG_CONFIG)'

# Builds every RPM from a source archive with rpmbuild into $(PACKAGE_DIR). It needs
# the build dependencies of packaging/rpm/bingux.spec unless BINGUX_RPM_NODEPS=1
# (the default here) skips the dependency check. See docs/release.md.
rpm-package:
	bash scripts/build-rpms.sh '$(VERSION)' '$(PACKAGE_DIR)'

# Checks the spec, the units, the desktop entries and the AppStream files without building.
rpm-check:
	python3 tests/rpm-package.py

# Builds the Arch packages from packaging/arch/PKGBUILD into $(PACKAGE_DIR)/arch, in
# an Arch container when makepkg is not installed.
arch-package:
	bash scripts/build-arch.sh '$(VERSION)' '$(PACKAGE_DIR)'

# The licences of the Rust crates linked into each program, for the packages.
licenses:
	python3 scripts/cargo-licenses.py --out '$(BUILD_DIR)/licenses'

check: doctor-build native daemons
	python3 scripts/generate-adwaita-icon-index.py --check
	python3 scripts/ui-lint.py
	python3 scripts/taxonomy-check.py
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/packages/bingux-text-layout/spacing-test.pro -o Makefile.tests && $(MAKE) -f Makefile.tests
	QT_QPA_PLATFORM=offscreen $(BUILD_DIR)/text/spacing-test
	$(CARGO) test --locked --manifest-path packages/bingux-config/Cargo.toml --target-dir $(CARGO_TARGET)
	BINGUX_CONFIG_BIN=$(CARGO_TARGET)/release/bingux-config python3 tests/bingux-config.test.py
	$(CARGO) test --locked --manifest-path packages/bingux-searchd/Cargo.toml --lib
	$(CARGO) test --locked --manifest-path packages/bingux-files/Cargo.toml
	$(CARGO) test --locked --manifest-path packages/bingux-search/Cargo.toml
	$(CARGO) test --locked --manifest-path packages/bingux-data-usage/Cargo.toml
	$(CARGO) test --locked --manifest-path packages/bingux-taxonomy/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) test --locked --manifest-path packages/bingux-statusd/Cargo.toml
	python3 tests/shortcut-reload.py
	python3 tests/gnoblin-menu-focus.py
	BINGUX_STATUSD_BIN=$(CARGO_TARGET)/release/bingux-statusd python3 tests/osd-event-bridge.py
	python3 tests/install-dependencies.py
	$(MAKE) rpm-check
	python3 tests/standalone-install.py

install:
	python3 scripts/install-shell.py --prefix '$(PREFIX)' --destdir '$(DESTDIR)' --build-dir '$(BUILD_DIR)' --qml-dir '$(QMLDIR)' --cargo-dir '$(CARGO_TARGET)' --ladspa-dir '$(LADSPADIR)'

install-user: doctor all
	python3 scripts/install-shell.py --user --prefix '$(USER_PREFIX)' --build-dir '$(BUILD_DIR)' --qml-dir '$(USER_PREFIX)/lib/bingux/qml' --cargo-dir '$(CARGO_TARGET)'

uninstall-user:
	python3 scripts/install-shell.py --user --uninstall --prefix '$(USER_PREFIX)'

# Settings, Files and Taxonomy only, for your user: no shell, no session change.
# APPS_PREFIX is the folder that gets bin/, share/ and lib/ (default ~/.local).
APPS_PREFIX ?= $(HOME)/.local
install-apps: apps
	python3 scripts/install-shell.py --apps --prefix '$(APPS_PREFIX)' --cargo-dir '$(CARGO_TARGET)'

uninstall-apps:
	python3 scripts/install-shell.py --apps --uninstall --prefix '$(APPS_PREFIX)'

source-archive:
	bash scripts/make-source-archive.sh '$(VERSION)' '$(OUTPUT)'

.PHONY: lint format
# ARGS="--files path ..." limits the check or format operation.
lint:
	./scripts/quality.sh lint $(ARGS)

format:
	./scripts/quality.sh format $(ARGS)
