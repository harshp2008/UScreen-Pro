#!/usr/bin/env bash
# UScreen Pro Master Unified Build Pipeline
# Builds Android APK, Rust Host Daemon, and Tauri 2.0 Frontend into dist/,
# then force-deploys them to ~/.local/bin so a stale copy can never keep running.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# 1. Deterministic version string extraction
GIT_HASH=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
IS_DIRTY=$(git status --porcelain 2>/dev/null || true)
if [ -n "$IS_DIRTY" ]; then
    export GIT_VERSION_STRING="${GIT_HASH}-dirty"
else
    export GIT_VERSION_STRING="${GIT_HASH}"
fi

echo "=========================================================="
echo "  UScreen Pro Master Unified Build Pipeline"
echo "  Version Stamp: ${GIT_VERSION_STRING}"
echo "=========================================================="

DIST_DIR="$SCRIPT_DIR/dist"
mkdir -p "$DIST_DIR"

# 2. Build Android APK via Gradle
echo ""
echo "==> [1/4] Building Android Client APK via Gradle..."
(
    cd android
    export GIT_VERSION_STRING
    ./gradlew assembleDebug --no-daemon
)

APK_SRC="android/app/build/outputs/apk/debug/app-debug.apk"
if [ -f "$APK_SRC" ]; then
    cp "$APK_SRC" "$DIST_DIR/app-debug.apk"
    cp "$APK_SRC" "$DIST_DIR/uscreen-v${GIT_VERSION_STRING}.apk"
    echo "  ✓ APK copied -> $DIST_DIR/uscreen-v${GIT_VERSION_STRING}.apk"
else
    echo "  ✗ Error: APK was not generated at $APK_SRC" >&2
    exit 1
fi

# 3. Build Rust Host Daemon
echo ""
echo "==> [2/4] Building Rust Host Daemon (cargo build --release -p uscreen)..."
export GIT_VERSION_STRING
cargo build --release -p uscreen

DAEMON_SRC="target/release/uscreen"
if [ -f "$DAEMON_SRC" ]; then
    cp --remove-destination "$DAEMON_SRC" "$DIST_DIR/uscreen"
    echo "  ✓ Daemon copied -> $DIST_DIR/uscreen"
else
    echo "  ✗ Error: Daemon was not generated at $DAEMON_SRC" >&2
    exit 1
fi

# Copy evdi_helper if available
if [ -f "target/release/evdi_helper" ]; then
    cp --remove-destination "target/release/evdi_helper" "$DIST_DIR/evdi_helper"
    echo "  ✓ EVDI Helper copied -> $DIST_DIR/evdi_helper"
fi

# 4. Build Tauri 2.0 MD3 Frontend
echo ""
echo "==> [3/4] Building UScreen Pro Tauri 2.0 Frontend (npm run tauri build)..."
(
    cd uscreen-pro-gui
    export GIT_VERSION_STRING
    npm run tauri build -- --no-bundle
)

TAURI_SRC="target/release/uscreen-pro-gui"
if [ -f "$TAURI_SRC" ]; then
    pkill -f uscreen-pro-gui 2>/dev/null || true
    cp --remove-destination "$TAURI_SRC" "$DIST_DIR/uscreen-pro-gui"
    echo "  ✓ Tauri Desktop App copied -> $DIST_DIR/uscreen-pro-gui"
else
    echo "  ✗ Error: Tauri binary was not found at $TAURI_SRC" >&2
    exit 1
fi

# 5. Force global deployment: kill running GUI instances, overwrite installed binaries
echo ""
echo "==> [4/4] Deploying to ~/.local/bin (forced)..."
BIN_DIR="$HOME/.local/bin"
SHARE_DIR="$HOME/.local/share/uscreen"
mkdir -p "$BIN_DIR" "$SHARE_DIR"

pkill -9 -f uscreen-pro-gui 2>/dev/null || true
sleep 0.5

cp --remove-destination "$DIST_DIR/uscreen-pro-gui" "$BIN_DIR/uscreen-pro-gui"
cp --remove-destination "$DIST_DIR/uscreen" "$BIN_DIR/uscreen"
# The GUI installs this APK onto tablets, so it must match the build above.
cp --remove-destination "$DIST_DIR/app-debug.apk" "$SHARE_DIR/app-debug.apk"

for pair in "uscreen-pro-gui:$BIN_DIR/uscreen-pro-gui" "uscreen:$BIN_DIR/uscreen" "app-debug.apk:$SHARE_DIR/app-debug.apk"; do
    name="${pair%%:*}"
    dest="${pair#*:}"
    if cmp -s "$DIST_DIR/$name" "$dest"; then
        echo "  ✓ $dest ($(stat -c '%y' "$dest" | cut -d. -f1))"
    else
        echo "  ✗ Deployed file differs from build output: $dest" >&2
        exit 1
    fi
done

echo ""
echo "=========================================================="
echo "  ✓ All Deliverables Assembled in dist/"
echo "=========================================================="
ls -lh "$DIST_DIR"
echo "Build pipeline completed successfully!"
