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
# lib or lib64: where the shell archive and QML plugins live under PREFIX.
LIBDIR_NAME ?= lib
CMAKE ?= cmake
USER_PREFIX ?= $(if $(XDG_DATA_HOME),$(XDG_DATA_HOME),$(HOME)/.local/share)/bingux
JOBS ?= 2
# Captures `layout-check` and `overflow-check` render at once.
LINT_JOBS ?= 4
VERSION ?= 0.1.0
OUTPUT ?= dist/bingux-$(VERSION).tar.xz
SHELL := /bin/bash
ROOT := $(abspath .)

.PHONY: all native shell shell-archive daemons apps icons icons-check doctor doctor-build check check-session layout-check overflow-check copy-check pixel-check stability-check motion-check state-check rules-check install install-user uninstall-user install-apps uninstall-apps rpm-package rpm-check arch-package licenses source-archive
all: native shell daemons

native: $(BUILD_DIR)/bingux-image-clipboard $(BUILD_DIR)/bingux-frame
	mkdir -p $(BUILD_DIR)/text $(BUILD_DIR)/effects $(BUILD_DIR)/wayland-sync
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/native/text-layout/text-layout.pro && $(MAKE)
	cd $(BUILD_DIR)/effects && $(QMAKE) $(ROOT)/native/effects/effects.pro && $(MAKE)
	cd $(BUILD_DIR)/wayland-sync && $(QMAKE) $(ROOT)/native/wayland-sync/sync.pro && $(MAKE)
	$(CC) -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror native/audio-meter/main.c $(CFLAGS) $(LDFLAGS) -o $(BUILD_DIR)/bingux-audio-meter $$($(PKG_CONFIG) --cflags --libs libpulse)

# bingux-shell: Quickshell (pinned submodule, Bingux patches applied) built
# into one binary, plus the packed QML archive it runs.
shell: shell-archive
	$(CMAKE) -G Ninja -S shell/runtime -B $(BUILD_DIR)/bingux-shell -DCMAKE_BUILD_TYPE=RelWithDebInfo -DBINGUX_LIBDIR=$(LIBDIR_NAME) -DBINGUX_SHELL_VERSION=$(VERSION)
	$(CMAKE) --build $(BUILD_DIR)/bingux-shell --parallel $(JOBS)

shell-archive:
	python3 scripts/pack-shell.py --version $(VERSION) $(BUILD_DIR)/bingux-shell.sxa

$(BUILD_DIR)/bingux-image-clipboard: native/image-clipboard/main.c native/image-clipboard/ext-data-control-v1.xml
	mkdir -p $(BUILD_DIR)
	wayland-scanner client-header native/image-clipboard/ext-data-control-v1.xml $(BUILD_DIR)/ext-data-control-v1-client-protocol.h
	wayland-scanner private-code native/image-clipboard/ext-data-control-v1.xml $(BUILD_DIR)/ext-data-control-v1-protocol.c
	$(CC) -std=c11 -D_POSIX_C_SOURCE=200809L -O2 -Wall -Wextra -Werror native/image-clipboard/main.c $(BUILD_DIR)/ext-data-control-v1-protocol.c -I$(BUILD_DIR) $(CFLAGS) $(LDFLAGS) -o $@ $$($(PKG_CONFIG) --cflags --libs wayland-client)

$(BUILD_DIR)/bingux-frame: native/frame/paint-gtk.c native/frame/build.sh native/frame/vendor/client.c native/frame/vendor/paint.h native/frame/vendor/gnoblin-window-frame-v1.xml
	bash native/frame/build.sh $(BUILD_DIR)

# The Rust programs. Each crate has its own Cargo.lock, so each is built on its own.
# bingux-settings, bingux-files and bingux-taxonomy are the Freya applications;
# bingux-preview-host is the helper process Files runs to play films and songs and
# render PDFs (GStreamer and poppler stay out of the application);
# bingux-effects and libbingux_dsp.so are the sound-effects engine and its LADSPA plugins.
# `apps` builds the applications and what Settings runs (bingux-config, the effects
# engine); `daemons` adds the search and status daemons, the privileged helper, the
# session manager and the portal backend.
apps:
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path crates/config/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path services/audio-effects/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path crates/dsp/Cargo.toml --target-dir $(DSP_TARGET)
	install -Dm755 $(DSP_TARGET)/release/libbingux_dsp.so $(CARGO_TARGET)/release/libbingux_dsp.so
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path apps/settings/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path apps/files/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path services/preview-host/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path apps/taxonomy/Cargo.toml --target-dir $(CARGO_TARGET)

daemons: apps
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path services/helper/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path services/searchd/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path services/statusd/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path services/session/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) build --locked --release -j $(JOBS) --manifest-path services/portal/Cargo.toml --target-dir $(CARGO_TARGET)

