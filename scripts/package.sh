#!/usr/bin/env bash
# Builds a self-contained release bundle for the current machine.
# Usage: ./scripts/package.sh
set -euo pipefail

cd "$(dirname "$0")/.."
export PATH="${HOME}/.cargo/bin:${PATH}"

VERSION="$(awk -F'"' '/^version/{print $2; exit}' Cargo.toml)"
OS="$(uname -s)"
RAW_ARCH="$(uname -m)"
case "${RAW_ARCH}" in
  arm64|aarch64) ARCH="arm64" ;;
  x86_64|amd64)  ARCH="x86_64" ;;
  *)             ARCH="${RAW_ARCH}" ;;
esac
case "${OS}" in
  Darwin)      PLAT="macos" ;;
  Linux)       PLAT="linux" ;;
  MINGW*|MSYS*|CYGWIN*) PLAT="windows" ;;
  *)           PLAT="$(echo "${OS}" | tr '[:upper:]' '[:lower:]')" ;;
esac

NAME="OPS-${VERSION}-${PLAT}-${ARCH}"
OUT="dist/${NAME}"
echo "==> building ${NAME}"
cargo build --release --offline

rm -rf "${OUT}"
mkdir -p "${OUT}"
cp target/release/ops target/release/ops-gui "${OUT}/"
cp assets/icon.png assets/icon.svg "${OUT}/"
cp README.md TUTORIAL.md "${OUT}/"

# --- launchers ---------------------------------------------------------------
if [ "${PLAT}" = "macos" ]; then
  cat > "${OUT}/Avvia OPS.command" <<'LAUNCH'
#!/bin/bash
cd "$(dirname "$0")"
exec ./ops-gui
LAUNCH
  chmod +x "${OUT}/Avvia OPS.command"
elif [ "${PLAT}" = "linux" ]; then
  cat > "${OUT}/run.sh" <<'LAUNCH'
#!/usr/bin/env bash
cd "$(dirname "$0")"
exec ./ops-gui
LAUNCH
  chmod +x "${OUT}/run.sh"
else
  cat > "${OUT}/run.bat" <<'LAUNCH'
@echo off
cd /d "%~dp0"
ops-gui.exe
pause
LAUNCH
fi

# --- macOS .app bundle -------------------------------------------------------
if [ "${PLAT}" = "macos" ]; then
  APP="dist/OPS.app"
  rm -rf "${APP}"
  mkdir -p "${APP}/Contents/MacOS" "${APP}/Contents/Resources/bin"
  cp target/release/ops-gui "${APP}/Contents/MacOS/OPS"
  cp target/release/ops "${APP}/Contents/Resources/bin/ops"
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
  (cd dist && ditto -c -k --sequesterRsrc --keepParent "OPS.app" "OPS-${VERSION}-${PLAT}-${ARCH}.zip")
  echo "==> dist/OPS-${VERSION}-${PLAT}-${ARCH}.zip"
fi

# --- archive -----------------------------------------------------------------
if [ "${PLAT}" = "windows" ]; then
  (cd dist && zip -qr "${NAME}.zip" "${NAME}")
  echo "==> dist/${NAME}.zip"
else
  tar -czf "dist/${NAME}.tar.gz" -C dist "${NAME}"
  echo "==> dist/${NAME}.tar.gz"
fi

echo
echo "Bundle ready:"
echo "  ${OUT}/"
echo "  ${OUT}/ops       (CLI)"
echo "  ${OUT}/ops-gui   (desktop app)"

if [ "${BUILD_DMG:-1}" = "1" ]; then
  echo
  "${PWD}/scripts/package_dmg.sh"
fi
