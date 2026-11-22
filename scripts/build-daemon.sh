#!/usr/bin/env bash
# Builds belfryd and places it where Tauri expects the sidecar:
# src-tauri/binaries/belfryd-<target-triple>[.exe]
#   scripts/build-daemon.sh [debug|release]
set -euo pipefail
profile="${1:-release}"
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
triple="$(rustc -vV | sed -n 's/^host: //p')"
if [[ "$profile" == release ]]; then
  cargo build -p belfryd --release
else
  cargo build -p belfryd
fi
ext=""
[[ "$triple" == *windows* ]] && ext=".exe"
mkdir -p src-tauri/binaries
cp "target/$profile/belfryd$ext" "src-tauri/binaries/belfryd-$triple$ext"
# Dev runs (`cargo run -p belfry`) look for the sidecar next to the app binary.
cp "target/$profile/belfryd$ext" "target/$profile/" 2>/dev/null || true
echo "sidecar: src-tauri/binaries/belfryd-$triple$ext"
