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
cargo conformance run --corpus pdfjs --read-only           # only open documents and count pages
cargo conformance read-diff --corpus pdfjs --base <reads.json>  # compare reads with an earlier run
cargo conformance accept --corpus pdfjs                    # record the last run as the baseline
cargo conformance repro <issue key or case id>             # rerun an issue's cases, no setup needed
cargo conformance verify <issue key>                       # exits 0 once no case fails that way
cargo conformance render <pdf> [--page N]                  # one local PDF, no corpus needed
```

`render` runs one page of any PDF through the same worker as `run`. It writes the Safe-PDF
image, the page's content-stream graph and `summary.md` to
`target/conformance/render/<file>/`, prints how long each Safe-PDF step took, and exits
non-zero when the page fails. Pass `--reference <png>` to compare against an image such as a
PDFium render. Otherwise it compares against `--pdfium <lib>`, `PDFIUM_LIBRARY` or the
`setup-pdfium` build when one exists.

## Fixing an issue

Every conformance issue carries a key such as `conf2-98ff16df22ee`.

```sh
cargo conformance repro conf2-98ff16df22ee
```

`repro` reads the published report on GitHub Pages, checks out only the PDFs of that
issue at the corpus revision the report used, downloads the PDFium images of their failing
pages, and reruns those cases against the images. It needs neither a corpus checkout nor a
PDFium build, so it also works in cloud sessions. Results land in
`target/conformance/<corpus>/` as for `run`. The repository is read from `origin`; pass
`--repo owner/name` otherwise.

The pdf.js corpus is only read on `main`, so it has no published report. For such an issue,
`repro` takes the cases from the issue's machine-readable summary (through `gh`) and reruns
them without reference images. That still shows whether a crash, timeout or error is gone, and
`render` gives the image to compare by eye.

`verify` does the same as `repro` and is the done check for a fix. It exits non-zero while any
case still fails with the issue's signature, and also when a case now fails with a different
signature, such as a timeout that became a wrong render. Pass `--allow-different` to accept
that.

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
- `pN-streams.txt` lists the content streams the page can draw through its resources: Form
  XObjects, Type 3 fonts with their glyph procedures, tiling patterns and soft masks, each
  under the stream that draws with it. `↻ cycle` marks a stream that leads back to one
  already being drawn. It is read from the resources without rendering, so it is there for
  crashes and timeouts too.
- The summary's "Safe-PDF render" line gives the time of each step and how much replay work
  the recording holds. Safe-PDF first records the page (`safe-record`), then replays the
  recording onto the raster (`safe-replay`). A step reported as "did not finish" is where a
  timeout or crash happened. The replay cost counts every replay of nested pattern cells and
  masks, so a large cost with deep nesting explains a slow replay.

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
- **Crashes and timeouts**: the panic location, the time limit or the memory limit, with the
  worker stage.
  Safe-PDF's stages are `safe-read`, `safe-streams` (the content-stream graph),
  `safe-record`, `safe-replay` and `safe-trace`. A stage of the harness itself (`inventory`,
  `compare`, `regions`, `write`) is labelled `harness` rather than `crash`.
- **`font_substitution`**: every difference on the page is text, in a document that uses
  non-embedded fonts. Renderers substitute such fonts differently, so these pages are
  expected to differ. They are counted in TRIAGE.md and the viewer but are not failures and
  are not filed.

## Output

`target/conformance/<corpus>/`:

- `index.html`: viewer with side-by-side, swipe, onion-skin and diff views,
  and filters by status, signature and baseline change (`j`/`k`, `1`–`4`).
  `index.html#case=<id>&page=<n>` opens one page; the address follows the selection.
- `TRIAGE.md`: failures clustered by signature, largest first, after any pages that took
  at least 5 s to render (not failures, but likely performance regressions)
- `index.json`, `results.json`: machine-readable run data
- `cases/<id>/summary.md`: the entry point for fixing one case; alongside it
  are `report.json`, `pN-*.png` images, `pN-content.txt`, `pN-streams.txt` and
  `objects.txt`

Workers run in separate processes. A crash, panic or timeout is therefore
recorded with its stage and stderr, and it does not stop the run. A worker is also killed
once its resident memory passes `--memory-limit` (3072 MiB by default, 0 for none; checked
through `/proc`, so Linux only) and recorded as a crash with `memory limit exceeded`, so a
runaway read cannot exhaust the machine and take the run down with it.

