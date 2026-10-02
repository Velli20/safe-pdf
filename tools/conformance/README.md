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
cargo conformance run --corpus pdfjs --filter issue1       # or --case <id> [--page N]; --case repeats
cargo conformance show --corpus pdfjs <case-id>            # print a case summary
cargo conformance accept --corpus pdfjs                    # record the last run as the baseline
cargo conformance repro <issue key or case id>             # rerun an issue's cases, no setup needed
cargo conformance verify <issue key>                       # exits 0 once no case fails that way
```

## Fixing an issue

Every conformance issue carries a key such as `conf2-98ff16df22ee`.

```sh
cargo conformance repro conf2-98ff16df22ee
```

`repro` reads the published report on GitHub Pages, checks out only the PDFs of that
issue at the corpus revision the report used, downloads the PDFium images of their failing
pages, and reruns those cases against the images. It needs neither a corpus checkout nor a
PDFium build, so it also works in cloud sessions. Results land in
`target/conformance/<corpus>/` as for `run`. `verify` does the same and exits non-zero while
any case still fails with the issue's signature, which makes it the done check for a fix.
The repository is read from `origin`; pass `--repo owner/name` otherwise.

The skill in `.claude/skills/fix-conformance-issue/SKILL.md` describes the whole loop for
agents.

## Reading the images

- `pN-compare.png`: reference | Safe-PDF | diff. Diff colors: red = ink missing in Safe-PDF,
  blue = extra ink in Safe-PDF, magenta = hue differs, orange = same hue but different
  intensity (shape), yellow = anti-aliasing or 1 px displacement (tolerated), pale blue
  hatching = annotation area (not compared), gray = matching.
- Region classes: `missing_ink` / `extra_ink` = one renderer drew nothing there;
  `color_shift` = same shapes, different color; `reshaped` = both drew but glyph shapes or
  geometry differ; `offset` = same content shifted by a few pixels.
- `pN-regionK.png`: reference | Safe-PDF crops of region K, enlarged.
- Region pixels are `[x0, y0, x1, y1]` from the top-left. Page space is PDF user space
  `[left, bottom, right, top]`.
- Draw `#n` is the n-th Safe-PDF backend call on the page. Bounds are device pixels after
  clipping. Colors print as `#RRGGBB`.
- `pN-content.txt` lists the page's content stream operators. `report.json` has full
  backtraces and worker stderr.

## Statuses and signatures

Each failing page (or document) gets a signature, and failures sharing a signature form a
cluster, filed as one issue.

- **Errors** (`read_error`, `render_error`): the innermost error message, with numbers and
  quoted resource names such as `'F0'` normalized, plus the crate and enum variant whose
  `#[error]` template produced it, for example
  `render_error: … @ pdf-canvas::PathRequired`.
- **Mismatches**: the class of the most telling differing region and the paint of the
  Safe-PDF draw call there (shading type, tiling pattern, image format, blend mode, soft
  mask), for example `mismatch: extra_ink / fill (tiling pattern)`.
- **Crashes and timeouts**: the panic location or the time limit, with the worker stage.
  A stage of the harness itself (`inventory`, `compare`, `regions`, `write`) is labelled
  `harness` rather than `crash`.
- **`font_substitution`**: every difference on the page is text, in a document that uses
  non-embedded fonts. Renderers substitute such fonts differently, so these pages are
  expected to differ. They are counted in TRIAGE.md and the viewer but are not failures and
  are not filed.

## Output

`target/conformance/<corpus>/`:

- `index.html`: viewer with side-by-side, swipe, onion-skin and diff views,
  and filters by status, signature and baseline change (`j`/`k`, `1`–`4`).
  `index.html#case=<id>&page=<n>` opens one page; the address follows the selection.
- `TRIAGE.md`: failures clustered by signature, largest first
- `index.json`, `results.json`: machine-readable run data
- `cases/<id>/summary.md`: the entry point for fixing one case; alongside it
  are `report.json`, `pN-*.png` images, `pN-content.txt` and `objects.txt`

