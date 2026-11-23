#!/bin/sh
# Builds dist/BoxLauncher.app (the current Mac's CPU)
set -e
cd "$(dirname "$0")/.."
cargo build --release
APP=dist/BoxLauncher.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/boxlauncher "$APP/Contents/MacOS/BoxLauncher"
cp assets/icon.icns "$APP/Contents/Resources/icon.icns"
VER=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cat > "$APP/Contents/Info.plist" <<PL
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Box Launcher</string>
<key>CFBundleDisplayName</key><string>Box Launcher</string>
<key>CFBundleIdentifier</key><string>local.boxlauncher.desktop</string>
<key>CFBundleExecutable</key><string>BoxLauncher</string>
<key>CFBundleIconFile</key><string>icon</string>
<key>CFBundleShortVersionString</key><string>$VER</string>
<key>CFBundleVersion</key><string>$VER</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PL
echo "built $APP"
