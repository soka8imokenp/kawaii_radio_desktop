#!/usr/bin/env bash
# Kawaii Radio — Linux o'rnatuvchisi (Ubuntu / Debian / Mint / Pop!_OS — .deb; Fedora / openSUSE — .rpm).
# Bitta buyruq bilan, kerakli tizim kutubxonalari (ovoz, shifrlash) bilan birga:
#   curl -fsSL https://radio.kawaii.uz/download/install.sh | bash
set -euo pipefail
BASE=https://radio.kawaii.uz/download
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
if command -v apt-get >/dev/null 2>&1; then
  KIND=deb; FILE="$TMP/kawaii-radio.deb"
elif command -v dnf >/dev/null 2>&1 || command -v zypper >/dev/null 2>&1; then
  KIND=rpm; FILE="$TMP/kawaii-radio.rpm"
else
  echo "Bu tizim uchun AppImage'ni oling: $BASE/appimage (chmod +x qilib ishga tushiring)"; exit 1
fi
echo "Kawaii Radio yuklab olinmoqda…"
curl -fL --proto '=https' --tlsv1.2 --progress-bar -o "$FILE" "$BASE/$KIND"
echo "O'rnatilmoqda (parol so'ralishi mumkin)…"
case "$KIND" in
  deb) sudo apt-get update -qq; sudo apt-get install -y "$FILE" ;;
  rpm) if command -v dnf >/dev/null 2>&1; then sudo dnf install -y "$FILE"; else sudo zypper --non-interactive install "$FILE"; fi ;;
esac
echo "Tayyor! Menyudan \"Kawaii Radio\" ni oching."
