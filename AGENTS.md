## Project Overview

Safe-PDF is a PDF reader and renderer. It's a modular monorepo organized as Cargo workspace crates under `crates/`.

## Common Commands

```sh
cargo test                                  # Run all tests
cargo test -p pdf-parser                    # Run tests for a single crate
cargo test -p pdf-parser -- test_name       # Run a single test
cargo check                                 # Type-check all crates
cargo clippy --all --workspace              # Run lints
cargo fmt                                   # Format code
cargo build --example skia --features skia  # Build Skia example
cargo run --example skia --features skia -- examples/assets/webgl.pdf  # Run viewer
cargo xtask emscripten --features skia-wasm # Build WASM target
cargo fuzz run parse_object                 # Fuzz the parser
```

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
- **`PdfOperatorBackend`**: Content stream operators dispatched via traits, enabling substitution with analyzers/exporters without modifying core logic.

Supporting crates:
- `pdf-font`: Font decoding (Type1/TrueType/Type3), isolated from rendering
- `pdf-graphics`: Common graphics types (color, transforms)
- `pdf-postscript`: Optional PostScript support
- `pdf-object-collection`: Utility collections for PDF objects

## Code Style

- Idiomatic Rust: iterators, ownership, lifetimes over cloning
- Small, composable functions over monolithic ones
- Avoid unnecessary heap allocations; prefer references and slices
- Document public functions, structs, and enums with `///` comments
- Unit tests go in `mod tests {}` within the same file
- State is threaded explicitly through contexts (no global state)

## CI

CI runs on push/PR to main (`.github/workflows/ci.yml`):
1. `cargo check` + `cargo test` + `cargo clippy` + `cargo fmt --check`
2. Minimal feature build (no optional features)
3. WASM/Emscripten build
