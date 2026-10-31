//! Source appearances a host paints on the page canvas in place of a native control or
//! native shapes.
use crate::{
    engine::DocumentView, entry::ControlKind, layer_error::AnnotationLayerResult,
    models::Annotation, projection, widget_control,
};
use pdf_graphics::rect::Rect;
use pdf_object_reader::pdf_object::PdfObject;
use pdf_optional_content::OptionalContentState;

/// The `/Off` appearance state of an unselected button.
const OFF_STATE: &[u8] = b"Off";

/// One annotation's normal appearance, selected by its live state and placed at its
/// rectangle.
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
        let Some(state) = annotation.painted_state(view)? else {
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
        Ok(Some(Self {
            normal,
            state,
            rect: rect.normalized(),
        }))
    }
}

impl Annotation {
    /// The appearance state to paint, or `None` when the annotation is not painted.
    ///
    /// # Errors
    /// Returns an error when a widget's bound field is missing.
    fn painted_state(
        &self,
        view: &DocumentView<'_>,
    ) -> AnnotationLayerResult<Option<Option<Vec<u8>>>> {
        let source_state = || {
            self.source()
                .and_then(|source| source.appearance_state.as_deref())
                .map(<[u8]>::to_vec)
        };
        if self.is_drawn_from_appearance() {
            return Ok(Some(source_state()));
        }
        let Some(field) = projection::widget_field(self, view)? else {
            return Ok(None);
        };
        let Some(control) = widget_control::control(self, field).filter(|c| c.painted) else {
            return Ok(None);
        };
        Ok(Some(match control.kind {
            ControlKind::Button => source_state(),
            _ if control.checked => self
                .source()
                .and_then(|source| source.button_on_state().ok())
                .map(<[u8]>::to_vec),
            _ => Some(OFF_STATE.to_vec()),
        }))
    }
}
