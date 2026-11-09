# UScreen Unified Build & Deployment Makefile
#
# Coordinates host daemon compilation, C EVDI helper compilation,
# Cargo workspace release builds, Android APK builds, and desktop installation.

.PHONY: all build build-helper install clean run android adb edid status stop list dist dist-local setup-system publish release-metadata

VERSION = 1.2.12

CARGO = cargo
CC = gcc
ADB = adb
BIN_DIR = ${HOME}/.local/bin
DATA_DIR = ${HOME}/.local/share/uscreen

# Determine git commit hash and uncommitted dirty state for version synchronization
GIT_HASH := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_DIRTY := $(shell git status --porcelain 2>/dev/null)
ifneq ($(GIT_DIRTY),)
    GIT_VERSION := $(GIT_HASH)-dirty
else
    GIT_VERSION := $(GIT_HASH)
endif
export GIT_VERSION_STRING := $(GIT_VERSION)

## all: Default target — builds EVDI helper, Rust release binaries, and Android APK
all: build android
	@echo "✓ Synchronized unified build complete: v$(GIT_VERSION_STRING)"

## build-helper: Compile C EVDI helper with O3 optimization and link with libevdi/X11
build-helper:
	$(CC) -O3 -o host/evdi/evdi_helper host/evdi/evdi_helper.c /lib/libevdi.so.1 -lpthread -lX11 -lXrandr -Ihost/evdi
	mkdir -p target/release target/debug $(BIN_DIR)
	rm -f target/release/evdi_helper $(BIN_DIR)/evdi_helper
	cp host/evdi/evdi_helper target/release/evdi_helper
	cp host/evdi/evdi_helper $(BIN_DIR)/evdi_helper
	@echo "✓ EVDI helper: host/evdi/evdi_helper"

## build: Build all workspace binaries (daemon, pen menu, desktop app) in release profile
build: build-helper
	$(CARGO) build --release
	@echo "✓ Binaries: $(PWD)/target/release/uscreen, uscreen-menu and uscreen-pro-gui"

