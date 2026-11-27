#!/usr/bin/env bash
# Build the Chessy WASM, then pack chessy.dna and chessy.happ.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

if ! command -v hc >/dev/null 2>&1; then
  echo "The hc CLI is not on PATH. Install Holochain 0.7.1 (see docs/local-development.md)." >&2
  exit 1
fi

cargo build --release --target wasm32-unknown-unknown -p chessy_integrity -p chessy

wasm_dir="$root/dnas/chessy/workdir/wasm"
mkdir -p "$wasm_dir"
cp "$root/target/wasm32-unknown-unknown/release/chessy_integrity.wasm" "$wasm_dir/"
cp "$root/target/wasm32-unknown-unknown/release/chessy.wasm" "$wasm_dir/"

hc dna pack "$root/dnas/chessy/workdir" -o "$root/dnas/chessy/workdir/chessy.dna"
hc app pack "$root/dnas/chessy/workdir" -o "$root/dnas/chessy/workdir/chessy.happ"

echo "Packed $root/dnas/chessy/workdir/chessy.happ"
