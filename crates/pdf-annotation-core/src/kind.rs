//! Core-owned annotation vocabulary with its translation and live bounds.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::{
    error::ValidationError,
    fields::FieldId,
    models::{AnnotationId, Highlight, Ink, Point, Quad, TextNote},
    pdf_data::LineEndingStyle,
    style::ResolvedStyle,
    widgets::Widget,
};
use pdf_graphics::{Bounds, Translate, rect::Rect};

/// Source `/Rect` retained beside a live annotation and the page point it was measured against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceLayout {
    /// Normalized copy of the source rectangle.
    pub rect: Rect,
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

impl AnnotationKind {
    /// Live page-space bounds of this payload.
    ///
    /// `layout` sizes anchored notes from their source rectangle and positions ink whose
    /// source rectangle was measured against its first point, or places ink without strokes
    /// at that rectangle; without it notes are 24 units square and ink bounds are derived
    /// from its strokes and width.
    ///
    /// # Errors
    /// Returns `InvalidQuads` for markup without quads, `InvalidInk` for ink without
    /// strokes or a source layout, and `InvalidBounds` when the result is not a finite, positive rectangle.
    pub fn bounds(&self, layout: Option<&SourceLayout>) -> Result<Rect, ValidationError> {
        let result = match self {
            Self::FreeText(v) => v.bounds,
            Self::Link(v) => v.bounds,
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v) => v.bounds,
            Self::Square(v) | Self::Circle(v) => v.bounds,
            Self::Stamp(v) => v.bounds,
            Self::Caret(v) => v.bounds,
            Self::Popup(v) => v.bounds,
            Self::Unknown(v) => v.bounds.unwrap_or(Rect::UNIT_RECT),
            Self::Widget(v) => v.bounds,
            Self::TextNote(v) => Rect {
                left: v.anchor.x,
                top: v.anchor.y,
                right: v.anchor.x + layout.map_or(24.0, |l| l.rect.width()),
                bottom: v.anchor.y + layout.map_or(24.0, |l| l.rect.height()),
            },
            Self::Highlight(v) => v.quads.bounds().ok_or(ValidationError::InvalidQuads)?,
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => {
                v.quads.bounds().ok_or(ValidationError::InvalidQuads)?
            }
            Self::Ink(v) => {
                let first = v.strokes.iter().flat_map(|stroke| &stroke.points).next();
                if let Some(l) = layout
                    && (first.is_some() || v.strokes.is_empty())
                {
                    let mut b = l.rect;
                    if let Some(first) = first {
                        b.translate(Point::new(first.x - l.anchor.x, first.y - l.anchor.y));
                    }
                    b
                } else {
                    let half = v.style.stroke_width() / 2.0;
                    let b = v.strokes.bounds().ok_or(ValidationError::InvalidInk)?;
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

    /// Field a widget is bound to; `None` for every other payload.
    pub fn bound_field(&self) -> Option<FieldId> {
        match self {
            Self::Widget(widget) => Some(widget.binding.field()),
            _ => None,
        }
    }

    /// Live parent of a popup; `None` for parentless popups and other payloads.
    pub fn popup_parent(&self) -> Option<AnnotationId> {
        match self {
            Self::Popup(popup) => popup.parent,
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
    /// Widgets without a style use the default; caret, popup, and unknown payloads
    /// carry none, so those return `None`.
    pub fn live_style(&self) -> Option<ResolvedStyle> {
        Some(match self {
            Self::Highlight(v) => v.style.clone(),
            Self::TextNote(v) => v.style.clone(),
            Self::Ink(v) => v.style.clone(),
            Self::FreeText(v) => v.style.clone(),
            Self::Link(v) => v.style.clone(),
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v) => v.style.clone(),
            Self::Square(v) | Self::Circle(v) => v.style.clone(),
            Self::Stamp(v) => v.style.clone(),
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => v.style.clone(),
            Self::Widget(v) => v.style.clone().unwrap_or_default(),
            Self::Caret(_) | Self::Popup(_) | Self::Unknown(_) => return None,
        })
    }

    /// Move every geometry point by `delta`; the caller validates the result.
    pub(crate) fn translate(&mut self, delta: Point) {
        // Page clamping belongs to the host: Core has no page dimensions.
        match &mut *self {
            Self::Highlight(value) => value.quads.translate(delta),
            Self::TextNote(value) => value.anchor.translate(delta),
            Self::Ink(value) => value.strokes.translate(delta),
            Self::Widget(value) => value.bounds.translate(delta),
            Self::FreeText(v) => v.translate(delta),
            Self::Line(v) | Self::Polygon(v) | Self::PolyLine(v) => {
                v.bounds.translate(delta);
                v.vertices.translate(delta);
            }
            Self::Square(v) | Self::Circle(v) => v.bounds.translate(delta),
            Self::Underline(v) | Self::Squiggly(v) | Self::StrikeOut(v) => v.quads.translate(delta),
            Self::Link(v) => {
                v.bounds.translate(delta);
                v.quads.translate(delta);
            }
            Self::Stamp(v) => v.bounds.translate(delta),
            Self::Caret(v) => v.bounds.translate(delta),
            Self::Popup(v) => v.bounds.translate(delta),
            Self::Unknown(v) => v.bounds.translate(delta),
        }
    }
}
