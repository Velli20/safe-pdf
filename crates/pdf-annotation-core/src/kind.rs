//! Core-owned annotation vocabulary with its validation, translation, and bounds.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::{
    error::ValidationError,
    models::{Highlight, Ink, Point, Quad, TextNote, validate_point, validate_quad},
    pdf_data::LineEndingStyle,
    style::ResolvedStyle,
    widgets::Widget,
};
use pdf_graphics::rect::Rect;

/// Source `/Rect` retained beside a live annotation and the page point it was measured against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceLayout {
    /// Normalized copy of the source rectangle.
    pub rect: Rect<f64>,
    /// Page point `rect` was measured against: the first ink point, else its origin.
    pub anchor: Point,
}

/// Core-owned annotation vocabulary; future variants can extend the boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AnnotationKind {
    /// Text-selection regions rendered as a highlight.
    Highlight(Highlight),
    /// Anchored textual comment.
    TextNote(TextNote),
    /// Completed freehand paths.
    Ink(Ink),
    /// Page-local native control bound to a shared document field.
    Widget(Widget),
    /// Styled page text; rich variants are retained without a plain-text editor.
    FreeText(Box<crate::free_text::FreeText>),
    /// Link geometry and host-owned navigation metadata.
    Link(Box<crate::subtypes::Link>),
    /// Straight line with exactly two vertices.
    Line(Box<crate::subtypes::PathAnnotation>),
    /// Rectangle with style and decoration metadata.
    Square(Box<crate::subtypes::BoxAnnotation>),
    /// Ellipse enclosed by its bounds.
    Circle(Box<crate::subtypes::BoxAnnotation>),
    /// Closed path with at least three vertices; closure is implicit.
    Polygon(Box<crate::subtypes::PathAnnotation>),
    /// Open path with at least two vertices.
    PolyLine(Box<crate::subtypes::PathAnnotation>),
    /// Underlined text-selection regions.
    Underline(crate::subtypes::Markup),
    /// Wavy underline regions.
    Squiggly(crate::subtypes::Markup),
    /// Strikeout regions.
    StrikeOut(crate::subtypes::Markup),
    /// Standard or custom stamp identity.
    Stamp(Box<crate::subtypes::Stamp>),
    /// Caret metadata retained for host inspection.
    Caret(Box<crate::subtypes::Caret>),
    /// Popup with an optional live annotation parent.
    Popup(crate::subtypes::Popup),
    /// Unknown subtype preserved without approximating its semantics.
    Unknown(Box<crate::subtypes::UnknownAnnotation>),
}

/// Union of every quad's bounds; markup without quads has no geometry.
fn quads_bounds(quads: &[Quad]) -> Result<Rect<f64>, ValidationError> {
    quads
        .iter()
        .map(Quad::bounds)
        .reduce(|a, b| a.union(&b))
        .ok_or(ValidationError::InvalidHighlight)
}

