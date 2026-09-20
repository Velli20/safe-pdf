//! PDF drawing interpretation, recording, and portable backend contracts.
use canvas_backend::CanvasBackend;
use pdf_canvas::PdfCanvas;
use pdf_content_stream_operators::pdf_operator_backend::PdfOperatorBackend;

/// Drawing operations implemented by concrete rendering backends.
pub mod canvas_backend;
/// PDF clipping-path operator handling.
mod canvas_clip_ops;
/// PDF color-space and color-setting operators.
mod canvas_color_ops;
/// Canvas external object ops.
mod canvas_external_object_ops;
/// PDF graphics-state selection and save/restore operators.
mod canvas_graphics_state_ops;
/// Marked-content operator handling.
mod canvas_marked_content_ops;
/// PDF path construction and painting operators.
mod canvas_path_ops;
/// PDF graphics-state values retained across saved states.
mod canvas_state;
/// PDF text operators and glyph painting.
mod canvas_text_ops;
/// Limits and context for nested content-stream rendering.
mod content_stream_render_state;
/// Errors reported across the canvas boundary.
pub mod error;
/// Validated mask descriptions and portable coverage preparation.
pub mod mask_layer;

/// Shared page and bitmap coordinate mappings.
pub mod viewport;
pub use viewport::{CanvasViewport, PageViewport, ViewportError};

/// Path geometry.
mod path_geometry;
pub use path_geometry::CanvasPath;
/// PDF operator interpretation and painting orchestration.
pub mod pdf_canvas;
/// Flat command recording and scoped replay.
pub mod recording_canvas;
/// Shading operators and shared paint preparation.
mod shading;
/// Backend-neutral stroke metadata.
pub mod stroke_style;
/// Text.
pub mod text;
/// Text state.
mod text_state;
/// Tiling shader.
pub mod tiling_shader;

impl<B: CanvasBackend> PdfOperatorBackend for PdfCanvas<'_, B> {}
