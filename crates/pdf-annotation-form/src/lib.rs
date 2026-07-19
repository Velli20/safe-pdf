//! Edits and interacts with PDF annotation forms.
//!
//! Annotation parsing and data types live in `pdf-annotation-types`; appearance
//! rendering lives in `pdf-annotations`.

mod free_text_appearance;
mod free_text_appearance_annotation_generator;
mod free_text_appearance_stream_builder;
mod free_text_appearance_style_deriver;
mod free_text_appearance_style_scanner;
pub mod free_text_editing;
mod free_text_layout;
mod free_text_layout_geometry;
mod free_text_layout_validation;
mod free_text_layout_wrapping;
mod free_text_style;
mod interaction_annotation_target;
mod interaction_click_tracker;
mod interaction_controller;
mod interaction_drag_session;
mod interaction_edit_session;
mod interaction_hit;
mod interaction_hit_tester;
mod interaction_listbox_layout;
mod interaction_listbox_metrics;
mod interaction_listbox_row;
mod interaction_overlay;
mod interaction_types;
pub mod interaction_viewport;
mod interaction_visible_listbox_rows;
pub mod widget_editing;

pub use crate::free_text_editing::{FreeText, FreeTextEditError, FreeTextEditor};
pub use crate::free_text_style::{FreeTextBorder, FreeTextFont, FreeTextOverflow, FreeTextStyle};
pub use crate::interaction_controller::AnnotationController;
pub use crate::interaction_types::{
    AnnotationControllerOptions, AnnotationEditCommand, AnnotationInteractionError,
    AnnotationInteractionResult, AnnotationPointerMove, AnnotationPointerPress,
};
pub use crate::interaction_viewport::AnnotationViewport;
pub use crate::widget_editing::{WidgetActivation, WidgetEditError, WidgetEditor};
