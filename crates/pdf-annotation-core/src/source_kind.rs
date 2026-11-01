//! Normalize one live annotation payload from the source record that describes it.
//!
//! Style is resolved once from the retained metadata; each subtype then copies the
//! geometry and properties it owns. Widgets need the binding their field group
//! assigned and popups the live identity of their parent.
use crate::{
    error::SourceAdoptionError,
    free_text::FreeText,
    kind::AnnotationKind,
    metadata::AnnotationMetadata,
    models::{AnnotationId, Highlight, Ink, NewAnnotation, PageIndex, Point, TextNote},
    pdf_data::*,
    style::resolve_source_style,
    subtypes::{
        BoxAnnotation, Caret, Link, Markup, PathAnnotation, Popup, Stamp, UnknownAnnotation,
    },
    widgets::{Widget, WidgetBinding},
};
use pdf_graphics::polyline::Polyline;

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
    ) -> Result<Self, SourceAdoptionError> {
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
    /// Returns `Validation` for an unusable resolved style, `MissingRect` or
    /// `InvalidRect` for kinds that need a usable rectangle, `InvalidTextString` for
    /// undecodable contents, and `InvalidSourceField` for a widget without a binding.
    /// The payload itself is validated when the engine accepts it.
    pub fn from_source(
        source: &SourceAnnotation,
        binding: Option<WidgetBinding>,
        parent: Option<AnnotationId>,
    ) -> Result<Self, SourceAdoptionError> {
        let style = resolve_source_style(source)?;
        Ok(match &source.kind {
            // Notes are anchored at the top-left corner of their source rectangle.
            NativeAnnotation::Text(v) => Self::TextNote(TextNote {
                properties: Some(v.clone()),
                anchor: source.bounds()?.origin(),
                text: source.contents()?,
                style,
            }),
            NativeAnnotation::Highlight(v) => Self::Highlight(Highlight {
                quads: v.quad_points.clone(),
                style,
            }),
            NativeAnnotation::Underline(v) => Self::Underline(Markup {
                quads: v.quad_points.clone(),
                style,
            }),
            NativeAnnotation::Squiggly(v) => Self::Squiggly(Markup {
                quads: v.quad_points.clone(),
                style,
            }),
            NativeAnnotation::StrikeOut(v) => Self::StrikeOut(Markup {
                quads: v.quad_points.clone(),
                style,
            }),
            NativeAnnotation::Ink(v) => Self::Ink(Ink {
                strokes: v.ink_list.strokes.clone(),
                style,
            }),
            NativeAnnotation::FreeText(v) => Self::FreeText(Box::new(FreeText {
                bounds: source.bounds()?,
                text: source.contents()?,
                style,
                rich_contents: v.rich_contents.clone(),
                callout: v.callout_line.clone(),
                border_effect: v.border_effect.clone(),
            })),
            NativeAnnotation::Link(v) => Self::Link(Box::new(Link {
                bounds: source.bounds()?,
                quads: v.quad_points.clone(),
                highlight_mode: v.highlight_mode.clone(),
                destination: v.destination.clone(),
                action: v.action.clone(),
                style,
            })),
            // Line endpoints become an open two-vertex path.
            NativeAnnotation::Line(v) => {
                let [x, y, x2, y2] = v.line;
                Self::Line(Box::new(PathAnnotation {
                    bounds: source.bounds()?,
                    vertices: Polyline::new(vec![Point { x, y }, Point { x: x2, y: y2 }], false),
                    endings: v.line_endings.clone(),
                    style,
                }))
            }
            // Polygon and polyline vertices are retained as parsed, including closure.
            NativeAnnotation::Polygon(v) => Self::Polygon(Box::new(PathAnnotation {
                bounds: source.bounds()?,
                vertices: v.vertices.clone(),
                endings: v.line_endings.clone(),
                style,
            })),
            NativeAnnotation::PolyLine(v) => Self::PolyLine(Box::new(PathAnnotation {
                bounds: source.bounds()?,
                vertices: v.vertices.clone(),
                endings: v.line_endings.clone(),
                style,
            })),
            NativeAnnotation::Square(_) => Self::Square(Box::new(BoxAnnotation {
                bounds: source.bounds()?,
                style,
            })),
            NativeAnnotation::Circle(_) => Self::Circle(Box::new(BoxAnnotation {
                bounds: source.bounds()?,
                style,
            })),
            // A stamp without a name keeps an empty name rather than approximating one.
            NativeAnnotation::Stamp(v) => Self::Stamp(Box::new(Stamp {
                bounds: source.bounds()?,
                name: v.name.clone().unwrap_or_default(),
                style,
            })),
            NativeAnnotation::Caret(v) => Self::Caret(Box::new(Caret {
                bounds: source.bounds()?,
                properties: *v.clone(),
            })),
            NativeAnnotation::Popup(v) => Self::Popup(Popup {
                bounds: source.bounds()?,
                parent,
                open: v.open.unwrap_or(false),
                text: source.contents()?,
            }),
            NativeAnnotation::Widget(_) => Self::Widget(Widget {
                style: Some(style),
                bounds: source.bounds()?,
                binding: binding.ok_or(SourceAdoptionError::InvalidSourceField {
                    reason: "widget without field binding",
                })?,
            }),
            // Unknown subtypes keep their bounds only when the source has a rectangle at all.
            NativeAnnotation::Unknown { subtype } => Self::Unknown(Box::new(UnknownAnnotation {
                subtype: subtype.to_vec(),
                bounds: source.rect.map(|_| source.bounds()).transpose()?,
            })),
        })
    }
}
