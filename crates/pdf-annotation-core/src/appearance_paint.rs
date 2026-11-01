//! Source appearances a host paints in place of a native presentation: widgets and ink
//! without strokes on the page canvas, stamp artwork into the stamp's own presentation.
use crate::{
    engine::DocumentView, entry::ControlKind, kind::AnnotationKind as Kind,
    layer_error::AnnotationLayerResult, models::Annotation, projection, subtypes::Stamp,
    widget_control,
};
use pdf_graphics::rect::Rect;
use pdf_object_reader::pdf_object::PdfObject;
use pdf_optional_content::OptionalContentState;

/// The `/Off` appearance state of an unselected button.
const OFF_STATE: &[u8] = b"Off";

/// One annotation's normal appearance, selected by its live state and placed at its rectangle.
#[derive(Clone, Debug)]
pub struct PaintedAppearance {
    /// The resolved `/AP /N` entry, or `/D` while pressed: a drawing stream, or a
    /// dictionary of state streams.
    pub normal: PdfObject,
    /// The state to draw from a state dictionary; `None` draws nothing from one.
    pub state: Option<Vec<u8>>,
    /// The annotation rectangle in PDF page user space.
    pub rect: Rect,
}

impl PaintedAppearance {
    /// The appearance of a displayed annotation painted on the canvas, or `None`.
    ///
    /// Widgets whose control is `painted` draw their live state: checkboxes and radio
    /// options their on-state while live Core state selects them and `Off` otherwise, push
    /// buttons their source `/AS`. A `pressed` widget draws its down appearance when it has
    /// one. Ink without strokes draws its source `/AS` state.
    ///
    /// # Errors
    /// Returns an error when a widget's bound field is missing.
    pub(crate) fn new(
        annotation: &Annotation,
        view: &DocumentView<'_>,
        optional_content: &OptionalContentState,
        pressed: bool,
    ) -> AnnotationLayerResult<Option<Self>> {
        if !projection::displayed(annotation, optional_content) {
            return Ok(None);
        }
        if annotation.is_drawn_from_appearance() {
            return Ok(drawn_from_appearance(annotation));
        }
        widget(annotation, view, pressed)
    }

    /// The source artwork of a displayed stamp at its live bounds, or `None`.
    ///
    /// The host draws it into the stamp's own presentation rather than the page
    /// canvas, so the artwork moves with the stamp while it is dragged.
    pub(crate) fn artwork(
        annotation: &Annotation,
        optional_content: &OptionalContentState,
    ) -> Option<Self> {
        let Kind::Stamp(payload) = &annotation.content else {
            return None;
        };
        if !projection::displayed(annotation, optional_content) {
            return None;
        }
        stamp(annotation, payload)
    }
}

/// A widget whose control is painted draws its appearance for the live field state.
fn widget(
    annotation: &Annotation,
    view: &DocumentView<'_>,
    pressed: bool,
) -> AnnotationLayerResult<Option<PaintedAppearance>> {
    let Some(field) = projection::widget_field(annotation, view)? else {
        return Ok(None);
    };
    let Some(control) = widget_control::control(annotation, field).filter(|c| c.painted) else {
        return Ok(None);
    };
    let Some(source) = annotation.source() else {
        return Ok(None);
    };
    let appearance = source.appearance.as_ref();
    let down = appearance
        .and_then(|ap| ap.down.clone())
        .filter(|_| pressed);
    let (Some(normal), Some(rect)) = (
        down.or_else(|| appearance.and_then(|ap| ap.normal.clone())),
        source.rect,
    ) else {
        return Ok(None);
    };
    let state = match control.kind {
        ControlKind::Button => source.appearance_state.as_deref().map(<[u8]>::to_vec),
        _ if control.checked => source.button_on_state().ok().map(<[u8]>::to_vec),
        _ => Some(OFF_STATE.to_vec()),
    };
    Ok(Some(PaintedAppearance {
        normal,
        state,
        rect: rect.normalized(),
    }))
}

/// Ink without strokes draws its source normal appearance, selected by `/AS`, at its
/// source rectangle.
fn drawn_from_appearance(annotation: &Annotation) -> Option<PaintedAppearance> {
    source_normal_appearance(annotation, annotation.source()?.rect?)
}

/// A stamp draws its source normal appearance, selected by `/AS`, at its live bounds.
fn stamp(annotation: &Annotation, stamp: &Stamp) -> Option<PaintedAppearance> {
    source_normal_appearance(annotation, stamp.bounds)
}

/// The source normal appearance, selected by `/AS`, drawn at `rect`.
fn source_normal_appearance(annotation: &Annotation, rect: Rect) -> Option<PaintedAppearance> {
    let source = annotation.source()?;
    Some(PaintedAppearance {
        normal: source.appearance.as_ref()?.normal.clone()?,
        state: source.appearance_state.as_deref().map(<[u8]>::to_vec),
        rect: rect.normalized(),
    })
}
