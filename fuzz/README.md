# Fuzzing Safe-PDF (pdf-parser)

This is a cargo-fuzz workspace for fuzzing the `pdf-parser` crate.

Quick start:

1. Install cargo-fuzz:
   - `cargo install cargo-fuzz`
2. From the `fuzz/` folder, run:
   - `cargo fuzz run parse_object`

Crash artifacts will be stored under `fuzz/artifacts/parse_object`.
