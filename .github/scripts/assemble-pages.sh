#!/usr/bin/env bash
# Completes the GitHub Pages site staged in <site-dir> before it is deployed.
#
# Pages holds a single deployment, so every deploy must carry the whole site:
#   <site-dir>/               web-canvas demo  (artifact `pages-web-canvas`, ci.yml)
#   <site-dir>/conformance/   conformance viewer (artifact `pages-conformance`, conformance.yml)
# The workflow that deploys stages its own half; this script fetches the missing half from
# the newest run on main that still has the artifact. A missing web-canvas demo fails the
# deploy, so the live demo is never replaced by a site without it; missing conformance
# reports are left out with a warning. Needs `gh` with GH_TOKEN and `actions: read`.
#
# GitHub Pages sites are limited to 1 GB, and a larger deployment can drop the demo. When
# the site is over SITE_BUDGET_MB, the conformance reports are left out so the demo still
# deploys.
set -euo pipefail

site=${1:?usage: assemble-pages.sh <site-dir>}
repo=${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}
budget_mb=${SITE_BUDGET_MB:-900}
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
  return 1
}

size_mb() {
  echo $(( $(du -sb "$1" | cut -f1) / 1024 / 1024 ))
}

if [ ! -f "$site/index.html" ]; then
  if ! fetch ci.yml pages-web-canvas "$site" || [ ! -f "$site/index.html" ]; then
    echo "::error ::No pages-web-canvas artifact in recent ci.yml runs on main; not deploying a site without the web-canvas demo."
    exit 1
  fi
fi
if [ ! -d "$site/conformance" ]; then
  if ! fetch conformance.yml pages-conformance "$site/conformance"; then
    echo "::warning ::No pages-conformance artifact in recent conformance.yml runs on main; deploying without it."
  fi
fi

echo "Pages site: $(size_mb "$site") MB (budget $budget_mb MB)"
if [ "$(size_mb "$site")" -gt "$budget_mb" ] && [ -d "$site/conformance" ]; then
  rm -rf "$site/conformance"
  echo "::warning ::Pages site is over $budget_mb MB; deploying without the conformance reports ($(size_mb "$site") MB left)."
fi
if [ "$(size_mb "$site")" -gt "$budget_mb" ]; then
  echo "::error ::Pages site is $(size_mb "$site") MB, over the $budget_mb MB budget."
  exit 1
fi
