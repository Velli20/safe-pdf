//! The [`Validate`] trait shared by annotation payloads, styles, and field definitions,
//! with its impls for geometry types that payloads embed.
//!
//! Each rule rejects NaN and infinities in the values it covers.
use crate::error::ValidationError;
use crate::models::{Point, Quad};
use pdf_graphics::{polyline::Polyline, rect::Rect};

/// Semantic rules of a self-contained value.
///
/// Rules that need surrounding state, such as a page count or a bound field, are
/// methods on the type that owns that state instead.
pub(crate) trait Validate {
    /// Error a rule violation reports.
    type Error: Into<ValidationError>;

    /// Check range, count, and cross-property rules.
    fn validate(&self) -> Result<(), Self::Error>;
}

/// An absent value is valid; a present one follows its own rules.
impl<T: Validate> Validate for Option<T> {
    type Error = T::Error;

    fn validate(&self) -> Result<(), Self::Error> {
        self.as_ref().map_or(Ok(()), T::validate)
    }
}

/// Bounds need finite edges and positive extents.
impl Validate for Rect {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        if self.is_valid() {
            Ok(())
        } else {
            Err(ValidationError::InvalidBounds)
        }
    }
}

/// Markup needs at least one region, and every region must be convex, which implies
/// finite corners.
impl Validate for [Quad] {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        if !self.is_empty() && self.iter().all(Quad::is_convex) {
            Ok(())
        } else {
            Err(ValidationError::InvalidQuads)
        }
    }
}

/// Points must be finite.
impl Validate for Point {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        if self.is_finite() {
            Ok(())
        } else {
            Err(ValidationError::NonFinite)
        }
    }
}

/// Vertices must be finite; closure and count are the owner's rules.
impl Validate for Polyline {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), ValidationError> {
        if self.points.iter().all(|point| point.is_finite()) {
            Ok(())
        } else {
            Err(ValidationError::NonFinite)
        }
    }
}
