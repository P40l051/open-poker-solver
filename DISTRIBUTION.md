# Distribution

OPS ships as a self-contained Rust binary with no runtime dependencies. The
desktop UI is a local HTTP server that opens the default browser (or Chrome in
app mode), so no Electron, Python or extra runtime is required.

## 1. Build a macOS DMG (universal)

```bash
./scripts/package_dmg.sh
```

This builds **arm64 + x86_64**, creates a universal `OPS.app`, signs it ad-hoc,
and produces:

| Artifact | Content |
|---|---|
| `dist/OPS-<ver>-macos-universal.dmg` | drag-to-Applications installer |
| `dist/OPS-<ver>-macos-universal.dmg.sha256` | checksum |
| `dist/universal/ops`, `dist/universal/ops-gui` | universal CLI/GUI binaries |

The DMG contains:

- `OPS.app`
- an `Applications` shortcut
- `README.md`
- `TUTORIAL.md`
- Finder icon positions for the drag-to-Applications layout (the regular
  macOS build also sets a background image; restricted/sandboxed builds may
  skip it)

`package.sh` also calls `package_dmg.sh` at the end. Skip it with:

```bash
BUILD_DMG=0 ./scripts/package.sh
```

### arm64-only fallback

If the x86_64 Rust standard library is unavailable (offline machine):

```bash
ARCH=arm64 ./scripts/package_dmg.sh
```

The resulting DMG works on Apple Silicon only.

### Requirements for the universal build

- Apple Silicon Mac with Command Line Tools (Apple clang can target x86_64).
- `x86_64-apple-darwin` Rust standard library:
  ```bash
  rustup target add x86_64-apple-darwin
  ```
  Without network access, copy the `rust-std` component for
  `x86_64-apple-darwin` into:
  ```bash
  $(rustc --print sysroot)/lib/rustlib/x86_64-apple-darwin/
  ```

## 2. macOS installation

1. Copy the DMG to the target Mac.
2. Double-click it and drag **OPS** into **Applications**.
3. First launch: right-click the app → **Open** (ad-hoc signed, not
   notarized). Alternatively:
   ```bash
   xattr -dr com.apple.quarantine /Applications/OPS.app
   ```
4. The app starts the server and opens the window. **Quit** in the header
   stops the server. If port 7788 is busy, OPS tries 7789, 7790, …

Requirements: macOS 11+, Apple Silicon or Intel (universal). Chrome is
optional: if missing, the default browser is opened.

## 3. Signing and notarization (public distribution)

For warning-free distribution outside your own Mac you need a Developer ID
certificate and notarization:

```bash
# 1. Sign the app before creating the DMG
codesign --force --deep --options runtime \
  --sign "Developer ID Application: Name (TEAMID)" dist/OPS.app

# 2. Create the DMG (./scripts/package_dmg.sh)

# 3. Sign the DMG
codesign --force --sign "Developer ID Application: Name (TEAMID)" \
  dist/OPS-<ver>-macos-universal.dmg

# 4. Notarize and staple
xcrun notarytool submit dist/OPS-<ver>-macos-universal.dmg \
  --apple-id "..." --team-id "TEAMID" \
  --password "app-specific-password" --wait
xcrun stapler staple dist/OPS-<ver>-macos-universal.dmg

# 5. Verify
spctl -a -vv -t open --context context:primary-signature \
  dist/OPS-<ver>-macos-universal.dmg
```

Without these steps the app still works with right-click → Open.

## 4. Windows

Compile **on Windows** (cross-compilation is not configured):

```powershell
cargo build --release
```

Ship `ops.exe`, `ops-gui.exe`, `README.md` and `TUTORIAL.md` in a folder. The
GUI opens the default browser with `cmd /C start`. Add a `.bat` launcher or an
Inno Setup installer for non-technical users. Unsigned executables trigger
SmartScreen warnings.

## 5. Linux

```bash
cargo build --release
./target/release/ops-gui          # opens xdg-open
```

For maximum portability use musl:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

The result is a static binary independent of the glibc version.

## 6. Verify a release

```bash
hdiutil verify dist/OPS-*.dmg
hdiutil attach dist/OPS-*.dmg
lipo -info /Volumes/OPS/OPS.app/Contents/MacOS/OPS
/Volumes/OPS/OPS.app/Contents/MacOS/OPS --version
hdiutil detach /Volumes/OPS
```

Expected `lipo` output: `x86_64 arm64`.

## 7. Do not ship

- `target/` (hundreds of MB, rebuildable)
- `dist/dmgroot/` and temporary `*.rw.dmg`
- personal hand histories or solution files

The compressed DMG is a few MB.