Workers run in separate processes. A crash, panic or timeout is therefore
recorded with its stage and stderr, and it does not stop the run.

## CI

`.github/workflows/conformance.yml` runs both corpora on Linux against the PDFium library
(`--reference pdfium`), so the pdfium corpus is compared without annotations and includes
PDFs that have no goldens.

- **Pull request merged to `main`, or a manual run on `main`:** each corpus is compared
  against its baseline and uploaded as the `conformance-<corpus>` artifact (viewer,
  `TRIAGE.md`, `results.json`, bundles) with a job summary. The `issues` job then reports
  both corpora together with `cargo conformance issues --corpus pdfium --corpus pdfjs`.
- **Pull requests:** not run, so they merge without waiting on the corpora. A regression
  shows up after the merge as a new or reopened issue labelled `regression`. To check a
  change before merging, run `cargo conformance repro <key>` for the issues it touches, or
  start the workflow manually on the branch.
- **Gating later:** add a `pull_request` trigger and add `pull_request` to
  `FAIL_ON_EVENTS` in the workflow to fail pull requests on regressions against the
  baselines.
- **Published viewer:** runs on `main` deploy the viewers to GitHub Pages at
  <https://velli20.github.io/safe-pdf/conformance/>, next to the web-canvas demo. Issues
  link straight into it (`#case=<id>&page=<n>`) and embed its images. Pages holds one site,
  so the `publish` job and `ci.yml`'s `deploy` job each fetch the other half from its newest
  `pages-conformance` / `pages-web-canvas` artifact (`.github/scripts/assemble-pages.sh`).
  A deploy without the web-canvas demo fails rather than replacing it. Pages sites are
  limited to 1 GB: images of opaque pages are written as RGB PNGs to stay well under it,
  and a site over 900 MB is deployed without the conformance reports.
- **Issues:**
  - One issue per cause across both corpora, labelled `conformance`, `conformance:<corpus>`,
    `area:<crate>` when the error's crate is known, and `crash`, `harness` or `regression`
    where they apply. At most 15 new issues per run; the rest follow in later runs, worst
    status first.
  - Each body opens with an alert and a table of facts, links the suspect source line (GitHub
    shows it as a snippet), shows one case's images from Pages, lists the cases with viewer,
    summary and PDF links, gives the `repro` and `verify` commands, and ends with a JSON
    block for agents.
  - A hidden `conformance-state` comment holds the issue's key and, per corpus, its cluster
    size and how many full runs in a row it was absent:
    - **Open:** the body is refreshed silently. A comment is posted only when new documents
      join the cluster or it shrinks by a quarter or more.
    - **Absent from two full runs in a row:** closed as completed.
    - **Closed as completed:** reopened if the failure comes back.
    - **Closed as not planned:** never touched again.
  - Issues without that comment (including those filed before it existed) are ignored.
  - Runs limited with `--filter`, `--case` or `--page` never close issues.
  - Preview locally with `cargo conformance issues --corpus pdfium --corpus pdfjs --repo
    owner/name --dry-run`. Bodies are written to `target/conformance/issues/`.
- **PDFium on CI:** the conformance legs download the pinned prebuilt
  `pdfium-linux-x64` from bblanchon/pdfium-binaries (`chromium/7881`, verified
  by SHA-256). Prebuilt binaries are used only in CI; local tooling never
  downloads them.
- **Bootstrapping baselines:** run the workflow manually with `accept` checked, download
  the `conformance-baseline-<corpus>` artifacts, and commit them as
  `tools/conformance/baselines/<corpus>.json`. Until a baseline exists, regressions are
  reported but nothing is labelled `regression`. Manual runs on other branches file no
  issues, so baselines can be recorded from a branch.

## Baselines

`baselines/<corpus>.json` records the expected status and mismatch of every
page. `run` exits non-zero only on regressions against it, or on new crashes
and timeouts.
