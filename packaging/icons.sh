#!/usr/bin/env bash
# Regenerate every app icon from assets/app-icon/guitario.svg (the canonical master), and,
# with --trace, the two SVGs from assets/app-icon/source.png first.
#
# Needs: resvg (brew install resvg / cargo install resvg); --trace also needs potrace and
# ImageMagick. On macOS, iconutil also writes the .icns. The outputs are committed, so packaging
# never needs these tools.
#
#   packaging/icons.sh [--trace]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/app-icon"
SVG="$DIR/guitario.svg"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

if [[ "${1:-}" == "--trace" ]]; then
  for tool in potrace magick python3; do
    command -v "$tool" >/dev/null || { echo "error: $tool not found" >&2; exit 1; }
  done
  # The source is white line work on black, 768 px. Upscale, smooth the edges' noise, then trace
  # the white (potrace traces black, hence -negate): at 4x for the master, 2x for the lighter one.
  magick "$DIR/source.png" -colorspace gray -filter Lanczos -resize 400% -blur 0x5 -threshold 50% -negate "$TMP/art.pbm"
  potrace "$TMP/art.pbm" -s -o "$TMP/art.svg" -t 60 -a 1.2 -O 1.0
  magick "$DIR/source.png" -colorspace gray -filter Lanczos -resize 200% -blur 0x2.5 -threshold 50% -negate "$TMP/small.pbm"
  potrace "$TMP/small.pbm" -s -o "$TMP/small.svg" -t 20 -a 1.2 -O 1.2
  # Centre the art in a 512-unit rounded tile, its longest side 350 units.
  BOX="$(magick "$DIR/source.png" -colorspace gray -threshold 50% -format '%@' info:)"
  python3 -I - "$BOX" "$TMP/art.svg" "$SVG" "$TMP/small.svg" "$DIR/guitario-small.svg" <<'PY'
import re, sys
box, pairs = sys.argv[1], sys.argv[2:]
w, h, x0, y0 = map(int, re.match(r"(\d+)x(\d+)\+(\d+)\+(\d+)", box).groups())
cx, cy = x0 + w / 2, y0 + h / 2
for (src, out), factor in zip([pairs[0:2], pairs[2:4]], [4, 2]):
    d = " ".join(p.replace("\n", " ") for p in re.findall(r'<path d="([^"]+)"', open(src).read()))
    s = 350.0 / max(w, h) / factor
    tf = f"translate(256 256) scale({s:.6f}) translate({-cx * factor:.3f} {-cy * factor:.3f}) translate(0 {768 * factor}) scale(0.1 -0.1)"
    open(out, "w").write(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">'
        '<defs><clipPath id="t"><rect width="512" height="512" rx="112"/></clipPath></defs>'
        '<g clip-path="url(#t)"><rect width="512" height="512" fill="#0b0b0c"/>'
        f'<path fill="#ffffff" transform="{tf}" d="{d}"/></g></svg>\n'
    )
PY
fi

# The artwork is a full-bleed 512-unit tile (rx=112). macOS icons pad it to Apple's 824/1024 body
# grid (transparent margin). Windows and Linux icons crop 22 units off each side (into the
# rounded corners) so the mark reads at 16-48 px.
grep -q 'viewBox="0 0 512 512"' "$SVG" || { echo "error: expected viewBox=\"0 0 512 512\" in $SVG" >&2; exit 1; }
MAC="$TMP/mac.svg"
sed 's/viewBox="0 0 512 512"/viewBox="-62 -62 636 636"/' "$SVG" >"$MAC"
TIGHT="$TMP/tight.svg"
sed 's/viewBox="0 0 512 512"/viewBox="22 22 468 468"/' "$SVG" >"$TIGHT"
SMALL="$TMP/small-tight.svg"
sed 's/viewBox="0 0 512 512"/viewBox="22 22 468 468"/' "$DIR/guitario-small.svg" >"$SMALL"

render() { resvg -w "$2" -h "$2" "$1" "$3" </dev/null; }

render "$MAC" 1024 "$DIR/guitario-1024.png"

# The mark alone (white line work, no tile) for the app's Homepage, which tints it.
MARK="$TMP/mark.svg"
sed -e 's/viewBox="0 0 512 512"/viewBox="76 76 360 360"/' -e 's|<rect width="512" height="512" fill="#0b0b0c"/>||' "$DIR/guitario-small.svg" >"$MARK"
render "$MARK" 192 "$DIR/guitario-mark.png"

# Linux hicolor theme: the lighter trace up to 64 px, the master above.
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  src="$TIGHT"; [[ $s -le 64 ]] && src="$SMALL"
  render "$src" "$s" "$DIR/hicolor/${s}x${s}/apps/ai.storyteller.guitario.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$DIR/guitario-small.svg" "$DIR/hicolor/scalable/apps/ai.storyteller.guitario.svg"

# Windows .ico.
ICO_PNGS=()
for s in 16 20 24 32 40 48 64 128 256; do
  src="$TIGHT"; [[ $s -le 64 ]] && src="$SMALL"
  render "$src" "$s" "$TMP/ico-$s.png"
  ICO_PNGS+=("$TMP/ico-$s.png")
done
(cd "$ROOT" && cargo run -q -p xtask -- ico "$DIR/guitario.ico" "${ICO_PNGS[@]}")

# macOS .icns.
if command -v iconutil >/dev/null; then
  SET="$TMP/guitario.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$MAC" "$s" "$SET/icon_${s}x${s}.png"
    render "$MAC" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/guitario.icns" "$SET"
else
  echo "warning: iconutil not found (macOS only); guitario.icns not regenerated" >&2
fi
echo "icons written to $DIR"
