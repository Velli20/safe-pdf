//! Parses and renders PDF annotations.
//!
//! Annotation data types are re-exported from `pdf-annotation-types`; rendering
//! support lives in this crate. Editing and interaction support lives in
//! `pdf-annotation-form`.

#[path = "render.rs"]
pub mod rendering;

pub use crate::rendering::{AnnotationAppearanceState, AnnotationRenderError, AnnotationRenderer};
pub use pdf_annotation_types::*;
