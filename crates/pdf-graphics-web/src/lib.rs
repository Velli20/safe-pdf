#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! Canvas 2D drawing and bounded browser rendering resources.

/// Shared pixel-storage accounting and reservation ownership.
mod budget;
/// PDF graphics-state values retained across saved states.
mod canvas_state;
/// Errors reported across the canvas boundary.
pub mod error;
/// Browser-backed temporary rendering surface.
mod surface;
/// Surface pool.
mod surface_pool;
/// Web canvas backend.
pub mod web_canvas_backend;
/// Image placement and upload.
mod web_image;
/// Isolated soft-mask rendering.
mod web_mask;
/// Web paint.
mod web_paint;
/// Web path.
mod web_path;
/// Shader and tiling-pattern rasters.
mod web_shader;

pub use error::WebCanvasBackendError;
pub use web_canvas_backend::{WebCanvasBackend, WebCanvasOptions};

/// Rasterization of device-space recordings into owned browser image resources.
pub mod rasterizer;
pub use budget::BudgetedImage;
pub use rasterizer::WebRasterizer;
