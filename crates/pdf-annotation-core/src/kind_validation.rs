//! Semantic rules for annotation payloads: positive bounds, path vertex counts,
//! convex regions, and style ranges.
//!
//! Every range below excludes NaN and infinities, so no separate finiteness pass is
//! needed.
use std::ops::RangeBounds;

use crate::error::ValidationError;
use crate::free_text::FreeText;
use crate::kind::AnnotationKind;
use crate::models::{Highlight, Ink, Quad, TextNote};
use crate::style::ResolvedStyle;
use crate::subtypes::{
    BoxAnnotation, Caret, Link, Markup, PathAnnotation, Popup, Stamp, UnknownAnnotation,
};
use crate::validate::Validate;
use crate::widgets::Widget;
use pdf_graphics::color::Color;

impl Validate for AnnotationKind {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Highlight(v) => v.validate(),
            Self::TextNote(v) => v.validate(),
            Self::Ink(v) => v.validate(),
            Self::Widget(v) => v.validate(),
            Self::FreeText(v) => v.validate(),
            Self::Line(v) => v.check_path(false, 2..=2),
            Self::Polygon(v) => v.check_path(true, 3..),
            Self::PolyLine(v) => v.check_path(false, 2..),
            Self::Square(v) | Self::Circle(v) => v.validate(),
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => v.validate(),
            Self::Link(v) => v.validate(),
            Self::Stamp(v) => v.validate(),
            Self::Caret(v) => v.validate(),
            Self::Popup(v) => v.validate(),
            Self::Unknown(v) => v.validate(),
        }
    }
}

/// Payloads whose rules are exactly their listed parts' rules, checked in order.
macro_rules! validate_parts {
    ($($ty:ty { $($part:ident),+ })*) => {$(
        impl Validate for $ty {
            type Error = ValidationError;

            fn validate(&self) -> Result<(), ValidationError> {
                $(self.$part.validate()?;)+
                Ok(())
            }
        }
    )*};
}

validate_parts! {
    Highlight { quads, style }
    Markup { quads, style }
    TextNote { anchor, style }
    Widget { bounds, style }
    BoxAnnotation { bounds, style }
    Stamp { bounds, style }
    Caret { bounds }
    Popup { bounds }
}

/// Strokes are open with at least two points; a source ink without strokes is
/// drawn from its appearance instead.
impl Validate for Ink {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        self.style.validate()?;
        for stroke in &self.strokes {
            if stroke.closed || stroke.points.len() < 2 {
                return Err(ValidationError::InvalidInk);
            }
            stroke.validate()?;
        }
        Ok(())
    }
}

/// Positive bounds, a valid style, and an open two- or three-point callout.
impl Validate for FreeText {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        self.bounds.validate()?;
        self.style.validate()?;
        if self
            .callout
            .as_ref()
            .is_some_and(|callout| callout.closed || !matches!(callout.points.len(), 2 | 3))
        {
            return Err(ValidationError::InvalidPath {
                field: "free-text callout",
            });
        }
        self.callout.validate()
    }
}

/// Link regions are optional, but present ones must be convex.
impl Validate for Link {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        self.bounds.validate()?;
        if !self.quads.iter().flatten().all(Quad::is_convex) {
            return Err(ValidationError::InvalidQuads);
        }
        self.style.validate()
    }
}

/// Unknown subtypes keep their name, and their bounds when present.
impl Validate for UnknownAnnotation {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        if self.subtype.is_empty() {
            return Err(ValidationError::InvalidMetadata {
                field: "unknown subtype name",
            });
        }
        self.bounds.validate()
    }
}

impl PathAnnotation {
    /// Vertices must have the given closure and a count within `count`.
    fn check_path(
        &self,
        closed: bool,
        count: impl RangeBounds<usize>,
    ) -> Result<(), ValidationError> {
        if self.vertices.closed != closed || !count.contains(&self.vertices.points.len()) {
            return Err(ValidationError::InvalidPath { field: "vertices" });
        }
        self.vertices.validate()?;
        self.bounds.validate()?;
        self.style.validate()
    }
}

/// Colors within zero through one, opacity within zero through one, finite
/// nonnegative border width and dash intervals, and a finite positive font size.
impl Validate for ResolvedStyle {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        if ![Some(self.color), self.background, self.border_color]
            .iter()
            .flatten()
            .all(Color::is_valid)
            || !(0.0..=1.0).contains(&self.opacity)
            || !(0.0..=f32::MAX).contains(&self.border_width)
            || !self
                .dash
                .iter()
                .flatten()
                .all(|v| (0.0..=f32::MAX).contains(v))
            || !(f32::MIN_POSITIVE..=f32::MAX).contains(&self.font_size)
        {
            return Err(ValidationError::InvalidStyle {
                field: "resolved style",
            });
        }
        Ok(())
    }
}
