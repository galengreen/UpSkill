#!/bin/sh
# Install (or reinstall) UpSkill from the latest GitHub release.
#
#   curl -fsSL https://github.com/galengreen/UpSkill/releases/latest/download/install.sh | sh
#
# macOS → /Applications/UpSkill.app. Linux → ~/.local/bin (no sudo), plus a
# launcher entry. After this, UpSkill keeps itself up to date.
set -eu

BASE="https://github.com/galengreen/UpSkill/releases/latest/download"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=darwin-aarch64 ;;
  Linux-x86_64) platform=linux-x86_64 ;;
  *) echo "UpSkill has no build for $(uname -s) $(uname -m) yet." >&2; exit 1 ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "Downloading UpSkill ($platform)…"
curl -fsSL "$BASE/UpSkill-$platform.tar.gz" -o "$tmp/upskill.tar.gz"
tar -xzf "$tmp/upskill.tar.gz" -C "$tmp"

if [ "$platform" = darwin-aarch64 ]; then
  osascript -e 'quit app "UpSkill"' >/dev/null 2>&1 || true
  sleep 1
  rm -rf /Applications/UpSkill.app
  mv "$tmp/UpSkill.app" /Applications/UpSkill.app
  open /Applications/UpSkill.app
  bin=/Applications/UpSkill.app/Contents/MacOS/upskill
else
  bindir="$HOME/.local/bin"
  mkdir -p "$bindir"
  pkill -x upskill-app >/dev/null 2>&1 || true
  install -m755 "$tmp/upskill-app" "$tmp/upskill" "$bindir/"
  install -Dm644 "$tmp/upskill.png" "$HOME/.local/share/icons/hicolor/256x256/apps/upskill.png"
  mkdir -p "$HOME/.local/share/applications"
  cat > "$HOME/.local/share/applications/upskill.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=UpSkill
Comment=One library for every agent's skills
Exec=$bindir/upskill-app
Icon=upskill
Terminal=false
Categories=Development;Utility;
StartupWMClass=upskill-app
EOF
  command -v update-desktop-database >/dev/null && update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
  if [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then
    setsid -f "$bindir/upskill-app" >/dev/null 2>&1 < /dev/null
  fi
  bin="$bindir/upskill"
fi

echo "Installed $("$bin" --version). It updates itself from here on."
