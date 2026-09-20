//! Normalize one live annotation payload from the source record that describes it.
//!
//! Style is resolved once from the retained metadata; each subtype then copies the
//! geometry and properties it owns. Widgets need the binding their field group
//! assigned and popups the live identity of their parent.
use crate::{
    error::ValidationError,
    free_text::{FreeText, FreeTextStyle},
    kind::AnnotationKind,
    metadata::AnnotationMetadata,
    models::{AnnotationId, Highlight, Ink, NewAnnotation, PageIndex, Point, Quad, TextNote},
    pdf_data::*,
    style::{ResolvedStyle, resolve_source_style},
    subtypes::{
        BoxAnnotation, Caret, Link, Markup, PathAnnotation, Popup, Stamp, UnknownAnnotation,
    },
    widgets::{Widget, WidgetBinding},
};
use pdf_graphics::{polyline::Polyline, rect::Rect};

impl NewAnnotation {
    /// Draft adopting `source` onto `page`, retaining the source in its metadata.
    ///
    /// # Errors
    /// Propagates [`AnnotationKind::from_source`] errors.
    pub fn from_source(
        page: PageIndex,
        source: &SourceAnnotation,
        binding: Option<WidgetBinding>,
        parent: Option<AnnotationId>,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            page,
            metadata: AnnotationMetadata::from_source(source),
            content: AnnotationKind::from_source(source, binding, parent)?,
        })
    }
}

impl AnnotationKind {
    /// Live payload for `source`, styled by [`resolve_source_style`].
    ///
    /// Widgets require `binding`; popups take their live `parent`, if any.
    ///
    /// # Errors
    /// Returns style and validation errors from the retained metadata, `MissingRect` or
    /// `InvalidBounds` for kinds that need a usable rectangle, `InvalidTextString` for
    /// undecodable contents, and `InvalidSourceField` for a widget without a binding.
    pub fn from_source(
        source: &SourceAnnotation,
        binding: Option<WidgetBinding>,
        parent: Option<AnnotationId>,
    ) -> Result<Self, ValidationError> {
        let style = resolve_source_style(source)?;
        Ok(match &source.kind {
            NativeAnnotation::Text(v) => Self::TextNote(text_note(source, v, &style)?),
            NativeAnnotation::Highlight(v) => Self::Highlight(highlight(v, &style)?),
            NativeAnnotation::Underline(v) => Self::Underline(markup(&v.quad_points, &style)),
            NativeAnnotation::Squiggly(v) => Self::Squiggly(markup(&v.quad_points, &style)),
            NativeAnnotation::StrikeOut(v) => Self::StrikeOut(markup(&v.quad_points, &style)),
            NativeAnnotation::Ink(v) => Self::Ink(ink(v, &style)?),
            NativeAnnotation::FreeText(v) => Self::FreeText(Box::new(free_text(source, v, style)?)),
            NativeAnnotation::Link(v) => Self::Link(Box::new(link(source, v, style)?)),
            NativeAnnotation::Line(v) => Self::Line(Box::new(line(source, v, style)?)),
            NativeAnnotation::Polygon(v) => {
                Self::Polygon(Box::new(path(source, &v.vertices, &v.line_endings, style)?))
            }
            NativeAnnotation::PolyLine(v) => {
                Self::PolyLine(Box::new(path(source, &v.vertices, &v.line_endings, style)?))
            }
            NativeAnnotation::Square(v) => Self::Square(Box::new(shape(
                source,
                &v.border_effect,
                v.difference_rect,
                style,
            )?)),
            NativeAnnotation::Circle(v) => Self::Circle(Box::new(shape(
                source,
                &v.border_effect,
                v.difference_rect,
                style,
            )?)),
            NativeAnnotation::Stamp(v) => Self::Stamp(Box::new(stamp(source, v, style)?)),
            NativeAnnotation::Caret(v) => Self::Caret(Box::new(Caret {
                bounds: source.bounds()?,
                properties: *v.clone(),
            })),
            NativeAnnotation::Popup(v) => Self::Popup(popup(source, v, parent)?),
            NativeAnnotation::Widget(_) => Self::Widget(widget(source, style, binding)?),
            NativeAnnotation::Unknown { subtype } => {
                Self::Unknown(Box::new(unknown(source, subtype)?))
            }
        })
    }
}

/// Note anchored at the top-left corner of its source rectangle.
fn text_note(
    source: &SourceAnnotation,
    v: &TextAnnotation,
    style: &ResolvedStyle,
) -> Result<TextNote, ValidationError> {
    let rect = source.bounds()?;
    Ok(TextNote {
        properties: Some(Box::new(v.clone())),
        style: Some(style.clone()),
        anchor: Point::new(rect.left, rect.top),
        text: source.contents()?,
        color: style.rgba_color()?,
    })
}

