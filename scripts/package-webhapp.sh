#!/usr/bin/env bash
# Zip the built UI and pack chessy.webhapp next to the .happ.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if ! command -v hc >/dev/null 2>&1; then
  echo "hc is not on PATH." >&2
  exit 1
fi

if [[ ! -d ui/dist ]]; then
  echo "ui/dist is missing. Run npm run build:ui first." >&2
  exit 1
fi

python3 "$root/scripts/make-icon.py"
cp "$root/ui/public/icon.png" "$root/ui/dist/icon.png"

workdir="$root/dnas/chessy/workdir"
rm -f "$workdir/ui.zip"
( cd "$root/ui/dist" && zip -qr "$workdir/ui.zip" . )

cat > "$workdir/web-happ.yaml" <<'EOF'
manifest_version: "0"
name: chessy
ui:
  path: ./ui.zip
happ:
  path: ./chessy.happ
EOF

if [[ ! -f "$workdir/chessy.happ" ]]; then
  "$root/scripts/build-happ.sh"
fi

hc web-app pack "$workdir" -o "$workdir/chessy.webhapp"
echo "Packed $workdir/chessy.webhapp"
