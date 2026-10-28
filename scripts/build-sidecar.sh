#!/usr/bin/env bash
# Build the `upskill` CLI/MCP binary and stage it as a Tauri sidecar.
set -euo pipefail
cd "$(dirname "$0")/.."
triple=$(rustc -vV | sed -n 's/^host: //p')
cargo build --release -p upskill-cli
mkdir -p src-tauri/binaries
cp target/release/upskill "src-tauri/binaries/upskill-$triple"
echo "staged src-tauri/binaries/upskill-$triple"
