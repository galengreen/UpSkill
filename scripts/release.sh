#!/usr/bin/env bash
# Cut a release: bump the version everywhere, commit, tag and push.
# GitHub Actions then builds, signs and publishes it, and every installed
# UpSkill picks it up within six hours.
# Usage: scripts/release.sh 0.2.0
set -euo pipefail
cd "$(dirname "$0")/.."

v=${1:?usage: scripts/release.sh <version>}
v=${v#v}
[[ $v =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must look like 1.2.3" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "commit or stash your changes first" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$v" >/dev/null && { echo "v$v already exists" >&2; exit 1; }

perl -pi -e 's/^version = "[^"]+"/version = "'"$v"'"/ if $. < 10' Cargo.toml
perl -pi -e 's/^  "version": "[^"]+"/  "version": "'"$v"'"/' package.json src-tauri/tauri.conf.json
cargo update -q -w

git commit -qam "Release v$v"
git tag -a "v$v" -m "UpSkill $v"
git push -q origin HEAD "v$v"
echo "Pushed v$v. Building: https://github.com/galengreen/UpSkill/actions"
