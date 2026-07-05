//! Parses and renders PDF annotations.
//!
//! Annotation data types are re-exported from `pdf-annotation-types`; rendering
//! support lives in this crate.

mod render;

pub use crate::render::{AnnotationInteractionState, AnnotationRenderError, AnnotationRenderer};
pub use pdf_annotation_types::*;
