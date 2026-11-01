#!/bin/sh
# Assembles the GitHub Pages site: the landing page, the README screenshots, the demo downloads and
# the mdBook user guide. Used by .github/workflows/pages.yml; run it locally to preview:
#
#   sh site/build.sh                 # into target/site (needs mdbook on PATH)
#   python3 -m http.server -d target/site
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-"$root/target/site"}

rm -rf "$out"
mkdir -p "$out/images" "$out/demos"

cp "$root/site/index.html" "$root/site/style.css" "$root/site/favicon.svg" "$out/"
cp "$root"/docs/images/*.png "$out/images/"

cp "$root"/demos/*.gt "$out/demos/"
cp "$root"/demos/gp5/*.gp5 "$root"/demos/midi/*.mid "$root"/demos/tab/*.txt "$out/demos/"

mdbook build "$root/book" --dest-dir "$out/guide"

# Serve files as they are; GitHub Pages would otherwise run Jekyll over them.
touch "$out/.nojekyll"
