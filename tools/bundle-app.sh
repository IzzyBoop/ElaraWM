#!/bin/sh
# Builds ElaraWM.app from a release build of `elara`.
#
#   tools/bundle-app.sh                 # writes target/release/ElaraWM.app
#   tools/bundle-app.sh ~/Applications  # writes it there instead
#
# Environment:
#   ELARA_SIGN_IDENTITY  the code-signing identity to sign with. By default, a certificate
#                        named "ElaraWM Signing" if the keychain has one, and otherwise
#                        "-", which signs ad hoc. An ad hoc build is a different
#                        application to macOS every time: the Accessibility permission
#                        has to be granted again after each one, and the old entries pile
#                        up in System Settings. Any certificate, even a self-signed one,
#                        keeps the permission across builds. See the README for making one.
#   ELARA_FLAT_ICON      set to anything to use the flat icon even when Xcode is there.
#   ELARA_BUNDLE_ID      the bundle identifier, org.elarawm.ElaraWM by default.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
dest=${1:-"$root/target/release"}
local_identity="ElaraWM Signing"
if [ -n "${ELARA_SIGN_IDENTITY:-}" ]; then
    identity=$ELARA_SIGN_IDENTITY
elif security find-certificate -c "$local_identity" >/dev/null 2>&1; then
    identity=$local_identity
else
    identity=-
fi
bundle_id=${ELARA_BUNDLE_ID:-org.elarawm.ElaraWM}
icon_layers="$root/resources/icons/MacOS Icons/Elara.icon"
min_system=14.0
icon_source="$root/resources/icons/MacOS Icons/Elara Exports/Elara-iOS-Default-1024@1x.png"

if [ "$(uname -s)" != Darwin ]; then
    echo "bundle-app: only macOS is supported" >&2
    exit 1
fi

cd "$root"
# The compiler writes the paths of the source files into the binary, for panic messages and
# backtraces. Those start with the home directory of whoever built it, which has no place
# in an application handed to others.
RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$root=elarawm --remap-path-prefix=$HOME=~" \
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
# The debugging entries of the symbol table name every object file by its full path in the
# build directory, which the remapping above does not reach. Function names stay, so a
# backtrace in the log still says where it was.
strip -S "$app/Contents/MacOS/elara"

# The icon. With Xcode installed, the layered `Elara.icon` is compiled, which is what gives
# the icon its dark, tinted and clear appearances. `actool` writes an asset catalog for
# that and a flat `.icns` beside it for anything that does not read the catalog.
#
# Xcode does not have to be the selected developer directory: it is looked for where it is
# normally installed too.
actool=$(xcrun --find actool 2>/dev/null ||
    DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcrun --find actool 2>/dev/null ||
    true)
icon_name=
if [ -n "$actool" ] && [ -z "${ELARA_FLAT_ICON:-}" ]; then
    if "$actool" "$icon_layers" \
        --compile "$app/Contents/Resources" \
        --app-icon Elara \
        --output-partial-info-plist "$work/icon.plist" \
        --platform macosx \
        --minimum-deployment-target "$min_system" >"$work/actool.log" 2>&1 &&
        [ -f "$app/Contents/Resources/Assets.car" ] &&
        [ -f "$app/Contents/Resources/Elara.icns" ]; then
        icon_file=Elara
        icon_name=Elara
        icon_kind="layered icon"
    else
        echo "bundle-app: actool could not compile the layered icon; using the flat one:" >&2
        cat "$work/actool.log" >&2
        rm -f "$app/Contents/Resources/Assets.car" "$app/Contents/Resources/Elara.icns"
    fi
fi

# Without Xcode, the flat picture of the icon. The export fills its canvas; macOS icons leave
# a margin around the shape, 824 points of 1024, so that it is the size of its neighbors in
# the Dock and Finder.
if [ -z "$icon_name" ]; then
    icon_file=ElaraWM
    icon_kind="flat icon"
    sips -z 824 824 "$icon_source" --out "$work/shape.png" >/dev/null
    sips -p 1024 1024 "$work/shape.png" --out "$work/icon.png" >/dev/null
    iconset="$work/ElaraWM.iconset"
    mkdir "$iconset"
    for size in 16 32 128 256 512; do
        double=$((size * 2))
        sips -z "$size" "$size" "$work/icon.png" \
            --out "$iconset/icon_${size}x${size}.png" >/dev/null
        sips -z "$double" "$double" "$work/icon.png" \
            --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$app/Contents/Resources/ElaraWM.icns"
fi

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
    <string>$icon_file</string>
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
    <string>$min_system</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>GPL-3.0-or-later. A port of niri to macOS.</string>
</dict>
</plist>
PLIST
if [ -n "$icon_name" ]; then
    plutil -insert CFBundleIconName -string "$icon_name" "$app/Contents/Info.plist"
fi
plutil -lint "$app/Contents/Info.plist" >/dev/null

codesign --force --sign "$identity" --identifier "$bundle_id" "$app"
codesign --verify --strict "$app"

mkdir -p "$dest"
rm -rf "$dest/ElaraWM.app"
mv "$app" "$dest/ElaraWM.app"
echo "bundle-app: wrote $dest/ElaraWM.app ($version, $icon_kind, signed with \"$identity\")"
