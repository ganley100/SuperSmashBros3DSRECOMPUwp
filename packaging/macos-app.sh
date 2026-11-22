#!/bin/sh
# packages the macOS binary as a functional Super Smash Bros. 3DS Recompiled.app, with MoltenVK inside so that the 3D
# draws on the GPU with nothing else installed, and zips it.
#
#   macos-app.sh <binary> <version> <output.zip>
set -eu

moltenvk_url=https://github.com/KhronosGroup/MoltenVK/releases/download/v1.4.2/MoltenVK-macos.tar
moltenvk_sha256=f95765a6229cb7b915990a2890ce12ebe36a730b021545d3d52ae69ce4c4024e

binary=$1
version=$2
output_zip=$(pwd)/$3
case "$output_zip" in *.zip) output_dmg="${output_zip%.zip}.dmg" ;; *) echo "output must end in .zip" >&2; exit 2 ;; esac
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# MoltenVK is fetched from GitHub at package time; retry transient GitHub/DNS failures.
curl --fail --silent --show-error --location \
    --retry 5 --retry-all-errors --retry-delay 5 --retry-max-time 180 \
    --connect-timeout 20 --max-time 240 \
    -o "$work/moltenvk.tar" "$moltenvk_url"
echo "$moltenvk_sha256  $work/moltenvk.tar" | shasum -a 256 -c -
tar xf "$work/moltenvk.tar" -C "$work" MoltenVK/LICENSE MoltenVK/MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib

app_name="Super Smash Bros. 3DS Recompiled"
contents="$work/$app_name.app/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources" "$contents/Frameworks"
cp "$binary" "$contents/MacOS/smash-3ds"
cp "$work/MoltenVK/MoltenVK/dynamic/dylib/macOS/libMoltenVK.dylib" "$contents/Frameworks/"
cp "$work/MoltenVK/LICENSE" "$contents/Resources/MoltenVK-LICENSE"
cp "$here/../recomplauncher.json" "$contents/Resources/recomplauncher.json"
cp "$here/Info.plist" "$contents/Info.plist"
/usr/libexec/PlistBuddy \
    -c "Set :CFBundleShortVersionString $version" \
    -c "Set :CFBundleVersion $version" \
    "$contents/Info.plist"

# the icon at every size macOS asks for, each with its double
icons=$work/zakuro.iconset
mkdir "$icons"
for size in 16 32 128 256; do
    sips -z "$size" "$size" "$here/zakuro.png" --out "$icons/icon_${size}x${size}.png" > /dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$here/zakuro.png" --out "$icons/icon_${size}x${size}@2x.png" > /dev/null
done
cp "$here/zakuro.png" "$icons/icon_512x512.png"
iconutil -c icns "$icons" -o "$contents/Resources/zakuro.icns"

# signed for no one, which is what lets it run on Apple silicon at all
codesign --force --deep --sign - "$work/$app_name.app"

# Include the manifest at the archive root and inside the app bundle.
package="$work/package"
mkdir "$package"
cp -R "$work/$app_name.app" "$package/"
cp "$here/../recomplauncher.json" "$package/recomplauncher.json"
cp "$here/../README.md" "$here/../LICENSE" "$here/../LICENSE-MIT" "$here/../THIRD_PARTY_NOTICES.md" "$package/"
test -f "$package/recomplauncher.json"
test -f "$package/Super Smash Bros. 3DS Recompiled.app/Contents/Resources/recomplauncher.json"
(cd "$package" && /usr/bin/zip -qry "$output_zip" .)
hdiutil create -volname "$app_name" -srcfolder "$package" -ov -format UDZO "$output_dmg" >/dev/null
echo "Created $output_zip and $output_dmg"
