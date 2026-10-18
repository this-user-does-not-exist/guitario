#!/bin/sh
# Installs Guitario from this folder for the current user (~/.local), or system-wide with
# PREFIX=/usr/local (run as root). Registers the file types and the icons.
#
#   ./install.sh            # or: PREFIX=/usr/local sudo -E ./install.sh
#   ./install.sh --remove
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
SHARE="$PREFIX/share"
case "${1:-}" in
  "" | --remove) ;;
  *) echo "usage: $0 [--remove]" >&2; exit 2 ;;
esac
if [ "${1:-}" = "--remove" ]; then
  rm -f "$PREFIX/bin/guitario" "$PREFIX/bin/guitario-cli" \
    "$SHARE/applications/ai.storyteller.guitario.desktop" \
    "$SHARE/mime/packages/guitario-mime.xml" \
    "$SHARE/metainfo/ai.storyteller.guitario.metainfo.xml"
  find "$SHARE/icons/hicolor" -name 'ai.storyteller.guitario.*' -delete 2>/dev/null || true
else
  mkdir -p "$PREFIX/bin" "$SHARE/applications" "$SHARE/mime/packages" "$SHARE/metainfo"
  install -m 755 "$HERE/bin/guitario" "$HERE/bin/guitario-cli" "$PREFIX/bin/"
  install -m 644 "$HERE/share/applications/ai.storyteller.guitario.desktop" "$SHARE/applications/"
  install -m 644 "$HERE/share/mime/packages/guitario-mime.xml" "$SHARE/mime/packages/"
  install -m 644 "$HERE/share/metainfo/ai.storyteller.guitario.metainfo.xml" "$SHARE/metainfo/"
  cp -R "$HERE/share/icons" "$SHARE/"
fi
command -v update-mime-database >/dev/null && update-mime-database "$SHARE/mime" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$SHARE/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$SHARE/icons/hicolor" || true
if [ "${1:-}" = "--remove" ]; then echo "Guitario removed from $PREFIX"; else echo "Guitario installed in $PREFIX"; fi
