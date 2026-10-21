# Standalone build. Package managers can override PREFIX and DESTDIR.
PREFIX ?= /usr/local
DESTDIR ?=
BUILD_DIR ?= build
QMAKE ?= qmake6
CARGO ?= cargo
PKG_CONFIG ?= pkg-config
QMLDIR ?= $(PREFIX)/lib/bingux/qml
# lib or lib64: where the shell archive and QML plugins live under PREFIX.
LIBDIR_NAME ?= lib
CMAKE ?= cmake
USER_PREFIX ?= $(if $(XDG_DATA_HOME),$(XDG_DATA_HOME),$(HOME)/.local/share)/bingux
JOBS ?= 2
VERSION ?= 0.1.0
OUTPUT ?= dist/bingux-$(VERSION).tar.xz
SHELL := /bin/bash
ROOT := $(abspath .)

.PHONY: all native shell shell-archive daemons doctor doctor-build check check-session install install-user uninstall-user rpm-package source-archive
all: native shell daemons

native: $(BUILD_DIR)/bingux-image-clipboard $(BUILD_DIR)/bingux-frame
	mkdir -p $(BUILD_DIR)/text $(BUILD_DIR)/effects $(BUILD_DIR)/wayland-sync
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/packages/bingux-text-layout/text-layout.pro && $(MAKE)
	cd $(BUILD_DIR)/effects && $(QMAKE) $(ROOT)/packages/bingux-effects/effects.pro && $(MAKE)
	cd $(BUILD_DIR)/wayland-sync && $(QMAKE) $(ROOT)/packages/bingux-wayland-sync/sync.pro && $(MAKE)
	$(CC) -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror packages/bingux-audio-meter/main.c -o $(BUILD_DIR)/bingux-audio-meter $$($(PKG_CONFIG) --cflags --libs libpulse)

# bingux-shell: Quickshell (pinned submodule, Bingux patches applied) built
# into one binary, plus the packed QML archive it runs.
shell: shell-archive
	$(CMAKE) -G Ninja -S packages/bingux-shell -B $(BUILD_DIR)/bingux-shell -DCMAKE_BUILD_TYPE=RelWithDebInfo -DBINGUX_LIBDIR=$(LIBDIR_NAME) -DBINGUX_SHELL_VERSION=$(VERSION)
	$(CMAKE) --build $(BUILD_DIR)/bingux-shell --parallel $(JOBS)

shell-archive:
	python3 scripts/pack-shell.py --version $(VERSION) $(BUILD_DIR)/bingux-shell.sxa

$(BUILD_DIR)/bingux-image-clipboard: packages/bingux-image-clipboard/main.c packages/bingux-image-clipboard/ext-data-control-v1.xml
	mkdir -p $(BUILD_DIR)
	wayland-scanner client-header packages/bingux-image-clipboard/ext-data-control-v1.xml $(BUILD_DIR)/ext-data-control-v1-client-protocol.h
	wayland-scanner private-code packages/bingux-image-clipboard/ext-data-control-v1.xml $(BUILD_DIR)/ext-data-control-v1-protocol.c
	$(CC) -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror packages/bingux-image-clipboard/main.c $(BUILD_DIR)/ext-data-control-v1-protocol.c -I$(BUILD_DIR) -o $@ $$($(PKG_CONFIG) --cflags --libs wayland-client)

$(BUILD_DIR)/bingux-frame: packages/bingux-frame/paint-gtk.c packages/bingux-frame/build.sh packages/bingux-frame/vendor/client.c packages/bingux-frame/vendor/paint.h packages/bingux-frame/vendor/gnoblin-window-frame-v1.xml
	bash packages/bingux-frame/build.sh $(BUILD_DIR)

daemons:
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-config/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-helper/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-searchd/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-statusd/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-session/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-settings/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo

doctor-build:
	python3 scripts/check-dependencies.py --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --cmake '$(CMAKE)' --pkg-config '$(PKG_CONFIG)'

doctor:
	python3 scripts/check-dependencies.py --install-user --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --cmake '$(CMAKE)' --pkg-config '$(PKG_CONFIG)'

rpm-package:
	python3 tests/rpm-package.py

check: doctor-build native daemons
	python3 scripts/generate-adwaita-icon-index.py --check
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/packages/bingux-text-layout/spacing-test.pro -o Makefile.tests && $(MAKE) -f Makefile.tests
	QT_QPA_PLATFORM=offscreen $(BUILD_DIR)/text/spacing-test
	$(CARGO) test --locked --manifest-path packages/bingux-config/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	BINGUX_CONFIG_BIN=$(abspath $(BUILD_DIR))/cargo/release/bingux-config python3 tests/bingux-config.test.py
	$(CARGO) test --locked --manifest-path packages/bingux-helper/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	$(CARGO) test --locked --manifest-path packages/bingux-searchd/Cargo.toml --lib
	$(CARGO) test --locked --manifest-path packages/bingux-statusd/Cargo.toml
	$(CARGO) test --locked --manifest-path packages/bingux-session/Cargo.toml
	python3 tests/shortcut-reload.py
	python3 tests/gnoblin-menu-focus.py
	BINGUX_STATUSD_BIN=$(abspath $(BUILD_DIR))/cargo/release/bingux-statusd python3 tests/osd-event-bridge.py
	python3 tests/lock-contract.test.py
	$(MAKE) check-session
	python3 tests/install-dependencies.py
	$(MAKE) rpm-package
	python3 tests/standalone-install.py

# The lock screen and the login greeter, driven in headless Sway with the
# built bingux-shell. Needs sway and grim; the lock client imports the
# Bingux.Wayland plugin from the build. shell-archive.py installs the build
# into a scratch prefix and runs the production shells from the packed archive.
SHELL_BIN = $(abspath $(BUILD_DIR))/bingux-shell/quickshell/src/bingux-shell
check-session: native daemons shell
	mkdir -p $(BUILD_DIR)/qml/Bingux/Wayland
	cp $(BUILD_DIR)/wayland-sync/libbinguxwaylandsync.so packages/bingux-wayland-sync/qmldir $(BUILD_DIR)/qml/Bingux/Wayland/
	QS_TEST_BIN=$(SHELL_BIN) BINGUX_SESSION_BIN=$(abspath $(BUILD_DIR))/cargo/release/bingux-session python3 tests/greeter-ui.py
	QS_TEST_BIN=$(SHELL_BIN) QML_IMPORT_PATH=$(abspath $(BUILD_DIR))/qml python3 tests/lock-ui.py
	python3 tests/shell-archive.py --build-dir $(BUILD_DIR)

install:
	python3 scripts/install-shell.py --prefix '$(PREFIX)' --destdir '$(DESTDIR)' --build-dir '$(BUILD_DIR)' --qml-dir '$(QMLDIR)'

install-user: doctor all
	python3 scripts/install-shell.py --user --prefix '$(USER_PREFIX)' --build-dir '$(BUILD_DIR)' --qml-dir '$(USER_PREFIX)/lib/bingux/qml'

uninstall-user:
	python3 scripts/install-shell.py --user --uninstall --prefix '$(USER_PREFIX)'

source-archive:
	bash scripts/make-source-archive.sh '$(VERSION)' '$(OUTPUT)'

.PHONY: lint format
# ARGS="--files path ..." limits the check or format operation.
lint:
	./scripts/quality.sh lint $(ARGS)

format:
	./scripts/quality.sh format $(ARGS)
