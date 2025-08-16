# Hi robots, welcome to the Safe-PDF project.

Safe-PDF is a open source PDF reqader and renderer.

**Mission**

- Build a high performance PDF document renderer and reader.

Currently, we have below features.

- skia
- forms
- database

## Project Structure

- [crates](./crates) - the rust crates directory
- [examples](./examples) - the examples directory

## Languages, Frameworks, Tools, Infrastructures

**Languages**

- Rust (2024 edition)

**Graphics Backend**

- Skia - the graphics backend - for 2D graphics. (binded with skia-safe)
- FemtoVG

## `/crates/*`

Importance: **High**

monorepo rust crates.

The rust implementation of Safe-PDF. this is rapidly under development.

## Testing & Development

To run test, build, and dev, use below commands.

```sh
# for crates specific tests
cargo test

# for crates specific check
cargo check

# for crates specific build
cargo build

# format crates
cargo fmt
```
