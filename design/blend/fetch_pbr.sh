#!/usr/bin/env bash
# Downloads the scanned CC0 surfaces (ambientCG) that lib.py's Ctx.pbr() uses into design/blend/pbr/.
# They are not in git (≈80 MB). Without them the renders still work, just without the scanned grain.
set -e
cd "$(dirname "$0")"
mkdir -p pbr && cd pbr
for a in Plastic006 Plastic010 Plastic013A Fingerprints002; do
  [ -d "$a" ] && continue
  curl -sL -o "$a.zip" "https://ambientcg.com/get?file=${a}_2K-JPG.zip"
  mkdir -p "$a" && unzip -oq "$a.zip" -d "$a" && rm "$a.zip"
done
echo done
