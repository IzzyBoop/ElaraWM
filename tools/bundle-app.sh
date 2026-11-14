#!/bin/sh
# Builds ElaraWM.app from a release build of `elara`.
#
#   tools/bundle-app.sh                 # writes target/release/ElaraWM.app
#   tools/bundle-app.sh ~/Applications  # writes it there instead
#
# Environment:
#   ELARA_SIGN_IDENTITY  the code-signing identity to sign with. The default, "-", signs
#                        ad hoc: macOS then sees every build as a different application,
#                        and the Accessibility permission has to be granted again after
#                        each one. A real identity, even a self-signed one, keeps it.
#   ELARA_BUNDLE_ID      the bundle identifier, org.elarawm.ElaraWM by default.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
dest=${1:-"$root/target/release"}
identity=${ELARA_SIGN_IDENTITY:--}
bundle_id=${ELARA_BUNDLE_ID:-org.elarawm.ElaraWM}
icon_source="$root/resources/icons/MacOS Icons/Elara Exports/Elara-iOS-Default-1024@1x.png"

if [ "$(uname -s)" != Darwin ]; then
    echo "bundle-app: only macOS is supported" >&2
    exit 1
fi

cd "$root"
cargo build --release -p elara
version=$(cargo metadata --no-deps --format-version 1 |
    sed -n 's/.*"name":"elara","version":"\([^"]*\)".*/\1/p')
if [ -z "$version" ]; then
    echo "bundle-app: could not read the version from cargo" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Assemble next to the destination and move into place at the end, so that a failure leaves
# the previous bundle alone.
app="$work/ElaraWM.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/target/release/elara" "$app/Contents/MacOS/elara"

# The icon. The export fills its canvas; macOS icons leave a margin around the shape, 824
# points of 1024, so that it is the size of its neighbors in the Dock and Finder.
#
# This is the flat picture of the icon. The layered `Elara.icon` next to it would need
# Xcode's `actool` to compile, which the Command Line Tools do not have.
sips -z 824 824 "$icon_source" --out "$work/shape.png" >/dev/null
sips -p 1024 1024 "$work/shape.png" --out "$work/icon.png" >/dev/null
iconset="$work/ElaraWM.iconset"
mkdir "$iconset"
for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" "$work/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" "$work/icon.png" \
        --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/ElaraWM.icns"

cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>elara</string>
    <key>CFBundleIconFile</key>
    <string>ElaraWM</string>
    <key>CFBundleIdentifier</key>
    <string>$bundle_id</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>ElaraWM</string>
    <key>CFBundleDisplayName</key>
    <string>ElaraWM</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>GPL-3.0-or-later. A port of niri to macOS.</string>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist" >/dev/null

codesign --force --sign "$identity" --identifier "$bundle_id" "$app"
codesign --verify --strict "$app"

mkdir -p "$dest"
rm -rf "$dest/ElaraWM.app"
mv "$app" "$dest/ElaraWM.app"
echo "bundle-app: wrote $dest/ElaraWM.app ($version, signed with \"$identity\")"
