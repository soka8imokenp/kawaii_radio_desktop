#!/usr/bin/env bash
# Linux release of the native app, run inside WSL Ubuntu after `python release.py` (it wrote dist/latest.json):
#   wsl -d Ubuntu -- bash /mnt/d/kawaii_radio_exe/native/release_linux.sh
# Builds in the WSL home (fast disk), makes the .deb (apt installs its few system libraries by itself) and the
# AppImage (one file, the in-app updater replaces it in place), signs both with the updater key and adds them to
# dist/latest.json (linux-x86_64 = AppImage, linux-x86_64-deb = .deb).
set -euo pipefail
ROOT=/mnt/d/kawaii_radio_exe
DIST=$ROOT/dist
B=~/kr-native-build
KEY=/mnt/c/Users/soka/kawaii-keys/desk-updater.key
mkdir -p "$B/app/ui" "$B/app/icons"
rsync -a --delete --exclude target "$ROOT/native/" "$B/native/"
rsync -a --delete "$ROOT/app/ui/assets/" "$B/app/ui/assets/"
rsync -a --delete "$ROOT/app/ui/fonts/" "$B/app/ui/fonts/"
rsync -a --delete "$ROOT/app/icons/" "$B/app/icons/"

# the ALSA headers without root: the -dev package unpacked next to the build
if ! pkg-config --exists alsa 2>/dev/null; then
  mkdir -p "$B/sysroot" && cd "$B/sysroot"
  [ -f libasound2-dev.deb ] || { apt-get download libasound2-dev && mv libasound2-dev_*.deb libasound2-dev.deb; }
  dpkg-deb -x libasound2-dev.deb .
  # the .so link points at the runtime library the system already has
  ln -sf /usr/lib/x86_64-linux-gnu/libasound.so.2 usr/lib/x86_64-linux-gnu/libasound.so
  sed -i "s|^prefix=.*|prefix=$B/sysroot/usr|" usr/lib/x86_64-linux-gnu/pkgconfig/alsa.pc
  export PKG_CONFIG_PATH="$B/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig"
  export RUSTFLAGS="-L $B/sysroot/usr/lib/x86_64-linux-gnu"
fi

cd "$B/native"
cargo build --release
V=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
BIN=target/release/kawaii-radio
strip "$BIN" || true

# ── the .deb
P="$B/pkg/kawaii-radio_${V}_amd64"
rm -rf "$B/pkg" && mkdir -p "$P/DEBIAN" "$P/usr/bin" "$P/usr/share/applications" "$P/usr/share/icons/hicolor/128x128/apps"
cp "$BIN" "$P/usr/bin/kawaii-radio"
cp "$B/app/icons/128x128.png" "$P/usr/share/icons/hicolor/128x128/apps/kawaii-radio.png"
cat > "$P/usr/share/applications/kawaii-radio.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Kawaii Radio
Comment=Jonli J-Pop / OST / Riot radio
Exec=kawaii-radio %u
Icon=kawaii-radio
Categories=AudioVideo;Audio;Player;
MimeType=x-scheme-handler/kawaii;
StartupWMClass=kawaii-radio
EOF
cat > "$P/DEBIAN/control" <<EOF
Package: kawaii-radio
Version: $V
Architecture: amd64
Maintainer: Kawaii Uz <radio@kawaii.uz>
Section: sound
Priority: optional
Depends: libasound2 | libasound2t64, libssl3 | libssl3t64, libdbus-1-3, libxkbcommon0, libvulkan1 | libgl1
Description: Kawaii Radio — live J-Pop / OST / Riot radio
 The KR-01 desk radio: live stations, lyrics, chat, Kawaii ID.
EOF
dpkg-deb --root-owner-group --build "$P" "$DIST/KawaiiRadio-${V}_amd64.deb"

# ── the AppImage (linuxdeploy from tauri's cache)
LD=$(ls ~/.cache/tauri/linuxdeploy-*-x86_64.AppImage | head -1)
APD="$B/AppDir" && rm -rf "$APD"
mkdir -p "$APD/usr/bin"
cp "$BIN" "$APD/usr/bin/kawaii-radio"
cd "$B"
export NO_STRIP=true ARCH=x86_64 OUTPUT="$DIST/KawaiiRadio-${V}_amd64.AppImage" APPIMAGE_EXTRACT_AND_RUN=1
PATH="$HOME/.cache/tauri:$PATH" "$LD" --appdir "$APD" -e "$APD/usr/bin/kawaii-radio" \
  -d "$P/usr/share/applications/kawaii-radio.desktop" -i "$P/usr/share/icons/hicolor/128x128/apps/kawaii-radio.png" \
  --output appimage
chmod +x "$DIST/KawaiiRadio-${V}_amd64.AppImage"

# ── signatures and the manifest
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
for f in "KawaiiRadio-${V}_amd64.AppImage" "KawaiiRadio-${V}_amd64.deb"; do
  cargo tauri signer sign "$DIST/$f" >/dev/null
done
python3 - "$V" <<'PY'
import json, os, sys
v = sys.argv[1]
d = '/mnt/d/kawaii_radio_exe/dist'
raw = 'https://raw.githubusercontent.com/soka8imokenp/kawaii_radio_desktop/main/dist/'
p = os.path.join(d, 'latest.json')
j = json.load(open(p, encoding='utf-8'))
assert j['version'] == v, 'run release.py for this version first'
for key, f in [('linux-x86_64', f'KawaiiRadio-{v}_amd64.AppImage'), ('linux-x86_64-deb', f'KawaiiRadio-{v}_amd64.deb')]:
    sig = open(os.path.join(d, f + '.sig')).read().strip()
    os.remove(os.path.join(d, f + '.sig'))
    j['platforms'][key] = {'signature': sig, 'url': raw + f}
json.dump(j, open(p, 'w', encoding='utf-8'), ensure_ascii=False, indent=2)
print('LINUX', v)
PY
ls -la "$DIST" | grep "$V"
