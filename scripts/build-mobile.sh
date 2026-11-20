#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:---apple}"
case "$mode" in --host|--apple|--android|--all) ;; *) echo "Usage: $0 [--host|--apple|--android|--all]" >&2; exit 2;; esac
export MACOSX_DEPLOYMENT_TARGET=14.0
repo_path="$PWD"
export CARGO_TARGET_DIR="$repo_path/target"
bridge_path="$repo_path/mobile/Bridge"
generated_path="$repo_path/target/mobile-bindings"
mkdir -p "$generated_path" "$bridge_path/Sources/MessengerCore" "$bridge_path/Headers"
cargo build --locked -p messenger-mobile
for language in swift kotlin; do
    cargo run --locked -p messenger-mobile --features bindgen --bin messenger-bindgen -- generate \
        target/debug/libmessenger_mobile.dylib --language "$language" \
        --out-dir "$generated_path/$language" --no-format
done
cp "$generated_path/swift/MessengerCore.swift" "$bridge_path/Sources/MessengerCore/Generated.swift"
cp "$generated_path/swift/MessengerCoreFFI.h" "$bridge_path/Headers/"
cp "$generated_path/swift/MessengerCoreFFI.modulemap" "$bridge_path/Headers/module.modulemap"
# Skip includes .kt files beside skip.yml in its Android module.
find "$generated_path/kotlin" -name '*.kt' -exec cp '{}' mobile/Sources/MessengerApp/Skip/ \;
if [[ "$mode" != --android || ! -d "$bridge_path/MessengerCoreFFI.xcframework" ]]; then
    xc_args=(-library "$repo_path/target/debug/libmessenger_mobile.a" -headers "$bridge_path/Headers")
    if [[ "$mode" == --apple || "$mode" == --all ]]; then
        for platform in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
            IPHONEOS_DEPLOYMENT_TARGET=17.0 cargo build --locked -p messenger-mobile --target "$platform"
        done
        mkdir -p "$generated_path/ios-simulator"
        lipo -create target/aarch64-apple-ios-sim/debug/libmessenger_mobile.a \
            target/x86_64-apple-ios/debug/libmessenger_mobile.a -output "$generated_path/ios-simulator/libmessenger_mobile.a"
        xc_args+=(-library "$repo_path/target/aarch64-apple-ios/debug/libmessenger_mobile.a" -headers "$bridge_path/Headers")
        xc_args+=(-library "$generated_path/ios-simulator/libmessenger_mobile.a" -headers "$bridge_path/Headers")
    fi
    # This directory contains only this script's generated build artifact.
    python3 - "$bridge_path/MessengerCoreFFI.xcframework" <<'PY'
import pathlib, shutil, sys
p = pathlib.Path(sys.argv[1])
if p.exists(): shutil.rmtree(p)
PY
    xcodebuild -create-xcframework "${xc_args[@]}" -output "$bridge_path/MessengerCoreFFI.xcframework"
fi
if [[ "$mode" == --android || "$mode" == --all ]]; then
    export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
    cargo ndk -t arm64-v8a -t x86_64 --platform 26 -o mobile/Android/app/src/main/jniLibs build --locked -p messenger-mobile
fi