impl AnnotationKind {
    /// Live page-space bounds of this payload.
    ///
    /// `layout` sizes anchored notes from their source rectangle and positions ink whose
    /// source rectangle was measured against its first point; without it notes are 24 units
    /// square and ink bounds are derived from its strokes and width.
    ///
    /// # Errors
    /// Returns `InvalidHighlight` for markup without quads, `InvalidInk` for ink without
    /// strokes, and `InvalidBounds` when the result is not a finite, positive rectangle.
    pub fn bounds(&self, layout: Option<&SourceLayout>) -> Result<Rect<f64>, ValidationError> {
        let result = match self {
            Self::FreeText(v) => v.bounds,
            Self::Link(v) => v.bounds,
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v) => v.bounds,
            Self::Square(v) | Self::Circle(v) => v.bounds,
            Self::Stamp(v) => v.bounds,
            Self::Caret(v) => v.bounds,
            Self::Popup(v) => v.bounds,
            Self::Unknown(v) => v.bounds.unwrap_or(Rect {
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
            }),
            Self::Widget(v) => v.bounds,
            Self::TextNote(v) => Rect {
                left: v.anchor.x,
                top: v.anchor.y,
                right: v.anchor.x + layout.map_or(24.0, |l| l.rect.width()),
                bottom: v.anchor.y + layout.map_or(24.0, |l| l.rect.height()),
            },
            Self::Highlight(v) => quads_bounds(&v.quads)?,
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => quads_bounds(&v.quads)?,
            Self::Ink(v) => {
                if let (Some(l), Some(first)) = (
                    layout,
                    v.strokes.iter().flat_map(|stroke| &stroke.points).next(),
                ) {
                    let mut b = l.rect;
                    b.translate(first.x - l.anchor.x, first.y - l.anchor.y);
                    b
                } else {
                    let half = v.width / 2.0;
                    let b = v
                        .strokes
                        .iter()
                        .filter_map(|stroke| stroke.bounds())
                        .reduce(|a, b| a.union(&b))
                        .ok_or(ValidationError::InvalidInk)?;
                    Rect {
                        left: b.left - half,
                        top: b.top - half,
                        right: b.right + half,
                        bottom: b.bottom + half,
                    }
                }
            }
        };
        if !result.is_valid() {
            return Err(ValidationError::InvalidBounds);
        }
        Ok(result)
    }

    /// Describe why legacy native presentation cannot faithfully display this payload.
    /// Returns `None` for supported payloads.
    pub fn unsupported_reason(&self) -> Option<&'static str> {
        match self {
            Self::FreeText(v) if !v.is_plain() => {
                Some("Rich text, callouts, and free-text border effects are not supported")
            }
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v)
                if v.endings
                    .iter()
                    .flatten()
                    .any(|e| matches!(e, LineEndingStyle::Unknown(_))) =>
            {
                Some("Unsupported line ending")
            }
            Self::Stamp(v) if !v.has_standard_name() => Some("Custom stamp artwork"),
            Self::Caret(_) | Self::Unknown(_) => Some("Unsupported annotation subtype"),
            _ => None,
        }
    }

    /// PDF `/Subtype` name of this payload; unknown subtypes keep their retained name.
    pub fn subtype_name(&self) -> Cow<'_, str> {
        match self {
            Self::Unknown(v) => String::from_utf8_lossy(&v.subtype),
            Self::Highlight(_) => Cow::Borrowed("Highlight"),
            Self::TextNote(_) => Cow::Borrowed("Text"),
            Self::Ink(_) => Cow::Borrowed("Ink"),
            Self::Widget(_) => Cow::Borrowed("Widget"),
            Self::FreeText(_) => Cow::Borrowed("FreeText"),
            Self::Link(_) => Cow::Borrowed("Link"),
            Self::Line(_) => Cow::Borrowed("Line"),
            Self::Square(_) => Cow::Borrowed("Square"),
            Self::Circle(_) => Cow::Borrowed("Circle"),
            Self::Polygon(_) => Cow::Borrowed("Polygon"),
            Self::PolyLine(_) => Cow::Borrowed("PolyLine"),
            Self::Underline(_) => Cow::Borrowed("Underline"),
            Self::Squiggly(_) => Cow::Borrowed("Squiggly"),
            Self::StrikeOut(_) => Cow::Borrowed("StrikeOut"),
            Self::Stamp(_) => Cow::Borrowed("Stamp"),
            Self::Caret(_) => Cow::Borrowed("Caret"),
            Self::Popup(_) => Cow::Borrowed("Popup"),
        }
    }

    /// Live Unicode text of payloads that carry their own; `None` for the rest.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::FreeText(v) => Some(&v.text),
            Self::TextNote(v) => Some(&v.text),
            Self::Popup(v) => Some(&v.text),
            _ => None,
        }
    }

    /// Quadrilateral regions of markup and links; `None` for payloads without quads.
    pub fn quads(&self) -> Option<&[Quad]> {
        match self {
            Self::Highlight(v) => Some(&v.quads),
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => Some(&v.quads),
            Self::Link(v) => v.quads.as_deref(),
            _ => None,
        }
    }

    /// Decoded stamp name or note icon name; empty for other payloads.
    pub fn label(&self) -> String {
        match self {
            Self::Stamp(v) => String::from_utf8_lossy(&v.name).into_owned(),
            Self::TextNote(v) => v
                .properties
                .as_ref()
                .and_then(|p| p.name.as_deref())
                .map(|name| String::from_utf8_lossy(name).into_owned())
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Current presentation style owned by this payload.
    ///
    /// Compact highlight, note, and ink payloads derive one from their color and
    /// width; widgets without a style use the default. Free text composes its style
    /// over the retained source, and caret, popup, and unknown payloads carry none,
    /// so those return `None`.
    pub fn live_style(&self) -> Option<ResolvedStyle> {
        Some(match self {
            Self::Link(v) => v.style.clone(),
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v) => v.style.clone(),
            Self::Square(v) | Self::Circle(v) => v.style.clone(),
            Self::Stamp(v) => v.style.clone(),
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => v.style.clone(),
            Self::Widget(v) => v.style.clone().unwrap_or_default(),
            Self::Highlight(v) => v
                .style
                .clone()
                .unwrap_or_else(|| ResolvedStyle::from_compact(v.color, 0.0)),
            Self::TextNote(v) => v
                .style
                .clone()
                .unwrap_or_else(|| ResolvedStyle::from_compact(v.color, 0.0)),
            Self::Ink(v) => v
                .style
                .clone()
                .unwrap_or_else(|| ResolvedStyle::from_compact(v.color, v.width)),
            Self::FreeText(_) | Self::Caret(_) | Self::Popup(_) | Self::Unknown(_) => return None,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        let color = match self {
            Self::Highlight(highlight) => {
                if let Some(style) = &highlight.style {
                    style.validate()?;
                }
                if highlight.quads.is_empty() {
                    return Err(ValidationError::InvalidHighlight);
                }
                for quad in &highlight.quads {
                    validate_quad(quad)?;
                }
                highlight.color
            }
            Self::TextNote(note) => {
                if let Some(style) = &note.style {
                    style.validate()?;
                }
                crate::validation::validate(&note.properties)?;
                validate_point(note.anchor)?;
                note.color
            }
            Self::Ink(ink) => {
                if let Some(style) = &ink.style {
                    style.validate()?;
                }
                if ink.strokes.is_empty()
                    || ink
                        .strokes
                        .iter()
                        .any(|stroke| stroke.closed || stroke.points.len() < 2)
                {
                    return Err(ValidationError::InvalidInk);
                }
                for point in ink.strokes.iter().flat_map(|stroke| &stroke.points) {
                    validate_point(*point)?;
                }
                if !ink.width.is_finite() || ink.width <= 0.0 {
                    return Err(ValidationError::InvalidStyle { field: "width" });
                }
                ink.color
            }
            Self::Widget(widget) => {
                if let Some(style) = &widget.style {
                    style.validate()?;
                }
                if !widget.bounds.is_valid() {
                    return Err(ValidationError::InvalidWidgetBounds);
                }
                return Ok(());
            }
            value => return value.validate_extended(),
        };
        if ![color.r, color.g, color.b, color.a]
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(ValidationError::InvalidStyle { field: "color" });
        }
        Ok(())
    }

    fn validate_extended(&self) -> Result<(), ValidationError> {
        match self {
            Self::FreeText(v) => v.validate(),
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v) => {
                let count = v.vertices.points.len();
                let valid = match self {
                    Self::Line(_) => count == 2,
                    Self::Polygon(_) => count >= 3,
                    _ => count >= 2,
                };
                if !valid || v.vertices.closed != matches!(self, Self::Polygon(_)) {
                    return Err(ValidationError::InvalidPath { field: "vertices" });
                }
                if !v.bounds.is_valid() {
                    return Err(ValidationError::InvalidBounds);
                }
                for point in &v.vertices.points {
                    validate_point(*point)?;
                }
                v.style.validate()
            }
            Self::Square(v) | Self::Circle(v) => {
                if !v.bounds.is_valid() {
                    return Err(ValidationError::InvalidBounds);
                }
                if v.insets
                    .is_some_and(|values| values.iter().any(|value| *value < 0.0))
                {
                    return Err(ValidationError::InvalidStyle {
                        field: "shape insets",
                    });
                }
                v.style.validate()
            }
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => {
                if v.quads.is_empty() {
                    return Err(ValidationError::InvalidHighlight);
                }
                for quad in &v.quads {
                    validate_quad(quad)?;
                }
                v.style.validate()
            }
            Self::Link(v) => {
                if !v.bounds.is_valid() {
                    return Err(ValidationError::InvalidBounds);
                }
                for quad in v.quads.iter().flatten() {
                    validate_quad(quad)?;
                }
                v.style.validate()
            }
            Self::Stamp(v) => {
                if !v.bounds.is_valid() {
                    return Err(ValidationError::InvalidBounds);
                }
                v.style.validate()
            }
            Self::Caret(v) => {
                if !v.bounds.is_valid() {
                    return Err(ValidationError::InvalidBounds);
                }
                Ok(())
            }
            Self::Popup(v) => {
                if !v.bounds.is_valid() {
                    return Err(ValidationError::InvalidBounds);
                }
                Ok(())
            }
            Self::Unknown(v) => {
                if v.subtype.is_empty() {
                    return Err(ValidationError::InvalidMetadata {
                        field: "unknown subtype name",
                    });
                }
                if v.bounds.is_some_and(|bounds| !bounds.is_valid()) {
                    return Err(ValidationError::InvalidBounds);
                }
                Ok(())
            }
            Self::Highlight(_) | Self::TextNote(_) | Self::Ink(_) | Self::Widget(_) => Ok(()),
        }?;
        crate::validation::validate(self)
    }

    pub(crate) fn translate(&mut self, delta: Point) -> Result<(), ValidationError> {
        validate_point(delta)?;
        // Page clamping belongs to the host: Core has no page dimensions.
        match &mut *self {
            Self::Highlight(value) => {
                for point in value.quads.iter_mut().flat_map(|quad| &mut quad.corners) {
                    point.translate(delta.x, delta.y);
                }
            }
            Self::TextNote(value) => value.anchor.translate(delta.x, delta.y),
            Self::Ink(value) => {
                for line in value.strokes.iter_mut() {
                    line.translate(delta.x, delta.y);
                }
            }
            Self::Widget(value) => value.bounds.translate(delta.x, delta.y),
            Self::FreeText(v) => v.translate(delta),
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v) => {
                v.bounds.translate(delta.x, delta.y);
                v.vertices.translate(delta.x, delta.y);
            }
            Self::Square(v) | Self::Circle(v) => v.bounds.translate(delta.x, delta.y),
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => {
                for point in v.quads.iter_mut().flat_map(|q| &mut q.corners) {
                    point.translate(delta.x, delta.y);
                }
            }
            Self::Link(v) => {
                v.bounds.translate(delta.x, delta.y);
                for point in v.quads.iter_mut().flatten().flat_map(|q| &mut q.corners) {
                    point.translate(delta.x, delta.y);
                }
            }
            Self::Stamp(v) => v.bounds.translate(delta.x, delta.y),
            Self::Caret(v) => v.bounds.translate(delta.x, delta.y),
            Self::Popup(v) => v.bounds.translate(delta.x, delta.y),
            Self::Unknown(v) => {
                if let Some(bounds) = &mut v.bounds {
                    bounds.translate(delta.x, delta.y);
                }
            }
        }
        self.validate()
    }
}
