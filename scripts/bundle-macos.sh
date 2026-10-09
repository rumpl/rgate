#!/bin/zsh
set -euo pipefail
[[ "$(uname -s)" == Darwin ]] || { print -u2 "This helper creates a macOS application bundle."; exit 1; }
root="${0:A:h:h}"
profile="${1:-release}"
[[ "$profile" == release || "$profile" == dev ]] || { print -u2 "Usage: $0 [release|dev]"; exit 1; }
export CARGO_TARGET_DIR="$root/target"
feature="${2:-}"
[[ -z "$feature" || "$feature" == icarus ]] || { print -u2 "Usage: $0 [release|dev] [icarus]"; exit 1; }
extra=()
[[ "$feature" == icarus ]] && extra=(--features icarus)
cargo build --locked --manifest-path "$root/Cargo.toml" -p rgate --profile "$profile" "${extra[@]}"
output=release
[[ "$profile" == dev ]] && output=debug
bundle="$root/target/RGate.app"
rm -f "$bundle/Contents/Resources/licenses/TKGATE-GPL-2.0"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/licenses"
cp "$root/target/$output/rgate" "$bundle/Contents/MacOS/rgate"
cp "$root/LICENSE" "$root/NOTICE" "$root/README.md" "$bundle/Contents/Resources/"
python3 "$root/scripts/package-licenses.py" "$bundle/Contents/Resources/licenses"
iconset="$root/target/rgate.iconset"
mkdir -p "$iconset"
for dimension in 16 32 128 256 512; do
  sips -s format png -z "$dimension" "$dimension" "$root/assets/rgate-icon.png" --out "$iconset/icon_${dimension}x${dimension}.png" >/dev/null
  double=$((dimension * 2))
  sips -s format png -z "$double" "$double" "$root/assets/rgate-icon.png" --out "$iconset/icon_${dimension}x${dimension}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/rgate.icns"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>RGate</string>
<key>CFBundleDisplayName</key><string>RGate</string>
<key>CFBundleIdentifier</key><string>org.rgate.editor</string>
<key>CFBundleExecutable</key><string>rgate</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleIconFile</key><string>rgate.icns</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHumanReadableCopyright</key><string>Independent GPL-3.0-or-later reimplementation. Original RGate artwork.</string>
</dict></plist>
PLIST
codesign --force --sign - "$bundle"
print "Created $bundle"
print "Launch with: open '$bundle'"
