# pdf-conformance

Renders PDF test corpora with Safe-PDF, compares the pages with a PDFium
reference, and writes the evidence needed to start a fix: images, differing
regions, the drawing calls and text in each region, the PDF features used,
error chains with backtraces, and failure clusters.

## Quick start

```sh
# PDFium corpus: compared against the official golden images it ships, no setup needed.
cargo conformance fetch --corpus pdfium              # or: --root ~/pdfium_tests
cargo conformance run   --corpus pdfium              # add the same --root if you used one

# pdf.js corpus: has no golden images, so it needs a PDFium library.
cargo conformance setup-pdfium                       # once; builds PDFium from official sources
cargo conformance fetch --corpus pdfjs --links       # --links also downloads linked test files
cargo conformance run   --corpus pdfjs
```

Then open `target/conformance/<corpus>/index.html`, or start from
`target/conformance/<corpus>/TRIAGE.md`.

`fetch` clones a corpus at its pinned revision from its upstream repository.
An existing checkout passed with `--root` is used as it is, and `fetch` says
when its revision differs from the pinned one.

## References

| Corpus | Default reference | Needs |
|---|---|---|
| `pdfium` ([pdfium_tests](https://pdfium.googlesource.com/pdfium_tests)) | Golden PNGs shipped with the corpus (`--reference goldens`) | Nothing |
| `pdfjs` ([pdf.js](https://github.com/mozilla/pdf.js) `test/pdfs` + `test_manifest.json`) | PDFium library (`--reference pdfium`) | `setup-pdfium`, or `--pdfium <lib>` / `PDFIUM_LIBRARY` |

- **Goldens.** The goldens are written by PDFium's `pdfium_test` at 72 dpi. The
  harness prefers the Skia variants, which match Safe-PDF's rasterizer. Goldens
  include annotations and form fields, but Safe-PDF's compared render is page
  content only, so annotation rectangles are not compared. CI therefore runs
  both corpora with `--reference pdfium`, which renders every PDF without
  annotations or form fields; goldens stay the zero-setup local default.
- **`setup-pdfium`.** It clones depot_tools from chromium.googlesource.com and
  syncs PDFium from pdfium.googlesource.com, both at pinned revisions. It then
  builds a shared library into `target/conformance/pdfium-build/`. The first run
  downloads several GB (sources plus Google's pinned clang, gn and ninja) and
  takes a while. Reruns resume or finish quickly. On macOS it needs the Xcode
  command line tools.
- **No prebuilt downloads.** The harness never downloads prebuilt PDFium binaries.

## Commands

```sh
cargo conformance run --corpus pdfjs --filter issue1       # or --case <id> [--page N]
cargo conformance show --corpus pdfjs <case-id>            # print a case summary
cargo conformance accept --corpus pdfjs                    # record the last run as the baseline
```

## Output

`target/conformance/<corpus>/`:

- `index.html`: viewer with side-by-side, swipe, onion-skin and diff views,
  and filters by status, signature and baseline change (`j`/`k`, `1`–`4`)
- `TRIAGE.md`: failures clustered by signature, largest first
- `index.json`, `results.json`: machine-readable run data
- `cases/<id>/summary.md`: the entry point for fixing one case; alongside it
  are `report.json`, `pN-*.png` images, `pN-content.txt` and `objects.txt`

Workers run in separate processes. A crash, panic or timeout is therefore
recorded with its stage and stderr, and it does not stop the run.

## CI

`.github/workflows/conformance.yml` runs both corpora on Linux against the
PDFium library (`--reference pdfium`), so the pdfium corpus is compared without
annotations and includes PDFs that have no goldens.

- **Pull request merged to `main`, or a manual run:** it compares against the
  baselines, uploads the `conformance-<corpus>` artifacts (viewer, `TRIAGE.md`,
  bundles) and writes a job summary. It then runs `cargo conformance issues`.
  Regressions show up as a warning and in issues; they never fail the run.
- **Published viewer:** runs on `main` deploy the viewers to GitHub Pages at
  <https://velli20.github.io/safe-pdf/conformance/>, next to the web-canvas
  demo. Pages holds one site, so the `publish` job and `ci.yml`'s `deploy` job
  each fetch the other half from its newest `pages-conformance` /
  `pages-web-canvas` artifact (`.github/scripts/assemble-pages.sh`).
- **Gating pull requests:** uncomment the `pull_request` trigger in the
  workflow. Pull request runs then fail on regressions against the baselines,
  because `FAIL_ON_EVENTS` lists `pull_request`, and they file no issues.
- **Issue filing, on merged and manual runs:**
  - One issue is filed per failure cluster not reported yet, at most 15 new
    issues per corpus per run; the rest follow in later runs. Baseline
    regressions and new crashes come first.
  - Issues are labelled `conformance`, `conformance:<corpus>`, plus `crash`
    and/or `regression` where they apply.
  - Each issue carries a `Conformance key` derived from the corpus and the
    cluster signature. That key is how later runs find the issue again:
    - **Open:** later runs update it when the cluster grows or shrinks.
    - **Closed as completed:** it's reopened if the failure comes back.
    - **Closed as not planned:** it's never touched again.
  - Signatures come from the harness's classification. Changing how failures
    are classified can change keys, and so file new issues.
  - Preview locally with `cargo conformance issues --corpus pdfium --repo
    owner/name --dry-run`. Bodies are written to `target/conformance/<corpus>/issues/`.
- **PDFium on CI:** both legs download the pinned prebuilt
  `pdfium-linux-x64` from bblanchon/pdfium-binaries (`chromium/7881`, verified
  by SHA-256). Prebuilt binaries are used only in CI; local tooling never
  downloads them.
- **Bootstrapping baselines:** run the workflow manually with `accept`
  checked, download the `conformance-baseline-<corpus>` artifacts, and commit
  them as `tools/conformance/baselines/<corpus>.json`. Until a baseline exists,
  regressions are reported but don't fail the run.

## Baselines

`baselines/<corpus>.json` records the expected status and mismatch of every
page. `run` exits non-zero only on regressions against it, or on new crashes
and timeouts.
