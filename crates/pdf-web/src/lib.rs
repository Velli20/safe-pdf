#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! Browser page orchestration, annotation presentation, and interaction adapters.

/// Entries extended with ready-to-assign CSS.
pub mod annotation_entry;
/// Browser adapter around the renderer's annotation overlay.
pub mod annotation_overlay;
/// Browser integration errors.
pub mod error;
/// Concrete WASM exports for annotations and text selection.
pub mod interop;
/// Content rendering and separate annotation preparation.
pub mod page_renderer;
/// CSS layout around shared page and canvas viewports.
pub mod viewport;

pub use annotation_entry::{WebAnnotationEntry, WebCss};
pub use annotation_overlay::{PreparedAnnotationOverlay, WebAnnotationOverlay};
pub use error::WebError;
pub use page_renderer::{WebPageOutput, WebPageRenderer};
pub use viewport::WebViewport;
