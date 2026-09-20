pub mod clipping_path_operators;
pub mod color_operators;
pub mod compatibility_operators;
pub mod error;
pub mod graphics_state_operators;
pub mod marked_content_operators;
pub mod operands;
pub mod operation_map;
pub mod variants;

pub mod operator_trait;
pub mod path_operators;
pub mod path_paint_operators;
pub mod pdf_operator_backend;
pub mod recording_pdf_operator_backend;
pub mod shadings_operators;
pub mod text_object_operators;
pub mod text_positioning_operators;
pub mod text_showing_operators;
pub mod text_state_operators;
pub mod type3_font_operators;
pub mod xobject_and_image_operators;

extern crate alloc;
use std::sync::Arc;

/// One operand from a PDF `Tj` or `TJ` text-showing operation.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfTextItem {
    /// Encoded PDF string bytes.
    Text(Arc<[u8]>),
    /// Numeric text-position adjustment in thousandths of a text-space unit.
    Adjustment(f32),
}
