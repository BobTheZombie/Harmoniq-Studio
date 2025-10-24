#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APPDIR="$ROOT_DIR/appimage/AppDir"
SHARED_DIR="$ROOT_DIR/shared"
DIST_DIR="$ROOT_DIR/../dist"
PROJECT_ROOT="$(dirname "$ROOT_DIR")"
APPIMAGE_TOOL=${APPIMAGE_TOOL:-appimagetool}

if ! command -v "$APPIMAGE_TOOL" >/dev/null 2>&1; then
  echo "appimagetool is required but was not found in PATH" >&2
  exit 1
fi

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/scalable/apps" "$DIST_DIR"

pushd "$PROJECT_ROOT" >/dev/null
cargo build --release -p harmoniq-app
popd >/dev/null

install -Dm755 "$PROJECT_ROOT/target/release/harmoniq-app" "$APPDIR/usr/bin/harmoniq-studio"
install -Dm755 "$ROOT_DIR/appimage/AppRun" "$APPDIR/AppRun"
install -Dm644 "$SHARED_DIR/harmoniq-studio.desktop" "$APPDIR/usr/share/applications/harmoniq-studio.desktop"
install -Dm644 "$PROJECT_ROOT/resources/icons/harmoniq-studio.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/harmoniq-studio.svg"

if command -v linuxdeploy >/dev/null 2>&1; then
  linuxdeploy --appdir="$APPDIR" --desktop-file="$APPDIR/usr/share/applications/harmoniq-studio.desktop" --icon-file="$APPDIR/usr/share/icons/hicolor/scalable/apps/harmoniq-studio.svg"
fi

VERSION=$(cargo metadata --format-version 1 --no-deps --manifest-path "$PROJECT_ROOT/Cargo.toml" | jq -r '.packages[] | select(.name=="harmoniq-app") | .version')
APPIMAGE_NAME="Harmoniq-Studio-${VERSION}-$(uname -m).AppImage"

"$APPIMAGE_TOOL" "$APPDIR" "$DIST_DIR/$APPIMAGE_NAME"
echo "Created $DIST_DIR/$APPIMAGE_NAME"
