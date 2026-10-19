#!/usr/bin/env bash
# Static production build for GitHub Pages (or any plain file host): runs the normal build, then assembles build/site/ with exactly what the page
# needs. No game data, no assets/, no BIOS, no source maps. All page URLs are relative, so the folder works under any sub-path
# (https://markzuel.com/snatcher-wasm-port/) and at a root alike.
#   SKIP_BUILD=1 scripts/build-static.sh   reuse the existing web/pkg, web/dist and i18n/build/pt-BR.pack (no cargo, no wasm-bindgen)
set -euo pipefail
cd "$(dirname "$0")/.."
[ -n "${SKIP_BUILD:-}" ] || scripts/build-web.sh

site=build/site
rm -rf "$site"
mkdir -p "$site"/{dist,pkg,i18n/pt-BR,styles,fonts,icons}
touch "$site/.nojekyll"   # Pages must not run Jekyll over the folder
cp web/index.html "$site/index.html"
cp web/dist/main.js web/dist/import.worker.js "$site/dist/"
cp web/pkg/snengine.js web/pkg/snengine_bg.wasm "$site/pkg/"
cp i18n/build/pt-BR.pack "$site/i18n/pt-BR.pack"
cp i18n/pt-BR/ui.json "$site/i18n/pt-BR/ui.json"
cp web/styles/system.css "$site/styles/"
cp web/fonts/DotGothic16-subset.woff2 web/fonts/OFL.txt "$site/fonts/"
cp web/manifest.webmanifest "$site/"                  # home-screen app (display fullscreen / standalone); no service worker: nothing can serve a stale wasm
cp web/icons/icon-180.png web/icons/icon-192.png web/icons/icon-512.png "$site/icons/"   # drawn by scripts/make-icons.py from the page's own pixel S, no game art

# Version token: a hash of every file of the build. It goes into the URLs of the JS, CSS, wasm, pack and strings (?v=...), so after a deploy the browser
# (and the 10 minute Pages cache) can never combine an old file with a new one. Fonts do not change and keep plain URLs.
build=$(cd "$site" && cat index.html dist/main.js dist/import.worker.js pkg/snengine.js pkg/snengine_bg.wasm i18n/pt-BR.pack i18n/pt-BR/ui.json styles/system.css | sha256sum | cut -c1-12)
sed -i "s/@@BUILD@@/$build/g; /^\/\/# sourceMappingURL=/d" "$site/dist/main.js" "$site/dist/import.worker.js"
sed -i -e "s#src=\"dist/main.js\"#src=\"dist/main.js?v=$build\"#" \
       -e "s#href=\"styles/system.css\"#href=\"styles/system.css?v=$build\"#" \
       -e 's#^<meta name="color-scheme" content="dark">#&\n<meta name="sn-served-assets" content="none">#' "$site/index.html"   # static host: no served game copy, the page does not probe /assets

# self-check: the substitutions happened, nothing refers to the host root, nothing from the game is inside
! grep -rl "@@BUILD@@" "$site" || { echo "build token left in the site" >&2; exit 1; }
grep -q "dist/main.js?v=$build" "$site/index.html" && grep -q "styles/system.css?v=$build" "$site/index.html" && grep -q 'sn-served-assets' "$site/index.html" || { echo "index.html was not rewritten" >&2; exit 1; }
! grep -E '(src|href)="/' "$site/index.html" || { echo "absolute path in index.html" >&2; exit 1; }
! find "$site" -iname '*.map' -o -iname '*.bin' -o -iname '*.cue' -o -iname '*.chd' -o -iname '*.iso' -o -iname '*.ogg' -o -iname '*.wav' | grep -q . || { echo "forbidden file type in the site" >&2; exit 1; }
echo "static site: $site (build $build, $(du -sh "$site" | cut -f1))"
(cd "$site" && find . -type f | sort | xargs ls -l | awk '{print $5"\t"$9}')
