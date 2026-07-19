#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
RUNS=5
OUTPUT_DIR=
SKIP_HEAP=0
SKIP_XCTRACE=0

usage() {
  echo "usage: run_profile.sh [--runs N] [--output DIR] [--skip-heap] [--skip-xctrace] <pdf>" >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --runs)
      RUNS=${2:-}
      shift 2
      ;;
    --output)
      OUTPUT_DIR=${2:-}
      shift 2
      ;;
    --skip-heap)
      SKIP_HEAP=1
      shift
      ;;
    --skip-xctrace)
      SKIP_XCTRACE=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --*)
      echo "unknown option: $1" >&2
      usage
      exit 2
      ;;
    *)
      break
      ;;
  esac
done

if [[ $# -ne 1 || ! "$RUNS" =~ ^[1-9][0-9]*$ ]]; then
  usage
  exit 2
fi

PDF_PATH=$1
if [[ ! -r "$PDF_PATH" ]]; then
  echo "PDF is not readable: $PDF_PATH" >&2
  exit 2
fi
PDF_DIR=$(CDPATH= cd -- "$(dirname -- "$PDF_PATH")" && pwd -P)
PDF_PATH="$PDF_DIR/$(basename -- "$PDF_PATH")"

REPO_ROOT=$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)
if [[ -z "$OUTPUT_DIR" ]]; then
  OUTPUT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/safe-pdf-profile.XXXXXX")
else
  mkdir -p "$OUTPUT_DIR"
  if find "$OUTPUT_DIR" -mindepth 1 -maxdepth 1 -print -quit | grep -q .; then
    echo "refusing to overwrite non-empty output directory: $OUTPUT_DIR" >&2
    exit 2
  fi
fi
OUTPUT_DIR=$(CDPATH= cd -- "$OUTPUT_DIR" && pwd -P)
echo "Profile artifacts directory: $OUTPUT_DIR"
profile_failed() {
  echo "profiling failed; partial artifacts retained at: $OUTPUT_DIR" >&2
}
trap profile_failed ERR

for tool in cargo rustc ruby; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool is unavailable: $tool" >&2
    exit 1
  fi
done

HARNESS_DIR="$OUTPUT_DIR/harness"
mkdir -p "$HARNESS_DIR/src"
cp "$SCRIPT_DIR/harness/src/main.rs" "$HARNESS_DIR/src/main.rs"
cp "$SCRIPT_DIR/harness/src/heap_profile.rs" "$HARNESS_DIR/src/heap_profile.rs"

ESCAPED_REPO_ROOT=${REPO_ROOT//\\/\\\\}
ESCAPED_REPO_ROOT=${ESCAPED_REPO_ROOT//&/\\&}
ESCAPED_REPO_ROOT=${ESCAPED_REPO_ROOT//|/\\|}
sed "s|@REPO_ROOT@|$ESCAPED_REPO_ROOT|g" \
  "$SCRIPT_DIR/harness/Cargo.toml.template" > "$HARNESS_DIR/Cargo.toml"

echo "Building symbolized release harness in $HARNESS_DIR"
(
  cd "$REPO_ROOT"
  CARGO_PROFILE_RELEASE_DEBUG=1 \
  CARGO_PROFILE_RELEASE_STRIP=none \
    cargo build \
      --release \
      --bins \
      --manifest-path "$HARNESS_DIR/Cargo.toml"
)

PROFILE_BINARY="$HARNESS_DIR/target/release/safe-pdf-profile"
HEAP_BINARY="$HARNESS_DIR/target/release/heap-profile"

if [[ $(uname -s) == Darwin ]]; then
  TIME_COMMAND=(/usr/bin/time -lp)
else
  TIME_COMMAND=(/usr/bin/time -v)
fi

"${TIME_COMMAND[@]}" "$PROFILE_BINARY" end-to-end "$PDF_PATH" \
  > "$OUTPUT_DIR/first-observed.log" 2>&1
"$PROFILE_BINARY" io "$PDF_PATH" >/dev/null

for mode in io parse end-to-end; do
  LOG_PATH="$OUTPUT_DIR/$mode.log"
  for ((run = 1; run <= RUNS; run += 1)); do
    echo "measurement mode=$mode run=$run" >> "$LOG_PATH"
    "${TIME_COMMAND[@]}" "$PROFILE_BINARY" "$mode" "$PDF_PATH" \
      >> "$LOG_PATH" 2>&1
  done
done

ruby "$SCRIPT_DIR/summarize_runs.rb" \
  "$OUTPUT_DIR/io.log" \
  "$OUTPUT_DIR/parse.log" \
  "$OUTPUT_DIR/end-to-end.log" \
  > "$OUTPUT_DIR/timing-summary.txt"

if [[ $SKIP_HEAP -eq 0 ]]; then
  (
    cd "$OUTPUT_DIR"
    "${TIME_COMMAND[@]}" "$HEAP_BINARY" "$PDF_PATH"
  ) > "$OUTPUT_DIR/heap-run.log" 2>&1
  ruby "$SCRIPT_DIR/summarize_dhat.rb" \
    "$OUTPUT_DIR/dhat-heap.json" > "$OUTPUT_DIR/heap-summary.txt"
fi

{
  echo "cache_condition=repeated measurements are warm after one explicit io read; first-observed is uncontrolled"
  uname -a
  rustc -Vv
  cargo -V
  stat -f "pdf_size_bytes=%z" "$PDF_PATH" 2>/dev/null || stat -c "pdf_size_bytes=%s" "$PDF_PATH"
  xctrace version 2>/dev/null || true
} > "$OUTPUT_DIR/environment.txt"

if [[ $SKIP_XCTRACE -eq 0 && $(uname -s) == Darwin ]] && command -v xctrace >/dev/null 2>&1; then
  if ! bash "$SCRIPT_DIR/record_time_profile.sh" \
    "$PROFILE_BINARY" "$PDF_PATH" "$OUTPUT_DIR"; then
    echo "warning: xctrace failed; rerun record_time_profile.sh with the permissions described in SKILL.md" >&2
  fi
fi

cat "$OUTPUT_DIR/timing-summary.txt"
if [[ -f "$OUTPUT_DIR/heap-summary.txt" ]]; then
  sed -n '1,8p' "$OUTPUT_DIR/heap-summary.txt"
fi
echo "Profile artifacts: $OUTPUT_DIR"
echo "Trace-only retry: bash $SCRIPT_DIR/record_time_profile.sh $PROFILE_BINARY $PDF_PATH $OUTPUT_DIR"
trap - ERR
