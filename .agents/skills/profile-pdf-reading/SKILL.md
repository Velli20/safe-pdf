---
name: profile-pdf-reading
description: Profile Safe-PDF document reading for any local PDF file from file I/O through PdfReader using a release harness, repeated timing, macOS Instruments Time Profiler, and dhat heap analysis. Use when investigating slow or memory-heavy PDF loading, parser/xref/decryption/page-materialization bottlenecks, or performance regressions; exclude rendering unless the user explicitly expands scope.
---

# Profile PDF Reading

Profile the path from `std::fs::read` through `PdfReader::read_with_report` without contaminating results with compilation, viewer startup, or rendering. Produce measured findings and leave tracked project sources unchanged unless the user separately requests fixes.

## Establish the boundary

Default to these workloads:

- `io`: file loading only.
- `parse`: preload bytes, then measure `PdfReader`.
- `end-to-end`: file loading plus `PdfReader`.

Use the repository's release profile with debug symbols and stripping disabled; preserve its optimization level, LTO, and panic settings. Record the PDF size, platform, tool versions, cache condition, page count, and diagnostic count. Treat first-observed I/O as uncontrolled unless the cache was deliberately controlled; do not run privileged cache-purge commands.

Before profiling, inspect `PdfReader::read_with_report`, xref construction, object loading, stream decoding, and page-tree materialization. Preserve unrelated worktree changes and do not add the input PDF to Git.

## Run the bundled workflow

From the repository root, run:

```sh
bash .agents/skills/profile-pdf-reading/scripts/run_profile.sh \
  --runs 5 \
  --output /private/tmp/safe-pdf-profile \
  path/to/document.pdf
```

The script creates an out-of-tree Cargo harness, builds it in release mode with symbols, captures repeated `/usr/bin/time` measurements, runs a parse-only `dhat-rs` heap profile, and attempts a macOS Instruments Time Profiler recording. It never edits workspace crates.

If the output directory is omitted, the script creates a unique temporary directory. It refuses to overwrite a non-empty directory. Use `--skip-heap` or `--skip-xctrace` only when the corresponding tool is unavailable.

The first build may need network approval to download `dhat`. On macOS, `xctrace` may need approval to write its Xcode cache outside the workspace. If the bundled run cannot invoke `xctrace`, rerun only the trace step with the command printed by the script:

```sh
bash .agents/skills/profile-pdf-reading/scripts/record_time_profile.sh \
  OUTPUT/harness/target/release/safe-pdf-profile \
  PDF \
  OUTPUT
```

Do not use the Instruments Allocations template as the primary heap profiler: it may require additional Developer Tools permissions and can fail to attach. The bundled `dhat-rs` binary profiles Rust allocations after the input bytes are loaded.

## Analyze the evidence

Read these generated artifacts:

- `timing-summary.txt`: median/range for the selected internal timer plus process-wide peak RSS, retired instructions, and cycles. For `parse`, the internal timer excludes byte preloading, but process-wide counters include startup and the retained input buffer; use `dhat` for parse-only heap attribution.
- `cpu-summary.txt`: sampled leaf and inclusive functions from Instruments.
- `heap-summary.txt`: total allocation traffic, peak live heap, and largest allocation stacks.
- `first-observed.log` and per-mode logs: raw validation and resource counters.
- `time-profile.trace`, `time-profile-table.xml`, and `dhat-heap.json`: primary evidence.

Correlate the hottest stacks with source using `rg` and direct file inspection. For large PDFs, explicitly check:

- Full-input scans such as `windows(...)`, recovery searches, or repeated marker collection.
- Byte-at-a-time tokenizer/parser loops and geometrically growing vectors.
- Deep clones of large strings, streams, dictionaries, or annotation values.
- Eager parsing/decompression of objects not needed to construct the requested document state.
- Retry passes, decryption, stream filters, content parsing, and page/resource inheritance.

Separate confirmed profile findings from static hypotheses. Do not call a path a bottleneck merely because it looks inefficient.

Without a successful Instruments trace, report only that parsing dominates CPU based on timing/instructions; do not infer function-level CPU percentages from the heap profile.

## Report results

Lead with the dominant CPU and memory costs. Include:

1. A table for `io`, `parse`, and `end-to-end` median, range, and peak RSS.
2. CPU sample counts and percentages for the top reader stacks.
3. Total allocated bytes, peak live heap, and the largest allocation sites.
4. Ranked optimizations with source links, confidence, tradeoffs, and an evidence-based upper bound rather than a promised speedup.
5. Cache limitations, profiler failures/fallbacks, artifact paths, and confirmation that tracked sources were unchanged.

Recommend `brew install hyperfine` only when richer timing statistics are needed, and `cargo install samply` only when a shareable browser flamegraph adds value. Prefer native Instruments over `cargo-flamegraph` on macOS because DTrace commonly requires elevated permissions.
