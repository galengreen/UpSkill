#!/usr/bin/env bash
# Package a finished release build for the updater and sign it.
# Usage: scripts/package.sh <darwin-aarch64|linux-x86_64>
# Needs TAURI_SIGNING_PRIVATE_KEY (and TAURI_SIGNING_PRIVATE_KEY_PASSWORD) set.
set -euo pipefail
cd "$(dirname "$0")/.."

platform=${1:?usage: scripts/package.sh <platform>}
mkdir -p release
archive="release/UpSkill-$platform.tar.gz"

case "$platform" in
  darwin-*)
    COPYFILE_DISABLE=1 tar -czf "$archive" -C target/release/bundle/macos UpSkill.app
    ;;
  linux-*)
    stage=$(mktemp -d)
    cp target/release/upskill-app target/release/upskill "$stage/"
    cp src-tauri/icons/128x128@2x.png "$stage/upskill.png"
    tar -czf "$archive" -C "$stage" upskill-app upskill upskill.png
    rm -rf "$stage"
    ;;
  *) echo "unknown platform $platform" >&2; exit 1 ;;
esac

bunx tauri signer sign "$archive" >/dev/null
echo "packaged $archive (+ .sig)"
