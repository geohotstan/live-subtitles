#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_NAME="subtitles-swift"
BUILD_DIR="$ROOT_DIR/build"
APP_DIR="$BUILD_DIR/Subtitles.app"

swift build -c release --package-path "$ROOT_DIR"

mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$ROOT_DIR/.build/release/$BIN_NAME" "$APP_DIR/Contents/MacOS/$BIN_NAME"
cp "$ROOT_DIR/Resources/Info.plist" "$APP_DIR/Contents/Info.plist"

chmod +x "$APP_DIR/Contents/MacOS/$BIN_NAME"

echo "Built $APP_DIR"
