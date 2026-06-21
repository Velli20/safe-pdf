use canvas_backend::CanvasBackend;
use pdf_canvas::PdfCanvas;
use pdf_content_stream_operators::pdf_operator_backend::PdfOperatorBackend;

pub mod canvas_backend;
mod canvas_clip_ops;
mod canvas_color_ops;
mod canvas_external_object_ops;
mod canvas_graphics_state_ops;
mod canvas_marked_content_ops;
mod canvas_path_ops;
mod canvas_state;
mod canvas_text_ops;
pub mod error;
mod font_renderer_support;
mod pdf_path_pen;

pub mod pdf_canvas;
pub mod recording_canvas;
mod shading;
pub mod stroke_style;
mod text_renderer;
mod text_state;
mod truetype_font_renderer;
pub mod type1_font_renderer;
mod type3_font_renderer;

impl<B: CanvasBackend> PdfOperatorBackend for PdfCanvas<'_, B> {}
