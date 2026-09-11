# Standalone build. Package managers can override PREFIX and DESTDIR.
PREFIX ?= /usr/local
DESTDIR ?=
BUILD_DIR ?= build
QMAKE ?= qmake6
CARGO ?= cargo
PKG_CONFIG ?= pkg-config
QMLDIR ?= $(PREFIX)/lib/bingux/qml
USER_PREFIX ?= $(if $(XDG_DATA_HOME),$(XDG_DATA_HOME),$(HOME)/.local/share)/bingux
JOBS ?= 2
VERSION ?= 0.1.0
OUTPUT ?= dist/bingux-$(VERSION).tar.xz
SHELL := /bin/bash
ROOT := $(abspath .)

.PHONY: all native daemons doctor doctor-build check install install-user uninstall-user source-archive
all: native daemons

native:
	mkdir -p $(BUILD_DIR)/text $(BUILD_DIR)/settings $(BUILD_DIR)/effects
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/packages/bingux-text-layout/text-layout.pro && $(MAKE)
	cd $(BUILD_DIR)/settings && $(QMAKE) $(ROOT)/packages/bingux-settings/platform/platform.pro && $(MAKE)
	cd $(BUILD_DIR)/effects && $(QMAKE) $(ROOT)/packages/bingux-effects/effects.pro && $(MAKE)
	$(CC) -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror packages/bingux-audio-meter/main.c -o $(BUILD_DIR)/bingux-audio-meter $$($(PKG_CONFIG) --cflags --libs libpulse)

daemons:
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-searchd/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path packages/bingux-statusd/Cargo.toml --target-dir $(abspath $(BUILD_DIR))/cargo

doctor-build:
	python3 scripts/check-dependencies.py --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --pkg-config '$(PKG_CONFIG)'

doctor:
	python3 scripts/check-dependencies.py --install-user --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --pkg-config '$(PKG_CONFIG)'

check: doctor-build native daemons
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/packages/bingux-text-layout/spacing-test.pro -o Makefile.tests && $(MAKE) -f Makefile.tests
	QT_QPA_PLATFORM=offscreen $(BUILD_DIR)/text/spacing-test
	$(CARGO) test --locked --manifest-path packages/bingux-searchd/Cargo.toml --lib
	$(CARGO) test --locked --manifest-path packages/bingux-statusd/Cargo.toml --lib
	python3 tests/install-dependencies.py
	python3 tests/standalone-install.py

install:
	python3 scripts/install-shell.py --prefix '$(PREFIX)' --destdir '$(DESTDIR)' --build-dir '$(BUILD_DIR)' --qml-dir '$(QMLDIR)'

install-user: doctor all
	python3 scripts/install-shell.py --user --prefix '$(USER_PREFIX)' --build-dir '$(BUILD_DIR)' --qml-dir '$(USER_PREFIX)/lib/bingux/qml'

uninstall-user:
	python3 scripts/install-shell.py --user --uninstall --prefix '$(USER_PREFIX)'

source-archive:
	bash scripts/make-source-archive.sh '$(VERSION)' '$(OUTPUT)'
