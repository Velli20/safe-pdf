#!/usr/bin/env bash
# Completes the GitHub Pages site staged in <site-dir> before it is deployed.
#
# Pages holds a single deployment, so every deploy must carry the whole site:
#   <site-dir>/               web-canvas demo  (artifact `pages-web-canvas`, ci.yml)
#   <site-dir>/conformance/   conformance viewer (artifact `pages-conformance`, conformance.yml)
# The workflow that deploys stages its own half; this script fetches the missing half from
# the newest run on main that still has the artifact. A half that cannot be found is left
# out with a warning. Needs `gh` with GH_TOKEN and `actions: read`.
set -euo pipefail

site=${1:?usage: assemble-pages.sh <site-dir>}
repo=${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}
mkdir -p "$site"

# fetch <workflow file> <artifact name> <destination>
fetch() {
  local workflow=$1 artifact=$2 dest=$3 id
  for id in $(gh run list --repo "$repo" --workflow "$workflow" --branch main --limit 10 \
      --json databaseId --jq '.[].databaseId'); do
    if gh run download "$id" --repo "$repo" --name "$artifact" --dir "$dest" 2>/dev/null; then
      echo "Fetched $artifact from $workflow run $id"
      return 0
    fi
  done
  echo "::warning ::No $artifact artifact in recent $workflow runs on main; deploying without it."
}

if [ ! -f "$site/index.html" ]; then
  fetch ci.yml pages-web-canvas "$site"
fi
if [ ! -d "$site/conformance" ]; then
  fetch conformance.yml pages-conformance "$site/conformance"
fi

size_kb=$(du -sk "$site" | cut -f1)
echo "Pages site: $((size_kb / 1024)) MB"
if [ "$size_kb" -gt $((900 * 1024)) ]; then
  echo "::warning ::Pages site is $((size_kb / 1024)) MB; GitHub Pages sites are limited to 1 GB."
fi
