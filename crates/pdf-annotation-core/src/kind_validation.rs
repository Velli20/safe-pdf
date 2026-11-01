//! Semantic rules for annotation payloads: positive bounds, path vertex counts,
//! convex regions, and style ranges.
//!
//! Finiteness and nesting are checked once per input by [`crate::finite_values::check`],
//! so these rules compare ranges only.
use std::ops::RangeBounds;

use crate::error::ValidationError;
use crate::free_text::FreeText;
use crate::kind::AnnotationKind;
use crate::models::{Ink, Quad};
use crate::style::ResolvedStyle;
use crate::subtypes::PathAnnotation;
use pdf_graphics::{color::Color, rect::Rect};

impl AnnotationKind {
    /// Check the payload's semantic rules; numbers must already be finite.
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::Highlight(v) => {
                check_quads(&v.quads)?;
                v.style.validate()
            }
            Self::TextNote(v) => v.style.validate(),
            Self::Ink(v) => v.validate(),
            Self::Widget(v) => {
                check_bounds(&v.bounds)?;
                v.style.as_ref().map_or(Ok(()), ResolvedStyle::validate)
            }
            Self::FreeText(v) => v.validate(),
            Self::Line(v) => v.validate(false, 2..=2),
            Self::Polygon(v) => v.validate(true, 3..),
            Self::PolyLine(v) => v.validate(false, 2..),
            Self::Square(v) | Self::Circle(v) => {
                check_bounds(&v.bounds)?;
                v.style.validate()
            }
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => {
                check_quads(&v.quads)?;
                v.style.validate()
            }
            Self::Link(v) => {
                check_bounds(&v.bounds)?;
                if !v.quads.iter().flatten().all(Quad::is_convex) {
                    return Err(ValidationError::InvalidQuads);
                }
                v.style.validate()
            }
            Self::Stamp(v) => {
                check_bounds(&v.bounds)?;
                v.style.validate()
            }
            Self::Caret(v) => check_bounds(&v.bounds),
            Self::Popup(v) => check_bounds(&v.bounds),
            Self::Unknown(v) => {
                if v.subtype.is_empty() {
                    return Err(ValidationError::InvalidMetadata {
                        field: "unknown subtype name",
                    });
                }
                v.bounds.as_ref().map_or(Ok(()), check_bounds)
            }
        }
    }
}

impl Ink {
    /// Strokes are open with at least two points; a source ink without strokes is
    /// drawn from its appearance instead.
    fn validate(&self) -> Result<(), ValidationError> {
        self.style.validate()?;
        if self
            .strokes
            .iter()
            .any(|stroke| stroke.closed || stroke.points.len() < 2)
        {
            return Err(ValidationError::InvalidInk);
        }
        Ok(())
    }
}

impl PathAnnotation {
    /// Vertices must have the given closure and a count within `count`.
    fn validate(
        &self,
        closed: bool,
        count: impl RangeBounds<usize>,
    ) -> Result<(), ValidationError> {
        if self.vertices.closed != closed || !count.contains(&self.vertices.points.len()) {
            return Err(ValidationError::InvalidPath { field: "vertices" });
        }
        check_bounds(&self.bounds)?;
        self.style.validate()
    }
}

impl FreeText {
    /// Positive bounds, a valid style, and an open two- or three-point callout.
    fn validate(&self) -> Result<(), ValidationError> {
        check_bounds(&self.bounds)?;
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
        Ok(())
    }
}

impl ResolvedStyle {
    /// Colors within zero through one, opacity within zero through one, nonnegative
    /// border width and dash intervals, and a positive font size.
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        if ![Some(self.color), self.background, self.border_color]
            .iter()
            .flatten()
            .all(Color::is_valid)
            || !(0.0..=1.0).contains(&self.opacity)
            || self.border_width < 0.0
            || self.dash.iter().flatten().any(|v| *v < 0.0)
            || self.font_size <= 0.0
        {
            return Err(ValidationError::InvalidStyle {
                field: "resolved style",
            });
        }
        Ok(())
    }
}

/// Bounds need finite edges and positive extents.
fn check_bounds(bounds: &Rect) -> Result<(), ValidationError> {
    if bounds.is_valid() {
        Ok(())
    } else {
        Err(ValidationError::InvalidBounds)
    }
}

/// Markup needs at least one region, and every region must be convex.
fn check_quads(quads: &[Quad]) -> Result<(), ValidationError> {
    if !quads.is_empty() && quads.iter().all(Quad::is_convex) {
        Ok(())
    } else {
        Err(ValidationError::InvalidQuads)
    }
}
