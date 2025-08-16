# Hi robots, welcome to the Safe-PDF project.

Safe-PDF is an open-source, high-performance PDF reader and renderer built in Rust.

**Mission**

- Build a fast, reliable, and safe PDF renderer and reader.
- Provide a modern Rust implementation with well-structured crates and clean APIs.

## Project Structure

- [crates](./crates) - Core Rust crates (monorepo style, main implementation of Safe-PDF).
- [examples](./examples) - Example applications and demos.

## Languages, Frameworks, Tools, Infrastructures

**Languages**

- Rust (2024 edition)

**Graphics Backend**

- Skia (via skia-safe crate) — Primary 2D rendering backend.
- FemtoVG — Lightweight vector graphics alternative (WIP).

## `/crates/*`

Importance: **High**

monorepo rust crates.

The rust implementation of Safe-PDF. this is rapidly under development.

## Testing & Development

Run the following commands from the project root:

```sh
# Run tests for all crates
cargo test

# Type-check all crates
cargo check

# Run Clippy lints
cargo clippy --all --workspace

# Build example app with Skia backend
cargo build --example skia --features "skia"

# Run example app with Skia backend
cargo run --example skia --features "skia"

# Format all code (pre-commit recommended)
cargo fmt
```

## Contribution Notes (for humans & copilots)

- Keep PRs small and focused.
- Add tests for new features and bug fixes.
- Maintain clear commit messages.
- Ensure cargo fmt and cargo check pass before pushing.

## Copilot Hints

When suggesting or generating code for this project, please follow these rules:

### Error Handling

- Prefer Result<T, Error> or Option<T> over unwrap / expect.
- Use thiserror::Error for custom error types.
- Always propagate errors with ?.

### Code Style

- Use idiomatic Rust patterns (iterators, ownership, lifetimes).
- Document all public functions, structs, and enums with /// comments.
- Write small, composable functions instead of monolithic ones.

### Examples & Testing

- Place runnable demos in /examples.
- Prefer unit tests inside the same crate (tests module).
- Use integration tests in a tests/ folder only for cross-crate functionality.

### Performance & Safety

- Avoid unnecessary heap allocations.
- Use references and slices instead of cloning where possible.
- Never use unsafe unless absolutely required — and document why.