# Compile the icon sources (scripts/icons/src) into the bingux-ui icons and the
# shell's copy. Needs uv: the script declares its own packages. map-fork.py first
# writes the one-line sources from scripts/icons/labels.tsv. colour.py compiles the
# colour file and folder icons (scripts/icons/colour) into crates/ui/icons/colour.
icons:
	scripts/icons/map-fork.py
	scripts/icons/build.py
	scripts/icons/colour.py

# Sources match labels.tsv, sources are valid, output is current, and every icon
# meets the size, centre, crispness and symmetry rules (docs/bingux-ui.md, Icons).
icons-check:
	scripts/icons/map-fork.py --check
	scripts/icons/build.py --check
	scripts/icons/colour.py --check
	scripts/icons/golden.py

doctor-build:
	python3 scripts/check-dependencies.py --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --cmake '$(CMAKE)' --pkg-config '$(PKG_CONFIG)'

doctor:
	python3 scripts/check-dependencies.py --install-user --qmake '$(QMAKE)' --cargo '$(CARGO)' --cc '$(CC)' --cmake '$(CMAKE)' --pkg-config '$(PKG_CONFIG)'

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

# Renders every Settings page, popup and taxonomy page headlessly (no compositor,
# fixed fakes for the machine's data) and fails on a layout defect or on text that
# overflows under pseudo-localised strings, 200 % text scale and very long data.
# Each builds the snapshot binaries first (a no-op when they are current). The
# findings and the ALLOWED lists are described in docs/bingux-ui.md, "Layout checks".
# --rtl captures everything a second time right to left and fails on a box that is not
# the reflection of where it is left to right (design/rtl.md).
layout-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/layout-lint.py --build --parity --rtl --jobs $(LINT_JOBS)

overflow-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/overflow-check.py --build --jobs $(LINT_JOBS)

# Drives every Settings page, popup, taxonomy page and the Files window headlessly:
# points at, presses and focuses each interactive node and renders every state,
# tabs through the page and back, walks every menu by key, and reads the cursor
# over each control. Fails on a control with no hover, pressed or focus look, a
# clipped ring, a touch target under 44 px at 390x800, a control under a resize
# handle, a Tab trap or the wrong cursor. The rules and the ALLOW lists are in
# docs/bingux-ui.md, "State checks".
state-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/state-check.py --build --jobs $(LINT_JOBS)

# Copy: the written rules about words (percent signs, "show more", filler, subtitles that
# restate their title, three dots for an ellipsis) over the source strings and the text of
# every capture. docs/bingux-ui.md, "Copy checks".
copy-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/copy-lint.py --build --jobs $(LINT_JOBS)

# Pixels: text contrast measured on the PNG of every page, popup and taxonomy state, and a
# visible border or shadow on all four sides of every popup, dialog and menu card, in both
# schemes and at 1x and 2x. docs/bingux-ui.md, "Pixel checks".
pixel-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/pixel-check.py --build --jobs $(LINT_JOBS)

# Stability: the first frame of every page, popup, taxonomy page and Files state against the
# settled one, and the scroll offset kept through a push and pop, a tab switch and a data
# refresh. docs/bingux-ui.md, "Stability checks".
stability-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/structure-stability.py --build --jobs $(LINT_JOBS)
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/scroll-continuity.py --jobs $(LINT_JOBS)

# Motion: loops run and nothing else moves, animated numbers roll, a close is its open
# mirrored, and every page mounts inside its frame budget (report and warn on a noisy
# runner: BINGUX_FRAME_GATE=report). docs/bingux-ui.md, "Motion checks".
motion-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/loop-check.py --all
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/number-roll-check.py
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/nav-slide.py
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/frame-gate.py

# The report on top of the checks above: it calls the same scripts (one at a time, through
# scripts/app_env.py), reads from the checks which rule IDs each one holds, and writes the state of
# every rule of design/rules.json (passing, failing, not run, unenforced) to design/rules-report.md
# and .json. ARGS passes options through:
#   make rules-check ARGS=--offline          the checks that need no app, report kept out of design/
#   make rules-check ARGS="--only ui-lint,copy-lint"
#   make rules-check ARGS="--reuse /var/tmp/rules-check/latest --strict"
# Exit status is 1 while a rule is failing. `check` does not call it: `check` already runs the
# checks and fails on their findings; this adds the per-rule report (design/rules-checks.md).
rules-check:
	CARGO_TARGET_DIR='$(CARGO_TARGET)' python3 scripts/rules-check.py --jobs $(JOBS) $(ARGS)

