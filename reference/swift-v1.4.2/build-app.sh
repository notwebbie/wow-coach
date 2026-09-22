#!/bin/zsh
set -euo pipefail

PROJECT_DIR="${0:A:h}"
OUTPUT_DIR="${1:-$PROJECT_DIR/dist}"
BUILD_DIR="$PROJECT_DIR/.build-app"
APP_DIR="$OUTPUT_DIR/WoW Coach.app"

mkdir -p "$OUTPUT_DIR" "$BUILD_DIR/module-cache"

CLANG_MODULE_CACHE_PATH="$BUILD_DIR/module-cache" \
SWIFTPM_MODULECACHE_OVERRIDE="$BUILD_DIR/module-cache" \
swift build \
    --package-path "$PROJECT_DIR" \
    --configuration release \
    --disable-sandbox \
    --scratch-path "$BUILD_DIR"

if [[ -d "$APP_DIR" ]]; then
    mv "$APP_DIR" "$OUTPUT_DIR/WoW Coach.previous.$(date +%Y%m%d%H%M%S).app"
fi

mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$PROJECT_DIR/Packaging/Info.plist" "$APP_DIR/Contents/Info.plist"
cp "$BUILD_DIR/release/WoWCoach" "$APP_DIR/Contents/MacOS/WoWCoach"
chmod 755 "$APP_DIR/Contents/MacOS/WoWCoach"

/usr/bin/codesign --force --deep --sign - "$APP_DIR"

echo "$APP_DIR"
