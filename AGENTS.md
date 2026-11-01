## Project Overview

Safe-PDF is a PDF reader and renderer. It's a modular monorepo organized as Cargo workspace crates under `crates/`.

## Common Commands

```sh
cargo test                                  # Run all tests
cargo test -p pdf-parser                    # Run tests for a single crate
cargo test -p pdf-parser -- test_name       # Run a single test
cargo check                                 # Type-check all crates
cargo clippy --workspace --all-targets      # Run lints (CI adds --all-features -D warnings)
cargo fmt                                   # Format code
cargo build --example skia --features skia  # Build Skia example
cargo run --example skia --features skia -- examples/assets/webgl.pdf  # Run viewer
cargo xtask emscripten --features skia-wasm # Build WASM target
cargo xtask web-canvas --profile debug      # Fast build for the Canvas 2D web example
cargo xtask web-canvas --profile debug --serve --port 8080 # Build and serve at http://127.0.0.1:8080
cargo fuzz run parse_object                 # Fuzz the parser
```

## Web Canvas Development

`pdf-graphics-web` runs through the `web-canvas-example` WASM application in
`examples/web-canvas`; it is not a native executable. The fast development loop is:

```sh
cargo xtask web-canvas --profile debug
cargo xtask web-canvas --profile debug --serve --port 8080
```

The build compiles for `wasm32-unknown-unknown`, writes browser bindings to
`examples/web-canvas/pkg/`, and copies the overlay fixture to
`examples/web-canvas/sample.pdf`. The server serves that directory at
`http://127.0.0.1:8080`; rerun the command after Rust changes to rebuild. It requires
the `wasm-bindgen-cli` version pinned by the workspace (`wasm-bindgen --version`).

## Workspace Lint Rules (Critical)

These are enforced workspace-wide via `Cargo.toml` and will fail CI:

- **`unwrap`/`expect` are denied** in non-test code. Use `Result<T, E>` with `?` propagation. (`clippy.toml` allows them in `#[cfg(test)]` only.)
- **`unsafe_code` is forbidden.** No exceptions without narrow justification.
- **`indexing_slicing` is denied.** Use `.get()` or iterators.
- **`panic` is denied.** Never panic in library code.
- **`as_conversions` is warned.** Prefer `From`/`Into`/`TryFrom`.
- **`arithmetic_side_effects` is warned.**

Use `thiserror::Error` for custom error types. Propagate errors with `?`.

## Architecture

The crates form a pipeline from bytes to pixels:

```
PDF bytes → pdf-tokenizer → pdf-parser → pdf-object → pdf-document
  → pdf-page → pdf-content-stream → pdf-canvas → pdf-graphics-{skia,femtovg} → display
```

Key architectural traits:
- **`CanvasBackend`** (in `pdf-canvas`): Abstracts rendering. Skia and FemtoVG are current implementations.
- **`PdfOperatorVariant`** (in `pdf-content-stream-operators`): A closed enum of parsed content stream operators. Consumers (the canvas, analyzers, exporters) match on it directly; `PdfCanvas::apply_operator` dispatches it for rendering.

Supporting crates:
- `pdf-font`: Font decoding (Type1/TrueType/Type3), isolated from rendering
- `pdf-graphics`: Common graphics types (color, transforms)
- `pdf-postscript`: Optional PostScript support
- `pdf-object-collection`: Utility collections for PDF objects

## Code Style

- Do not create subdirectories beneath any `src/` directory; keep Rust modules in flat sibling `.rs` files
- Idiomatic Rust: iterators, ownership, lifetimes over cloning
- Small, composable functions over monolithic ones
- Do not add trivial helper functions whose only purpose is to construct and return a single error variant; inline the error construction at the call site
- Avoid unnecessary heap allocations; prefer references and slices
- Document public functions, structs, and enums with `///` comments
- Unit tests go in `mod tests {}` within the same file
- State is threaded explicitly through contexts (no global state)

## CI

CI runs on push/PR to main (`.github/workflows/ci.yml`):
1. `cargo check` + `cargo test` + `cargo clippy` + `cargo fmt --check`
2. Minimal feature build (no optional features)
3. Web Canvas (wasm-bindgen) build of `examples/web-canvas`, deployed to GitHub Pages on push to `main`

Conformance runs in a separate repository, [pdf-conformance](https://github.com/Velli20/pdf-conformance),
which builds Safe-PDF's crates from a checkout and compares Safe-PDF with PDFium, showing pdf.js
and WebKit next to failing pages for context. `.github/workflows/conformance-dispatch.yml` sends
it each pull request and each push to `main`. On a pull request it reads both corpora (open each
document and compare page counts with PDFium, no rendering), compares the reads with the base
commit's, comments on the pull request when a document reads worse or better, and sets the
`conformance/reads` status. On `main` it also renders the PDFium corpus with the Skia renderer
(against PDFium's annotation-free render) and with the browser stack (`pdf-web` with its
annotation layer, against PDFium's render with annotations and form fields), keeps one issue
here per failure cause, closes issues whose failure is gone, and publishes the viewers to
<https://velli20.github.io/pdf-conformance/>. To fix a conformance issue, follow
`.claude/skills/fix-conformance-issue/SKILL.md`.