## CI

`.github/workflows/conformance.yml` checks both corpora on Linux against the PDFium library
(`--reference pdfium`), so the pdfium corpus is compared without annotations and includes
PDFs that have no goldens.

- **Reads (pull requests, `main` and manual runs):** the `reads` job runs both corpora with
  `--read-only`: every document is opened with Safe-PDF and PDFium and their page counts are
  compared, but no page is rendered. Each run writes `reads.json` and uploads the report as the
  `reads-<corpus>` artifact. Runs on `main` cache `reads.json` under the commit.
- **Pull requests:** the reads are compared with the base commit's, from that cache or, when
  the base commit has none yet, by reading the corpus at the base commit in the same job.
  `cargo conformance read-diff --corpus <corpus> --base <reads.json>` writes `read-diff.md`
  and `read-diff.json`. A document regresses when Safe-PDF no longer opens it, crashes or
  times out on it, or newly counts a different number of pages than PDFium; the reverse is an
  improvement. When anything regressed or improved, the `read-comment` job posts one comment
  on the pull request and updates it on later pushes (also once nothing differs any more).
  Pull requests from forks get no comment. Pull requests are never failed by these jobs.
- **Rendering (`main` and manual runs):** the PDFium corpus is rendered and every page is
  compared against its baseline, uploaded as the `conformance-pdfium` artifact (viewer,
  `TRIAGE.md`, `results.json`, bundles) with a job summary. The pdf.js corpus is not rendered.
  The `issues` job then reports the PDFium render and the pdf.js reads together with
  `cargo conformance issues --corpus pdfium --corpus pdfjs`.
- **Gating later:** add `pull_request` to `FAIL_ON_EVENTS` and the `pull_request` trigger to
  the rendering job to fail pull requests on render regressions against the baselines.
- **Published viewer:** runs on `main` deploy the viewers to GitHub Pages at
  <https://velli20.github.io/safe-pdf/conformance/>, next to the web-canvas demo. Issues
  link straight into it (`#case=<id>&page=<n>`). Pages holds one site,
  so the `publish` job and `ci.yml`'s `deploy` job each fetch the other half from its newest
  `pages-conformance` / `pages-web-canvas` artifact (`.github/scripts/assemble-pages.sh`).
- **Issues:**
  - One issue per cause across both corpora, labelled `conformance`, `conformance:<corpus>`,
    `area:<crate>` when the error's crate is known, and `crash`, `harness` or `regression`
    where they apply. The first run, when the repository has no conformance issue yet, files
    every cluster; later runs file at most 15 new issues each (`--max-new`, 0 for no limit)
    and the rest follow in later runs, worst status first. Issues are written about one a
    second to stay under GitHub's secondary rate limit.
  - Each body opens with an alert and a table of facts, links the suspect source line (GitHub
    shows it as a snippet), shows one case's images, lists the cases with viewer,
    summary and PDF links, gives the `repro` and `verify` commands, and ends with a JSON
    block for agents.
  - A hidden `conformance-state` comment holds the issue's key and, per corpus, its cluster
    size and how many full runs in a row it was absent:
    - **Open:** the body is refreshed silently. A comment is posted only when new documents
      join the cluster or it shrinks by a quarter or more.
    - **Absent from two full runs in a row:** closed as completed.
    - **Closed as completed:** reopened if the failure comes back.
    - **Closed as not planned:** never touched again.
    - **Older body layout:** an open issue written with an older layout (the state's
      `format` below the current one) is rewritten on the next run even when its cluster did
      not change.
  - Issues without that comment (including those filed before it existed) are ignored.
  - **Images:** GitHub's API cannot attach files to issues and the Pages site is replaced on
    every deploy, so before writing any issue the job commits the images the bodies embed to
    the `conformance-assets` branch and the bodies link them through
    `raw.githubusercontent.com`. Files are named by a hash of their content, so a link keeps
    showing the image it was written with. Each run replaces the branch with one
    parentless commit holding only the images open issues link to, so images of closed
    issues drop out and the branch stays small. Contributors who do not want it in their
    clone can use `git clone --single-branch`. A dry run copies the images to
    `target/conformance/issues/assets/` instead.
  - Runs limited with `--filter`, `--case` or `--page` never close issues. Read-only runs
    (the pdf.js corpus) file and refresh issues but never count a failure as absent, so they
    do not close issues either.
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