check: doctor-build native daemons
	python3 scripts/generate-adwaita-icon-index.py --check
	python3 scripts/ui-lint.py
	python3 scripts/taxonomy-check.py
	cd $(BUILD_DIR)/text && $(QMAKE) $(ROOT)/native/text-layout/spacing-test.pro -o Makefile.tests && $(MAKE) -f Makefile.tests
	QT_QPA_PLATFORM=offscreen $(BUILD_DIR)/text/spacing-test
	$(CARGO) test --locked --manifest-path crates/config/Cargo.toml --target-dir $(CARGO_TARGET)
	BINGUX_CONFIG_BIN=$(CARGO_TARGET)/release/bingux-config python3 tests/bingux-config.test.py
	$(CARGO) test --locked --manifest-path services/helper/Cargo.toml --target-dir $(CARGO_TARGET)
	python3 tests/app-env.test.py
	python3 tests/rules-check.test.py
	$(CARGO) test --locked --manifest-path services/searchd/Cargo.toml --lib
	$(CARGO) test --locked --manifest-path crates/files/Cargo.toml
	$(CARGO) test --locked --manifest-path crates/preview-core/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) test --locked --manifest-path services/preview-host/Cargo.toml --target-dir $(CARGO_TARGET)
	$(CARGO) test --locked --manifest-path crates/search/Cargo.toml
	$(CARGO) test --locked --manifest-path crates/data-usage/Cargo.toml
	$(CARGO) test --locked --manifest-path apps/taxonomy/Cargo.toml --target-dir $(CARGO_TARGET)
	$(MAKE) layout-check overflow-check state-check
	$(MAKE) copy-check pixel-check stability-check motion-check
	$(CARGO) test --locked --manifest-path crates/ui/Cargo.toml --features testing --target-dir $(CARGO_TARGET) --lib slide_users_tests
	$(CARGO) test --locked --manifest-path services/statusd/Cargo.toml
	$(CARGO) test --locked --manifest-path services/session/Cargo.toml
	$(CARGO) test --locked --manifest-path services/portal/Cargo.toml --target-dir $(CARGO_TARGET)
	python3 tests/shortcut-reload.py
	python3 tests/gnoblin-menu-focus.py
	BINGUX_STATUSD_BIN=$(CARGO_TARGET)/release/bingux-statusd python3 tests/osd-event-bridge.py
	BINGUX_STATUSD_BIN=$(CARGO_TARGET)/release/bingux-statusd python3 tests/audio-osd-feedback.py
	BINGUX_STATUSD_BIN=$(CARGO_TARGET)/release/bingux-statusd python3 tests/pad-help-event-bridge.py
	python3 tests/lock-contract.test.py
	python3 tests/bsm-greeter-progress.test.py
	node --test tests/bsm-session.test.mjs tests/bsm-readiness.test.mjs
	python3 tests/bsm-lock-binding.py
	python3 tests/qmldir-complete.py --self-test
	python3 tests/qmldir-complete.py
	$(MAKE) check-session
	python3 tests/install-dependencies.py
	$(MAKE) rpm-check
	python3 tests/standalone-install.py

# The lock screen, the login greeter and the bsm greeter (against the fake bsm
# control server), driven in headless Sway with the
# built bingux-shell. Needs sway and grim; the lock client imports the
# Bingux.Wayland plugin from the build. shell-archive.py installs the build
# into a scratch prefix and runs the production shells from the packed archive.
SHELL_BIN = $(abspath $(BUILD_DIR))/bingux-shell/quickshell/src/bingux-shell
check-session: native daemons shell
	mkdir -p $(BUILD_DIR)/qml/Bingux/Wayland
	cp $(BUILD_DIR)/wayland-sync/libbinguxwaylandsync.so native/wayland-sync/qmldir $(BUILD_DIR)/qml/Bingux/Wayland/
	QS_TEST_BIN=$(SHELL_BIN) BINGUX_SESSION_BIN=$(CARGO_TARGET)/release/bingux-session python3 tests/greeter-ui.py
	QS_TEST_BIN=$(SHELL_BIN) QML_IMPORT_PATH=$(abspath $(BUILD_DIR))/qml python3 tests/lock-ui.py
	QS_TEST_BIN=$(SHELL_BIN) python3 tests/bsm-greeter-client.test.py
	QS_TEST_BIN=$(SHELL_BIN) QML_IMPORT_PATH=$(abspath $(BUILD_DIR))/qml python3 tests/bsm-greeter-ui.py
	QS_TEST_BIN=$(SHELL_BIN) python3 tests/identify.py
	QS_TEST_BIN=$(SHELL_BIN) python3 tests/pad-help.py
	QS_TEST_BIN=$(SHELL_BIN) python3 tests/bsm-shell-integration.py
	QUICKSHELL_BIN=$(SHELL_BIN) bash tests/search-command.sh
	python3 tests/shell-archive.py --build-dir $(BUILD_DIR)

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
