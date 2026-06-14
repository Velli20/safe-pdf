# safe-pdf

[![CI](https://github.com/Velli20/safe-pdf/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Velli20/safe-pdf/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Safe-PDF is a modular PDF reader and renderer written in Rust.

The project is still pre-alpha and under active development. APIs change quickly, rendering coverage is incomplete, and the current value of the repo is its architecture: strict safety constraints, clear layering, and extension points for new renderers or non-rendering PDF tooling.

## Overview

Safe-PDF is organized as a Cargo workspace under `crates/`, with examples and build tooling alongside it.

At a high level, the pipeline looks like this:

```text
PDF bytes
  -> pdf-tokenizer
  -> pdf-parser
  -> pdf-object
  -> pdf-document
  -> pdf-page
  -> pdf-content-stream / pdf-content-stream-operators
  -> pdf-canvas
  -> pdf-renderer
  -> pdf-graphics-skia / pdf-graphics-femtovg
```

## Architecture

### Core pipeline

- `pdf-tokenizer`: lexical analysis for PDF byte streams.
- `pdf-parser`: syntax-level parsing for objects, streams, xref data, headers, and related structures.
- `pdf-object`: in-memory PDF object model, including dictionaries, streams, trailers, versions, and object resolution support.
- `pdf-document`: document loading, decryption/encryption handling, object stream support, and high-level document access.
- `pdf-page`: page tree traversal, resource lookup, forms, patterns, shadings, external graphics state, and page-level caches.
- `pdf-content-stream`: content stream parsing and operator stream handling.
- `pdf-content-stream-operators`: trait-based operator categories used to dispatch path, text, color, graphics-state, clipping, shading, image, and marked-content operations.
- `pdf-canvas`: stateful PDF drawing engine that interprets page content against a generic backend.
- `pdf-renderer`: page rendering orchestration, plus recording-canvas based page caching and replay.

### Supporting crates

- `pdf-filter`: PDF stream filters such as ASCII85, ASCIIHex, LZW, predictors, and CCITT Fax support.
- `pdf-decode`: sample decoding helpers and indexed/ranged decode utilities.
- `pdf-image`: image XObject and inline-image handling.
- `pdf-color-space`: PDF color space parsing and conversion support, including Indexed, ICCBased, Separation, DeviceN, Lab, CalGray, and CalRGB.
- `pdf-function`: sampled, stitching, exponential interpolation, and PostScript-backed PDF functions.
- `pdf-shading`: shading model parsing and paint generation for gradients and mesh-based shadings.
- `pdf-font`: font decoding and mapping support for Type 0, Type 1, TrueType, Type 3, encodings, widths, CMaps, and ToUnicode handling.
- `pdf-postscript`: PostScript parser and calculator used by higher-level PDF functionality.
- `pdf-graphics`: shared geometry, color, path, transform, and rendering data structures.
- `pdf-object-collection`: utility collection support for PDF objects.

### Backend layer

- `pdf-graphics-skia`: Skia-backed `CanvasBackend` implementation.
- `pdf-graphics-femtovg`: FemtoVG-backed `CanvasBackend` implementation.

Two design points matter most if you are evaluating the repo:

- `CanvasBackend` in `pdf-canvas` keeps PDF interpretation separate from the concrete graphics engine. A backend supplies drawing primitives; the PDF pipeline stays backend-agnostic.
- `pdf-content-stream-operators` exposes trait-based operator handling, so the same parsed operator stream can drive a renderer, recorder, analyzer, or extraction tool without rewriting the parser.

## Workspace Layout

```text
safe-pdf/
├── crates/      # Core workspace crates
├── examples/    # Native and web-facing demos
├── fuzz/        # Fuzz targets for parser-oriented testing
├── xtask/       # Build automation, including emscripten packaging
├── Cargo.toml
└── README.md
```

The workspace members are `crates/*`, `examples`, and `xtask`.

## Rendering Model

The current rendering path is intentionally layered:

1. Parse a document into structured PDF objects and pages.
2. Resolve page resources such as fonts, forms, patterns, images, shadings, and color spaces.
3. Parse content streams into PDF operators.
4. Execute those operators against `PdfCanvas`.
5. Forward low-level draw calls into a chosen `CanvasBackend`.

`pdf-renderer` also supports recording a page into a `RecordingCanvas`, then replaying it later. That cache is resolution-independent and backend-agnostic, which is useful for page reuse, navigation, and future prefetch strategies.

## Examples And Features

The `examples` workspace member contains the currently supported demos.

- `cargo run --example skia --features skia -- examples/assets/webgl.pdf`
  Opens the native Skia viewer.
- `cargo run --example femtovg --features femtovg`
  Runs the FemtoVG prototype renderer.
- `cargo xtask emscripten --features skia-wasm`
  Builds the web target and copies artifacts into `examples/web/dist/`.
- `cargo xtask emscripten --features skia-wasm --serve --port 8080`
  Builds and serves the web example locally.

Feature flags in `examples/Cargo.toml`:

- `skia`: native Skia viewer with `winit` and `glutin`.
- `skia-wasm`: emscripten/web build without the native windowing stack.
- `femtovg`: FemtoVG renderer with `wgpu`.

Sample PDFs for experiments and debugging live in `examples/assets/`.

## Development

Common commands:

```sh
cargo test
cargo test -p pdf-parser
cargo test -p pdf-parser -- test_name
cargo check
cargo clippy --all --workspace
cargo fmt
cargo build --example skia --features skia
cargo run --example skia --features skia -- examples/assets/webgl.pdf
cargo xtask emscripten --features skia-wasm
cargo fuzz run parse_object
```

Important workspace constraints:

- `unsafe_code` is forbidden workspace-wide.
- `unwrap` and `expect` are denied in non-test code.
- indexing and slicing are denied by Clippy policy.
- errors are expected to propagate through `Result` rather than panic paths.

These constraints are not cosmetic. They shape the implementation style across the entire repo.

## License

Safe-PDF is licensed under the MIT License. See [LICENSE](LICENSE).

The repository also embeds fallback font assets under the SIL Open Font
License 1.1:

- Roboto and Roboto Mono. See `crates/pdf-font/assets/OFL.txt`.
- Noto Sans CJK Japanese Regular. See
  `crates/pdf-font/assets/NotoSansCJK-OFL.txt` and
  `crates/pdf-font/assets/NotoSansCJK-NOTICE.txt`.
