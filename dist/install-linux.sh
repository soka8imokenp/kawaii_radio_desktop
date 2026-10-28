#!/usr/bin/env bash
# Kawaii Radio — Linux (Ubuntu / Debian / Mint / Pop!_OS) o'rnatuvchisi.
# Bitta buyruq bilan: kerakli tizim kutubxonalari (ovoz, shifrlash) avtomatik o'rnatiladi.
#   curl -fsSL https://raw.githubusercontent.com/soka8imokenp/kawaii_radio_desktop/main/dist/install-linux.sh | bash
set -euo pipefail
RAW=https://raw.githubusercontent.com/soka8imokenp/kawaii_radio_desktop/main/dist
V=$(curl -fsSL "$RAW/latest.json" | sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' | head -1)
DEB="KawaiiRadio-${V}_amd64.deb"
TMP=$(mktemp -d)
echo "Kawaii Radio v$V yuklab olinmoqda…"
curl -fL --progress-bar -o "$TMP/$DEB" "$RAW/$DEB"
echo "O'rnatilmoqda (parol so'ralishi mumkin)…"
sudo apt-get update -qq
# apt installs the .deb together with every library it depends on (dpkg -i would not)
sudo apt-get install -y "$TMP/$DEB"
rm -rf "$TMP"
echo "Tayyor! Menyudan \"Kawaii Radio\" ni oching."
