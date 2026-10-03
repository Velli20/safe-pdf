---
name: fix-conformance-issue
description: Fix a GitHub issue filed by the conformance workflow (label `conformance`, key `conf2-…`) — reproduce it without PDFium, find the cause, fix it with a unit test, and verify.
---

# Fix a conformance issue

Conformance issues are filed by `.github/workflows/conformance.yml`. Each one carries a key
(`conf2-…`), a signature, the suspect source location, the failing cases, and a JSON block
under "For agents" with the same data. Read that block first.

## 1. Reproduce

```sh
cargo conformance repro <key>
```

This checks out only the issue's PDFs and compares Safe-PDF against the PDFium images
published on GitHub Pages; it needs no PDFium build. Output lands in
`target/conformance/<corpus>/`: start from `TRIAGE.md`, then
`cases/<case dir>/summary.md` for each case. If `repro` cannot reach GitHub Pages or the
upstream corpus, say so and work from the images and details in the issue.

A corpus that is only read on `main` (pdf.js) has no published images. `repro` then reruns
the cases listed in the issue without a reference, which still shows crashes, timeouts and
errors. To look at one PDF, including one the user puts in the repository, run
`cargo conformance render <pdf> --page <n>` (add `--reference <png>` when you have a PDFium
image). It writes the Safe-PDF image and the evidence below to `target/conformance/render/`.
Look at the image before calling a fix done: a page that no longer times out can still render
wrongly.

## 2. Locate the cause

- **`read_error` / `render_error`**: the signature ends in `@ <crate>::<Variant>`, the enum
  variant whose `#[error]` template produced the message. `summary.md` names its file and
  line under "Defined at" and lists the workspace frames. Find where that variant is
  constructed on the failing path, not only where it is defined.
- **`mismatch`**: open `pN-compare.png` and the region crops. The region table says what
  differs (`missing_ink`, `extra_ink`, `color_shift`, `reshaped`, `offset`) and which
  Safe-PDF draw calls touch it; `pN-content.txt` lists the page's operators.
- **`crash` / `timeout`**: the panic location or the stage is in the signature. Stages
  `inventory`, `compare`, `regions` and `write` belong to the harness in
  `tools/conformance/`, not to Safe-PDF (label `harness`). `safe-record` interprets the
  page into a recording and `safe-replay` draws that recording, so they point at different
  code. The summary's "Safe-PDF render" line gives each step's time and the recording's replay
  cost and nesting. `pN-streams.txt` lists the Form XObjects, Type 3 glyph procedures, tiling
  patterns and soft masks the page can draw through, with cycles marked `↻ cycle`. Start a
  timeout there.

Compare with how PDFium or pdf.js handle the same input when the PDF is malformed:
Safe-PDF aims to render what they render.

## 3. Fix with a test

- Follow `AGENTS.md`: no `unwrap`/`expect`/`panic`/indexing outside tests, errors
  propagate with `?`, modules stay flat.
- Add a unit test in the fixed crate (`mod tests` in the same file) that builds the smallest
  input showing the bug. Do not copy corpus PDFs into the repository.
- Run `cargo test -p <crate>`, `cargo clippy --all --workspace -- -D warnings` and
  `cargo fmt`.

## 4. Verify

```sh
cargo conformance verify <key>
```

It exits 0 when no case of the issue fails with its signature any more and none fails with a
different one. A case that now fails differently is marked `≠` and fails the check. If that is
expected, rerun with `--allow-different` and say which case in the pull request. Reference the issue with
`Fixes #<number>` so it closes on merge; otherwise the workflow closes it after two runs
on `main` without the failure.
