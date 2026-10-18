#!/usr/bin/env bash
# Build a universal macOS app bundle (arm64 + x86_64) and a drag-to-Applications DMG.
# Usage: ./scripts/package_dmg.sh
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="${HOME}/.cargo/bin:${PATH}"

ROOT="$(pwd)"
VERSION="$(awk -F'"' '/^version/{print $2; exit}' Cargo.toml)"
APP="dist/OPS.app"
DMG="dist/OPS-${VERSION}-macos-universal.dmg"
TMP="dist/OPS-rw.dmg"
STAGE="dist/dmgroot"
UNI="dist/universal"
BG="assets/dmg-background.png"
VOLNAME="OPS"

echo "==> checking universal toolchain"
X86_LIB="$(rustc --print target-libdir --target x86_64-apple-darwin 2>/dev/null || true)"
if [ -z "${X86_LIB}" ] || ! compgen -G "${X86_LIB}/libstd-*.rlib" >/dev/null 2>&1; then
  echo "    x86_64 std not installed; trying rustup target add"
  if ! rustup target add x86_64-apple-darwin; then
    echo "error: x86_64-apple-darwin target is missing and could not be installed." >&2
    echo "       On an offline machine, copy the rust-std component into the sysroot." >&2
    echo "       To build an arm64-only DMG instead, run with ARCH=arm64." >&2
    if [ "${ARCH:-universal}" != "arm64" ]; then
      exit 1
    fi
  fi
fi

if [ "${ARCH:-universal}" = "arm64" ]; then
  echo "==> building arm64 only"
  cargo build --release --offline --target aarch64-apple-darwin
  mkdir -p "${UNI}"
  cp target/aarch64-apple-darwin/release/ops "${UNI}/ops"
  cp target/aarch64-apple-darwin/release/ops-gui "${UNI}/ops-gui"
else
  echo "==> building arm64 + x86_64"
  cargo build --release --offline --target aarch64-apple-darwin
  cargo build --release --offline --target x86_64-apple-darwin
  mkdir -p "${UNI}"
  lipo -create \
    target/aarch64-apple-darwin/release/ops \
    target/x86_64-apple-darwin/release/ops \
    -output "${UNI}/ops"
  lipo -create \
    target/aarch64-apple-darwin/release/ops-gui \
    target/x86_64-apple-darwin/release/ops-gui \
    -output "${UNI}/ops-gui"
fi
chmod +x "${UNI}/ops" "${UNI}/ops-gui"
lipo -info "${UNI}/ops"
lipo -info "${UNI}/ops-gui"

echo "==> building ${APP}"
rm -rf "${APP}"
mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources/bin"
cp "${UNI}/ops-gui" "${APP}/Contents/MacOS/OPS"
cp "${UNI}/ops" "${APP}/Contents/Resources/bin/ops"
cp README.md TUTORIAL.md "${APP}/Contents/Resources/"
cp assets/icon.icns "${APP}/Contents/Resources/icon.icns"
cat > "${APP}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>OPS</string>
  <key>CFBundleDisplayName</key><string>OPS</string>
  <key>CFBundleIdentifier</key><string>local.ops.app</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleExecutable</key><string>OPS</string>
  <key>CFBundleIconFile</key><string>icon.icns</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
codesign --force --deep --sign - "${APP}" >/dev/null 2>&1 || true

echo "==> generating DMG background"
python3 scripts/make_dmg_background.py "${BG}" >/dev/null

echo "==> staging DMG"
rm -rf "${STAGE}" "${TMP}" "${DMG}"
mkdir -p "${STAGE}/.background"
cp -R "${APP}" "${STAGE}/"
ln -s /Applications "${STAGE}/Applications"
cp README.md TUTORIAL.md "${STAGE}/"
cp "${BG}" "${STAGE}/.background/background.png"
cp assets/icon.icns "${STAGE}/.VolumeIcon.icns"
chflags hidden "${STAGE}/.background" "${STAGE}/.VolumeIcon.icns"

hdiutil create -volname "${VOLNAME}" \
  -srcfolder "${STAGE}" -ov -format UDRW -fs HFS+ "${TMP}" >/dev/null

MNT="$(mktemp -d "${TMPDIR:-/tmp}/ops_dmg.XXXXXX")"
hdiutil attach "${TMP}" -nobrowse -mountpoint "${MNT}" >/dev/null
SetFile -a C "${MNT}" 2>/dev/null || true

echo "==> arranging Finder window"
if ! osascript <<APPLESCRIPT
tell application "Finder"
  tell disk "${VOLNAME}"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 120, 820, 620}
    set opts to the icon view options of container window
    set arrangement of opts to not arranged
    set icon size of opts to 96
    set text size of opts to 12
    set background picture of opts to file ".background:background.png"
    set position of item "OPS.app" of container window to {155, 190}
    set position of item "Applications" of container window to {465, 190}
    set position of item "README.md" of container window to {155, 360}
    set position of item "TUTORIAL.md" of container window to {340, 360}
    close
    open
    update without registering applications
    delay 2
  end tell
end tell
APPLESCRIPT
then
  echo "warning: Finder layout failed; DMG will have the default icon layout" >&2
fi

sync
hdiutil detach "${MNT}" >/dev/null 2>&1 || hdiutil detach "$(hdiutil info | awk -v img="${TMP}" '$0 ~ img {getline; print $3; exit}')" >/dev/null 2>&1 || true
rmdir "${MNT}" 2>/dev/null || true

echo "==> converting DMG"
hdiutil convert "${TMP}" -format UDZO -imagekey zlib-level=9 -o "${DMG%.dmg}" >/dev/null
rm -f "${TMP}"
codesign --force --sign - "${DMG}" >/dev/null 2>&1 || true
hdiutil verify "${DMG}" >/dev/null

shasum -a 256 "${DMG}" > "${DMG}.sha256"
rm -rf "${STAGE}"

echo
echo "Bundle ready:"
echo "  ${DMG}"
echo "  ${DMG}.sha256"
echo "  ${UNI}/ops  ${UNI}/ops-gui"
