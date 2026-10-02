#!/usr/bin/env bash
# Posts or updates the pull request comment listing documents that read worse or better than
# on the base commit, from the `read-diff-<corpus>` artifacts downloaded into <diff-dir>.
#
# The comment is created only when something changed. Once it exists, later runs keep it
# current, including saying when nothing differs any more. Needs `gh` with GH_TOKEN and
# `pull-requests: write`, plus GITHUB_REPOSITORY, PR_NUMBER and RUN_URL.
set -euo pipefail

dir=${1:?usage: conformance-read-comment.sh <diff-dir>}
repo=${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}
pr=${PR_NUMBER:?PR_NUMBER is not set}
run_url=${RUN_URL:?RUN_URL is not set}
marker='<!-- conformance-read-diff -->'

changed=0
compared=0
sections=""
unchanged=""
for corpus in pdfium pdfjs; do
  json="$dir/read-diff-$corpus/read-diff.json"
  [ -f "$json" ] || continue
  compared=$((compared + 1))
  regressed=$(jq -r '.regressed' "$json")
  improved=$(jq -r '.improved' "$json")
  if [ $((regressed + improved)) -gt 0 ]; then
    changed=$((changed + regressed + improved))
    sections+=$(cat "$dir/read-diff-$corpus/read-diff.md")$'\n\n'
  else
    unchanged+="$(jq -r '"\(.compared) documents of the \(if .corpus == "pdfjs" then "pdf.js" else "PDFium" end) corpus read as on `\(.base_commit // "unknown" | .[0:7])`."' "$json")"$'\n'
  fi
done

if [ "$compared" = 0 ]; then
  echo "No read diffs to report; the base commit had no reads to compare with."
  exit 0
fi

existing=$(gh api --paginate "repos/$repo/issues/$pr/comments" \
  --jq ".[] | select(.user.login == \"github-actions[bot]\" and (.body | startswith(\"$marker\"))) | .id" \
  | head -n1)

if [ "$changed" = 0 ]; then
  if [ -z "$existing" ]; then
    echo "No document reads differently; no comment needed."
    exit 0
  fi
  body="$marker
## Conformance reads

No document reads differently from the base commit any more.

$unchanged
[Workflow run]($run_url)"
else
  body="$marker
## Conformance reads

Documents that Safe-PDF reads differently from the base commit, compared with PDFium. A read
regresses when Safe-PDF fails to open a document it opened before, crashes or times out, or
counts a different number of pages than PDFium.

$sections$unchanged
Details are in the job summaries of the [workflow run]($run_url). Rerun a case locally with
\`cargo conformance run --corpus <corpus> --read-only --case <id>\`."
fi

payload=$(jq -n --arg body "$body" '{body: $body}')
if [ -n "$existing" ]; then
  gh api --method PATCH "repos/$repo/issues/comments/$existing" --input - <<<"$payload" >/dev/null
  echo "Updated comment $existing"
else
  gh api --method POST "repos/$repo/issues/$pr/comments" --input - <<<"$payload" >/dev/null
  echo "Posted a comment on #$pr"
fi
