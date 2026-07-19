#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: record_time_profile.sh <profile-binary> <pdf> <output-dir>" >&2
  exit 2
fi

PROFILE_BINARY=$1
PDF_PATH=$2
OUTPUT_DIR=$3
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)

if ! command -v xctrace >/dev/null 2>&1; then
  echo "xctrace is unavailable; install Xcode or run with --skip-xctrace" >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"
TRACE_PATH="$OUTPUT_DIR/time-profile.trace"
XML_PATH="$OUTPUT_DIR/time-profile-table.xml"
SUMMARY_PATH="$OUTPUT_DIR/cpu-summary.txt"

if [[ -e "$TRACE_PATH" || -e "$XML_PATH" || -e "$SUMMARY_PATH" ]]; then
  echo "refusing to overwrite existing Time Profiler artifacts in $OUTPUT_DIR" >&2
  exit 2
fi

xctrace record \
  --template "Time Profiler" \
  --output "$TRACE_PATH" \
  --target-stdout "$OUTPUT_DIR/time-profile-target.log" \
  --no-prompt \
  --launch -- "$PROFILE_BINARY" parse "$PDF_PATH"

xctrace export \
  --input "$TRACE_PATH" \
  --xpath '/trace-toc/run[@number="1"]/data/table[@schema="time-profile"]' \
  --output "$XML_PATH"

ruby "$SCRIPT_DIR/analyze_time_profile.rb" "$XML_PATH" > "$SUMMARY_PATH"
echo "CPU summary: $SUMMARY_PATH"
