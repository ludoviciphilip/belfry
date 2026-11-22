#!/usr/bin/env bash
# Downloads the pinned EasyTier release for one platform, checks its SHA-256
# and puts the engine files into src-tauri/engine/ (bundled as a resource).
#   scripts/fetch-engine.sh linux|windows
set -euo pipefail

VERSION="v2.6.4"
declare -A SHA=(
  [linux]="61b659eaedba658fa66fe47d17e1426cdd77e5d02fa15fed447bb4357c09dfd6"
  [windows]="27af91e270e554709b048bd32327fefd2dfce5062ae1e8701af7550c6f525f84"
)
declare -A FILES=(
  [linux]="easytier-core easytier-cli"
  [windows]="easytier-core.exe easytier-cli.exe wintun.dll Packet.dll WinDivert64.sys"
)

os="${1:-linux}"
[[ -n "${SHA[$os]:-}" ]] || { echo "usage: $0 linux|windows" >&2; exit 2; }
root="$(cd "$(dirname "$0")/.." && pwd)"
zip="easytier-${os}-x86_64-${VERSION}.zip"
cache="$root/.cache"
mkdir -p "$cache"
if [[ ! -f "$cache/$zip" ]]; then
  curl -fsSL -o "$cache/$zip.part" "https://github.com/EasyTier/EasyTier/releases/download/${VERSION}/${zip}"
  mv "$cache/$zip.part" "$cache/$zip"
fi
echo "${SHA[$os]}  $cache/$zip" | sha256sum -c -

out="$root/src-tauri/engine"
rm -rf "$out" "$cache/x"
mkdir -p "$out" "$cache/x"
if command -v unzip >/dev/null; then
  unzip -q -o "$cache/$zip" -d "$cache/x"
else # Windows runners without unzip
  powershell -NoProfile -Command "Expand-Archive -Force '$cache/$zip' '$cache/x'"
fi
for f in ${FILES[$os]}; do
  cp "$cache/x/easytier-${os}-x86_64/$f" "$out/"
done
chmod +x "$out"/easytier-* 2>/dev/null || true
echo "$VERSION" > "$out/VERSION"
# LGPL-3.0: ship the license and say where the exact source lives.
cp "$root/licenses/EasyTier-LGPL-3.0.txt" "$out/LICENSE-EasyTier.txt"
echo "EasyTier $VERSION, unmodified. Source: https://github.com/EasyTier/EasyTier/tree/$VERSION" > "$out/SOURCE.txt"
ls -la "$out"
