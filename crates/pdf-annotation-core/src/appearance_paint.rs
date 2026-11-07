//! Source appearances a host paints in place of a native presentation: widgets and ink
//! without strokes on the page canvas, stamp artwork into the stamp's own presentation.
use crate::{
    engine::DocumentView,
    kind::AnnotationKind as Kind,
    layer_error::AnnotationLayerResult,
    models::Annotation,
    pdf_data::{AppearanceStreams, SourceAnnotation},
    widget_control::ButtonState,
};
use pdf_graphics::rect::Rect;
use pdf_object_reader::pdf_object::PdfObject;
use pdf_optional_content::OptionalContentState;

/// The `/Off` appearance state of an unselected button.
const OFF_STATE: &[u8] = b"Off";

/// One annotation's appearance stream, selected by its live state and placed at its rectangle.
#[derive(Clone, Debug)]
pub struct PaintedAppearance {
    /// The appearance stream drawn: `/AP /D` while pressed when it has the state, else `/N`.
    pub stream: PdfObject,
    /// The annotation rectangle in PDF page user space.
    pub rect: Rect,
}

impl PaintedAppearance {
    /// The appearance of a displayed annotation painted on the canvas, or `None`.
    ///
    /// Widgets whose control is `painted` draw their live state: checkboxes and radio
    /// options their on-state while live Core state selects them and `Off` otherwise, push
    /// buttons their source `/AS`. A `pressed` widget draws its down appearance when it has
    /// one for that state. Ink without strokes draws its source `/AS` state.
    ///
    /// # Errors
    /// Returns an error when a widget's bound field is missing.
    pub(crate) fn new(
        annotation: &Annotation,
        view: &DocumentView<'_>,
        optional_content: &OptionalContentState,
        pressed: bool,
    ) -> AnnotationLayerResult<Option<Self>> {
        if !annotation.is_displayed(optional_content) {
            return Ok(None);
        }
        if annotation.is_drawn_from_appearance() {
            return Ok(annotation
                .source()
                .and_then(|source| source.normal_appearance(source.rect?)));
        }
        let Some(field) = annotation.widget_field(view)? else {
            return Ok(None);
        };
        let Some(source) = annotation.source() else {
            return Ok(None);
        };
        let state = match annotation.button_state(field) {
            Some(ButtonState::Source) => source.appearance_state.as_deref(),
            Some(ButtonState::Checked(true)) => source.button_on_state().ok(),
            Some(ButtonState::Checked(false)) => Some(OFF_STATE),
            None => return Ok(None),
        };
        Ok(source
            .rect
            .and_then(|rect| source.widget_appearance(state, pressed, rect)))
    }

    /// The source artwork of a displayed stamp at its live bounds, or `None`.
    ///
    /// The host draws it into the stamp's own presentation rather than the page
    /// canvas, so the artwork moves with the stamp while it is dragged.
    pub(crate) fn artwork(
        annotation: &Annotation,
        optional_content: &OptionalContentState,
    ) -> Option<Self> {
        let Kind::Stamp(stamp) = &annotation.content else {
            return None;
        };
        if !annotation.is_displayed(optional_content) {
            return None;
        }
        annotation.source()?.normal_appearance(stamp.bounds)
    }
}

impl SourceAnnotation {
    /// The normal appearance selected by the source `/AS`, drawn at `rect`.
    fn normal_appearance(&self, rect: Rect) -> Option<PaintedAppearance> {
        let normal = self.appearance.as_ref()?.normal.as_ref()?;
        let stream = normal.stream(self.appearance_state.as_deref())?;
        Some(PaintedAppearance {
            stream: stream.clone(),
            rect: rect.normalized(),
        })
    }

    /// A widget's appearance for `state`, drawn at the source `rect`: the down appearance
    /// while `pressed` when it has one for `state`, else the normal appearance.
    fn widget_appearance(
        &self,
        state: Option<&[u8]>,
        pressed: bool,
        rect: Rect,
    ) -> Option<PaintedAppearance> {
        let appearance = self.appearance.as_ref()?;
        let normal = appearance.normal.as_ref()?;
        let stream = appearance
            .down
            .as_ref()
            .filter(|_| pressed)
            .and_then(|down| down.stream(state))
            .or_else(|| normal.stream(state))?;
        Some(PaintedAppearance {
            stream: stream.clone(),
            rect: rect.normalized(),
        })
    }
}

impl AppearanceStreams {
    /// The stream drawn for `state`: the single stream whatever the state, or the
    /// stream named `state`.
    pub(crate) fn stream(&self, state: Option<&[u8]>) -> Option<&PdfObject> {
        match self {
            Self::Stream(stream) => Some(stream),
            Self::States(states) => {
                let state = state?;
                states
                    .iter()
                    .find(|candidate| *candidate.name == *state)
                    .map(|candidate| &candidate.stream)
            }
        }
    }
}
