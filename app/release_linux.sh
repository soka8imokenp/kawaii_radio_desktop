#!/usr/bin/env bash
# Linux release, run inside WSL Ubuntu (after `python release.py` on Windows wrote dist/latest.json):
#   wsl -d Ubuntu -- bash /mnt/d/kawaii_radio_exe/app/release_linux.sh
# Builds the .deb (install with apt / dpkg) and the AppImage (portable; the in-app updater replaces it in place),
# copies them to dist/ and adds the linux-x86_64 entry to dist/latest.json. Builds in the WSL home (fast disk).
set -euo pipefail
SRC=/mnt/d/kawaii_radio_exe/app
DIST=/mnt/d/kawaii_radio_exe/dist
B=~/kr-desk-build
KEY=/mnt/c/Users/soka/kawaii-keys/desk-updater.key
mkdir -p "$B"
rsync -a --delete --exclude target --exclude gen "$SRC/" "$B/"
cd "$B"
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
export NO_STRIP=true      # linuxdeploy's bundled strip fails on newer system libraries
cargo tauri build --bundles deb,appimage
V=$(python3 -c "import json;print(json.load(open('tauri.conf.json'))['version'])")
DEB=$(ls target/release/bundle/deb/*_"$V"_amd64.deb)
AI=$(ls target/release/bundle/appimage/*_"$V"_amd64.AppImage)
cp "$DEB" "$DIST/KawaiiRadio-${V}_amd64.deb"
cp "$AI" "$DIST/KawaiiRadio-${V}_amd64.AppImage"
python3 - "$V" "$AI.sig" <<'PY'
import json, sys
v, sig = sys.argv[1], open(sys.argv[2]).read().strip()
p = '/mnt/d/kawaii_radio_exe/dist/latest.json'
j = json.load(open(p, encoding='utf-8'))
assert j['version'] == v, 'run release.py for this version first'
j['platforms']['linux-x86_64'] = {'signature': sig, 'url': f'https://raw.githubusercontent.com/soka8imokenp/kawaii_radio_desktop/main/dist/KawaiiRadio-{v}_amd64.AppImage'}
json.dump(j, open(p, 'w', encoding='utf-8'), ensure_ascii=False, indent=2)
print('LINUX', v)
PY
ls -la "$DIST" | grep "$V"
