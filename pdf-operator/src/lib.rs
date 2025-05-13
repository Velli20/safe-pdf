pub mod clipping_path_operators;
pub mod color_operators;
pub mod error;
pub mod graphics_state_operators;
pub mod marked_content_operators;
pub mod operation_map;
pub mod operator_tokenizer;
pub mod path_operators;
pub mod path_paint_operators;
pub mod pdf_operator;
pub mod text_object_operators;
pub mod text_positioning_operators;
pub mod text_showing_operators;
pub mod text_state_operators;
pub mod xobject_and_image_operators;

extern crate alloc;

// TextElement enum for ShowTextArray operator
#[derive(Debug, Clone, PartialEq)]
pub enum TextElement {
    Text { value: String },
    Adjustment { amount: f32 },
}
