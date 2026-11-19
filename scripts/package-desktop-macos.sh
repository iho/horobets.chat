#!/bin/sh
set -eu
if [ "$(uname -s)" != Darwin ]; then
    echo "This script creates a macOS app bundle; run it on macOS." >&2
    exit 1
fi
cd "$(dirname "$0")/.."
case "${1:---release}" in
    --debug) cargo build -p messenger-desktop; profile=debug ;;
    --release) cargo build --release -p messenger-desktop; profile=release ;;
    *) echo "Usage: $0 [--release|--debug]" >&2; exit 1 ;;
esac
target_dir="${CARGO_TARGET_DIR:-target}"
bundle="$target_dir/$profile/horobets.chat.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
# Replace atomically so a running app keeps its original executable inode.
cp "$target_dir/$profile/messenger-desktop" "$bundle/Contents/MacOS/messenger-desktop.new"
mv -f "$bundle/Contents/MacOS/messenger-desktop.new" "$bundle/Contents/MacOS/messenger-desktop"
cp crates/desktop/resources/Info.plist "$bundle/Contents/Info.plist"
echo "$bundle"