fn highlight(v: &HighlightAnnotation, style: &ResolvedStyle) -> Result<Highlight, ValidationError> {
    Ok(Highlight {
        style: Some(style.clone()),
        quads: v.quad_points.clone(),
        color: style.rgba_color()?,
    })
}

/// Underline, squiggly, and strikeout regions share one payload.
fn markup(quads: &[Quad], style: &ResolvedStyle) -> Markup {
    Markup {
        quads: quads.to_vec(),
        style: style.clone(),
    }
}

/// Ink strokes drawn at the style's stroke width.
fn ink(v: &InkAnnotation, style: &ResolvedStyle) -> Result<Ink, ValidationError> {
    Ok(Ink {
        style: Some(style.clone()),
        strokes: v.ink_list.strokes.clone(),
        width: style.stroke_width(),
        color: style.rgba_color()?,
    })
}

fn free_text(
    source: &SourceAnnotation,
    v: &FreeTextAnnotation,
    style: ResolvedStyle,
) -> Result<FreeText, ValidationError> {
    Ok(FreeText {
        bounds: source.bounds()?,
        text: source.contents()?,
        style: FreeTextStyle::from_resolved(&style, v.difference_rect),
        rich_contents: v.rich_contents.clone(),
        callout: v.callout_line.clone(),
        border_effect: v.border_effect.clone(),
    })
}

fn link(
    source: &SourceAnnotation,
    v: &LinkAnnotation,
    style: ResolvedStyle,
) -> Result<Link, ValidationError> {
    Ok(Link {
        bounds: source.bounds()?,
        quads: v.quad_points.clone(),
        highlight_mode: v.highlight_mode.clone(),
        destination: v.destination.clone(),
        action: v.action.clone(),
        style,
    })
}

/// Line endpoints become an open two-vertex path.
fn line(
    source: &SourceAnnotation,
    v: &LineAnnotation,
    style: ResolvedStyle,
) -> Result<PathAnnotation, ValidationError> {
    let [x, y, x2, y2] = v.line;
    let vertices = Polyline::new(vec![Point { x, y }, Point { x: x2, y: y2 }], false);
    path(source, &vertices, &v.line_endings, style)
}

/// Polygon and polyline vertices are retained as parsed, including closure.
fn path(
    source: &SourceAnnotation,
    vertices: &Polyline<f64>,
    endings: &Option<[LineEndingStyle; 2]>,
    style: ResolvedStyle,
) -> Result<PathAnnotation, ValidationError> {
    Ok(PathAnnotation {
        bounds: source.bounds()?,
        vertices: vertices.clone(),
        endings: endings.clone(),
        style,
    })
}

/// Square and circle shapes keep their border effect and difference rectangle.
fn shape(
    source: &SourceAnnotation,
    border_effect: &Option<BorderEffect>,
    insets: Option<[f64; 4]>,
    style: ResolvedStyle,
) -> Result<BoxAnnotation, ValidationError> {
    Ok(BoxAnnotation {
        bounds: source.bounds()?,
        style,
        border_effect: border_effect.clone(),
        insets,
    })
}

/// A stamp without a name keeps an empty name rather than approximating one.
fn stamp(
    source: &SourceAnnotation,
    v: &StampAnnotation,
    style: ResolvedStyle,
) -> Result<Stamp, ValidationError> {
    Ok(Stamp {
        bounds: source.bounds()?,
        name: v.name.clone().unwrap_or_default(),
        style,
    })
}

fn popup(
    source: &SourceAnnotation,
    v: &PopupAnnotation,
    parent: Option<AnnotationId>,
) -> Result<Popup, ValidationError> {
    Ok(Popup {
        bounds: source.bounds()?,
        parent,
        open: v.open.unwrap_or(false),
        text: source.contents()?,
    })
}

fn widget(
    source: &SourceAnnotation,
    style: ResolvedStyle,
    binding: Option<WidgetBinding>,
) -> Result<Widget, ValidationError> {
    Ok(Widget {
        style: Some(style),
        bounds: source.bounds()?,
        binding: binding.ok_or(ValidationError::InvalidSourceField {
            reason: "widget without field binding",
        })?,
    })
}

/// Unknown subtypes keep their bounds only when the source has a rectangle at all.
fn unknown(
    source: &SourceAnnotation,
    subtype: &[u8],
) -> Result<UnknownAnnotation, ValidationError> {
    let bounds: Option<Rect<f64>> = source.rect.map(|_| source.bounds()).transpose()?;
    Ok(UnknownAnnotation {
        subtype: subtype.to_vec(),
        bounds,
    })
}
