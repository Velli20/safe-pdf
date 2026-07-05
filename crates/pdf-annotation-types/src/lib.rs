//! Reads page annotation dictionaries from the `/Annots` entry.
//!
//! This crate materializes PDF 1.7 annotations into typed Rust models while
//! preserving original dictionaries for compatibility and debugging. Parsing is
//! strict for any annotation field that is present: malformed values return
//! [`AnnotationError`].

mod action;
mod annotation;
mod annotation_border;
mod annotation_color;
mod annotation_kind;
mod appearance_characteristics;
mod appearance_dictionary;
mod border_effect;
mod border_effect_style;
mod border_style;
mod border_style_name;
mod caret_symbol_style;
mod destination;
mod error;
mod file_specification;
mod helpers;
mod ink_list;
mod line_ending_style;
mod link_highlight_mode;
mod optional_content;
mod quad_points;
mod rendition;
mod subtypes;
mod three_d;

pub use crate::action::AnnotationAction;
pub use crate::annotation::Annotation;
pub use crate::annotation_border::AnnotationBorder;
pub use crate::annotation_color::AnnotationColor;
pub use crate::annotation_kind::AnnotationKind;
pub use crate::appearance_characteristics::AppearanceCharacteristics;
pub use crate::appearance_dictionary::{AppearanceDictionary, AppearanceField};
pub use crate::border_effect::BorderEffect;
pub use crate::border_effect_style::BorderEffectStyle;
pub use crate::border_style::BorderStyle;
pub use crate::border_style_name::BorderStyleName;
pub use crate::caret_symbol_style::CaretSymbolStyle;
pub use crate::destination::{AnnotationDestination, DestinationTarget, ExplicitDestination};
pub use crate::error::AnnotationError;
pub use crate::file_specification::{FileSpecification, FileSpecificationDictionary};
pub use crate::ink_list::InkList;
pub use crate::line_ending_style::LineEndingStyle;
pub use crate::link_highlight_mode::LinkHighlightMode;
pub use crate::optional_content::OptionalContent;
pub use crate::quad_points::QuadPoints;
pub use crate::rendition::Rendition;
pub use crate::subtypes::*;
pub use crate::three_d::{MovieActivation, ThreeDAnnotation, ThreeDView};
