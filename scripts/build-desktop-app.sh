#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
profile=debug
cargo_args=(build -p messenger-desktop --message-format=json)
if [[ "${1:-}" == "--release" ]]; then profile=release; cargo_args+=(--release); fi
build_log=$(mktemp)
trap 'rm -f "$build_log"' EXIT
cargo "${cargo_args[@]}" > "$build_log"
agent=$(python3 - "$build_log" <<'PY'
import json,pathlib,sys
for line in pathlib.Path(sys.argv[1]).read_text().splitlines():
 try:v=json.loads(line)
 except ValueError:continue
 if v.get('reason')=='build-script-executed' and 'messenger-desktop' in v.get('package_id',''):
  print(pathlib.Path(v['out_dir'])/'NotificationAgent')
PY
)
app="target/$profile/horobets.chat.app"
mkdir -p "$app/Contents/MacOS"
cp "target/$profile/messenger-desktop" "$app/Contents/MacOS/messenger-desktop"
cp "$agent" "$app/Contents/MacOS/NotificationAgent"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>chat.horobets.desktop</string>
<key>CFBundleName</key><string>horobets.chat</string>
<key>CFBundleDisplayName</key><string>horobets.chat</string>
<key>CFBundleExecutable</key><string>messenger-desktop</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --deep --sign - "$app"
printf 'Desktop app built: %s\nOpen the bundle and enable notifications in Settings.\n' "$app"
