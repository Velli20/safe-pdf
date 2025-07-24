# pdf-canvas

**`pdf-canvas` is a Rust crate for constructing and rendering 2D vector graphics, primarily designed for PDF document generation.**

It offers a stateful API for defining paths (lines, curves, shapes) and then painting them (stroking, filling) onto a canvas. The crate abstracts the low-level drawing commands by delegating the actual rendering to a `CanvasBackend`. This design makes `pdf-canvas` flexible and allows it to be used with different PDF generation backends or potentially other rendering targets.

## Overview

The central component of `pdf-canvas` is the `PdfCanvas` struct. It acts as the main interface for all drawing operations and maintains the state of the current path being constructed. Path construction methods (e.g., `move_to`, `line_to`, `rectangle`) build up this internal path. When a painting operation (e.g., `stroke_path`, `fill_path_nonzero_winding`) is called, `PdfCanvas` instructs the provided `CanvasBackend` to render the accumulated path, and then typically clears the current path for new operations.

## Core Concepts

### `PdfCanvas<'a>`
The primary struct for drawing. It holds an optional `PdfPath` (the path currently being built) and a mutable reference to a `CanvasBackend` implementation. It implements `PathConstructionOps` and `PathPaintingOps`.

### `PdfPath`
Represents a sequence of path construction commands, stored as `PathVerb` enums (e.g., `MoveTo`, `LineTo`, `CubicTo`, `Close`). It effectively stores the geometry of a shape or line.

### `PathVerb`
An enum defining the individual operations within a `PdfPath`, like moving to a point, drawing a line to a point, drawing a curve, or closing the path.

### `CanvasBackend` Trait
A crucial trait that must be implemented by any backend intended for use with `PdfCanvas`. It defines how paths are actually drawn via the `draw_path` method. It also has extensive supertrait bounds from `pdf_content_stream::pdf_operator_backend` (like `ClippingPathOps`, `ColorOps`, `GraphicsStateOps`, etc.), indicating that a compliant backend is a full-featured PDF operator handler.

### `PaintMode` and `PathFillType`
Enums that control how a path is rendered:
*   `PaintMode`: Specifies whether to `Fill`, `Stroke`, or `FillAndStroke` the path.
*   `PathFillType`: Determines the rule for filling complex paths: `Winding` (non-zero) or `EvenOdd`.

### `PdfCanvasError`
The error type used by this crate, indicating issues like `NoActivePath` for a painting operation or `NoCurrentPoint` for a relative drawing command.

## Modules

*   `error.rs`: Defines `PdfCanvasError` for error handling within the canvas operations.
*   `pdf_path.rs`: Contains `PdfPath` for storing sequences of path commands and `PathVerb` for representing individual commands.
*   `lib.rs`: The main library file, defining `PdfCanvas`, the `CanvasBackend` trait, `PaintMode`, `PathFillType`, and implementations of `PathConstructionOps` and `PathPaintingOps` for `PdfCanvas`.

## Contributing

Contributions, issues, and feature requests are welcome.

## License

MIT License