## install: Install binaries, assets, desktop entry, systemd user service, and GNOME Shell extension
install: build edid
	mkdir -p $(BIN_DIR) $(DATA_DIR)/edid
	# Remove old binaries first to avoid 'text file busy' errors if daemon is running
	rm -f $(BIN_DIR)/uscreen $(BIN_DIR)/uscreen-gui $(BIN_DIR)/uscreen-pro-gui $(BIN_DIR)/uscreen-menu $(BIN_DIR)/evdi_helper
	cp target/release/uscreen $(BIN_DIR)/uscreen
	cp target/release/uscreen-pro-gui $(BIN_DIR)/uscreen-pro-gui
	cp target/release/uscreen-menu $(BIN_DIR)/uscreen-menu
	cp host/evdi/evdi_helper $(BIN_DIR)/evdi_helper
	cp edid/s9ultra.bin $(DATA_DIR)/edid/
	@if [ -f android/app/build/outputs/apk/debug/app-debug.apk ]; then \
		cp android/app/build/outputs/apk/debug/app-debug.apk $(DATA_DIR)/app-debug.apk; \
		echo "✓ Installed APK to $(DATA_DIR)/app-debug.apk"; \
	fi
	@echo "✓ Installed to $(BIN_DIR)/uscreen, uscreen-menu, uscreen-pro-gui and evdi_helper"
	mkdir -p ${HOME}/.local/share/applications
	cp scripts/uscreen.desktop ${HOME}/.local/share/applications/ 2>/dev/null || true
	mkdir -p ${HOME}/.local/share/icons/hicolor/scalable/apps
	cp packaging/icons/uscreen.svg packaging/icons/uscreen-pen.svg ${HOME}/.local/share/icons/hicolor/scalable/apps/ 2>/dev/null || true
	@echo "✓ Desktop entry and icons installed (UScreen in the app menu)"
	mkdir -p ${HOME}/.config/systemd/user/ 2>/dev/null || true
	cp scripts/uscreen.service ${HOME}/.config/systemd/user/ 2>/dev/null || true
	systemctl --user daemon-reload 2>/dev/null || true
	@echo "✓ systemd user service installed"
	mkdir -p ${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/ 2>/dev/null || true
	cp extensions/uscreen-app-lock/metadata.json extensions/uscreen-app-lock/extension.js extensions/uscreen-app-lock/stylesheet.css ${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/ 2>/dev/null || true
	mkdir -p ${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/schemas 2>/dev/null || true
	cp extensions/uscreen-app-lock/schemas/*.gschema.xml ${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/schemas/ 2>/dev/null || true
	glib-compile-schemas ${HOME}/.local/share/gnome-shell/extensions/uscreen-app-lock@uscreen.app/schemas 2>/dev/null || true
	@echo "✓ GNOME Shell extension installed: uscreen-app-lock@uscreen.app"

## setup-system: One-time kernel module and modprobe configuration (requires sudo)
setup-system:
	printf "options evdi initial_device_count=2\nsoftdep evdi pre: drm_kms_helper i915\n" | sudo tee /etc/modprobe.d/uscreen-evdi.conf
	printf "evdi\nuinput\n" | sudo tee /etc/modules-load.d/uscreen.conf
	sudo modprobe evdi || true
	sudo modprobe uinput || true
	@if [ "$$(cat /sys/devices/evdi/count 2>/dev/null || echo 0)" = "0" ]; then \
		echo 1 | sudo tee /sys/devices/evdi/add; \
	fi
	@echo "✓ System setup done (EVDI device available now and at every boot)"

## run: Start the uscreen daemon in the foreground
run: build
	sudo modprobe -q evdi || true
	sudo modprobe -q uinput || true
	./target/release/uscreen start

## adb: Manually forward TCP ports 8890 (video) and 8891 (input) over ADB reverse
adb:
	$(ADB) reverse tcp:8890 tcp:8890
	$(ADB) reverse tcp:8891 tcp:8891
	@echo "✓ ADB ports: 8890 (video), 8891 (input)"

## android: Build debug APK using Gradle
android:
	cd android && ./gradlew assembleDebug --no-daemon
	@echo "✓ APK: android/app/build/outputs/apk/debug/app-debug.apk"

## android-install: Build and install debug APK onto connected Android device
android-install: android
	$(ADB) install -r android/app/build/outputs/apk/debug/app-debug.apk
	@echo "✓ APK installed on device"

## edid: Generate custom 2960x1848 60Hz binary EDID descriptor
edid:
	mkdir -p edid
	python3 scripts/gen-edid.py 2960 1848 60 edid/s9ultra.bin
	@echo "✓ EDID generated: edid/s9ultra.bin"

## list: Query and print detected video outputs from display servers
list:
	./target/release/uscreen list-displays

## status: Check active running daemon state and PID
status:
	./target/release/uscreen status

## stop: Terminate running daemon process
stop:
	./target/release/uscreen stop

## publish: Create GitHub release and upload distribution artifacts
publish:
	./scripts/publish-release.sh $(NOTES)

## release-metadata: Update release date and version across website and citation files
release-metadata:
	./scripts/update-release-metadata.sh $(VERSION) $(or $(DATE),$(shell date +%F))

## dist: Create portable release tarball via distrobox container or local fallback
dist:
	@if distrobox list 2>/dev/null | grep -q ' uscreen-build '; then \
		./scripts/build-release.sh && ./packaging/build-packages.sh; \
	else \
		echo "!! no uscreen-build container: building locally (NOT portable — see scripts/build-release.sh)"; \
		$(MAKE) dist-local; \
	fi

## dist-local: Package release tarball locally
dist-local: build
	rm -rf dist/uscreen-$(VERSION)
	mkdir -p dist/uscreen-$(VERSION)/bin dist/uscreen-$(VERSION)/scripts dist/uscreen-$(VERSION)/packaging
	cp target/release/uscreen target/release/uscreen-menu target/release/uscreen-pro-gui host/evdi/evdi_helper dist/uscreen-$(VERSION)/bin/
	cp scripts/install.sh scripts/uscreen.desktop scripts/uscreen.service dist/uscreen-$(VERSION)/scripts/
	cp packaging/uscreen-evdi.conf packaging/uscreen-modules.conf packaging/uscreen.service packaging/60-uscreen-uinput.rules dist/uscreen-$(VERSION)/packaging/
	mkdir -p dist/uscreen-$(VERSION)/packaging/icons && cp packaging/icons/uscreen.svg packaging/icons/uscreen-pen.svg dist/uscreen-$(VERSION)/packaging/icons/
	cp README.md dist/uscreen-$(VERSION)/
	cd android && ./gradlew assembleRelease -q && cp app/build/outputs/apk/release/app-release.apk ../dist/uscreen-$(VERSION)/uscreen.apk 2>/dev/null || true
	tar -C dist -czf dist/uscreen-$(VERSION)-linux-x86_64.tar.gz uscreen-$(VERSION)
	@echo "✓ Release: dist/uscreen-$(VERSION)-linux-x86_64.tar.gz"

## clean: Remove build artifacts, binaries, and temporary release directories
clean:
	$(CARGO) clean
	rm -f host/evdi/evdi_helper
	rm -rf dist
	@echo "✓ Cleaned"
