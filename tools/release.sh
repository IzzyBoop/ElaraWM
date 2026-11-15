#!/bin/sh
# Builds ElaraWM.app and zips it for a release.
#
#   tools/release.sh    # writes target/release/ElaraWM-<version>-<arch>.zip
#
# The version is the one in Cargo.toml. The application is signed as tools/bundle-app.sh
# signs it; see there for ELARA_SIGN_IDENTITY. Every release has to be signed with the same
# identity, or macOS takes the new version for a different application and its users have
# to grant the permissions again. An ad hoc signature is refused for that reason.
#
# This only makes the file. Tagging and uploading are left to whoever makes the release:
#
#   git tag v<version> && git push origin v<version>
#   gh release create v<version> target/release/ElaraWM-<version>-<arch>.zip
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
out="$root/target/release"
app="$out/ElaraWM.app"

cd "$root"
if [ -n "$(git status --porcelain)" ]; then
    echo "release: the working tree has changes that are not committed" >&2
    exit 1
fi

"$root/tools/bundle-app.sh" "$out"

# What the application says about itself, so that the name of the file cannot be wrong.
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$app/Contents/Info.plist")
binary="$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"
arch=$(lipo -archs "$binary" | tr ' ' '-')

if codesign -dv "$app" 2>&1 | grep -q '^Signature=adhoc'; then
    echo "release: the application is signed ad hoc; sign it with a certificate" >&2
    exit 1
fi
codesign --verify --deep --strict "$app"

# bundle-app.sh keeps the builder's home directory out of the binary. Make sure of it.
# Looked for in the raw bytes: `strings` skips the parts of the file it takes for symbols.
if LC_ALL=C grep -q -a -F "$HOME" "$binary"; then
    echo "release: the binary names paths under $HOME" >&2
    exit 1
fi

zip="$out/ElaraWM-$version-$arch.zip"
rm -f "$zip"
# ditto keeps what a signed bundle needs, which zip does not.
ditto -c -k --keepParent "$app" "$zip"

echo "release: wrote $zip"
echo "release: sha256 $(shasum -a 256 "$zip" | cut -d' ' -f1)"
