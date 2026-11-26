//! Source appearances a host paints in place of a native presentation: widgets, ink
//! without strokes, and unchanged squares and circles on the page canvas, stamp, note and free-text artwork into the
//! annotation's own presentation.
use crate::{
    engine::DocumentView,
    fields::Field,
    kind::AnnotationKind as Kind,
    layer_error::AnnotationLayerResult,
    models::{Annotation, AnnotationId},
    pdf_data::{AppearanceStreams, SourceAnnotation},
    variable_text::VariableText,
    widget_control::PaintedState,
};
use pdf_graphics::rect::Rect;
use pdf_object_reader::pdf_object::PdfObject;
use pdf_optional_content::OptionalContentState;

/// The `/Off` appearance state of an unselected button.
const OFF_STATE: &[u8] = b"Off";

/// Host presentation state that selects which appearance a widget paints. It never
/// advances the document revision.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentationState {
    /// The widget the pointer holds down, painted with its down appearance.
    pub pressed: Option<AnnotationId>,
    /// The text entry being edited, painted without its value, which its control shows.
    pub editing: Option<AnnotationId>,
}

/// One annotation's appearance stream, selected by its live state and placed at its rectangle.
#[derive(Clone, Debug)]
pub struct PaintedAppearance {
    /// The appearance stream drawn: `/AP /D` while pressed when it has the state, else `/N`.
    pub stream: PdfObject,
    /// The annotation rectangle in PDF page user space.
    pub rect: Rect,
    /// The live value of a text-entry widget, drawn in place of the value inside the
    /// stream's `/Tx` marked content; `None` draws the stream as it is.
    pub text: Option<VariableText>,
}

impl PaintedAppearance {
    /// The appearance of a displayed annotation painted on the canvas, or `None`.
    ///
    /// Widgets whose control is `painted` draw their live state: checkboxes and radio
    /// options their on-state while live Core state selects them and `Off` otherwise, push
    /// buttons, read-only text fields, combo boxes whose value is still the source `/V` and
    /// list boxes whose selection is still the source one their source `/AS`, and editable
    /// text fields and editable combo boxes their normal appearance with the live value,
    /// drawn as it is while the value is still its source `/V`. The widget
    /// `presentation` marks pressed draws its down appearance when it has one for that
    /// state, and the text entry it marks editing draws without its value. Ink without
    /// strokes, and squares and circles that still show their source shape, draw their
    /// source `/AS` state.
    ///
    /// # Errors
    /// Returns an error when a widget's bound field is missing.
    pub(crate) fn new(
        annotation: &Annotation,
        view: &DocumentView<'_>,
        optional_content: &OptionalContentState,
        presentation: PresentationState,
    ) -> AnnotationLayerResult<Option<Self>> {
        if !annotation.is_displayed(optional_content) {
            return Ok(None);
        }
        if annotation.paints_source_appearance() {
            return Ok(annotation
                .source()
                .and_then(|source| source.normal_appearance(source.rect?)));
        }
        let Some(field) = annotation.widget_field(view)? else {
            return Ok(None);
        };
        let editing = presentation.is_editing(annotation.id);
        Ok(match annotation.text_entry_appearance(field) {
            Some(appearance) => Some(appearance.presented(annotation, field, editing)),
            // A text field without a value layout cannot leave its value out, so while it
            // is edited only its control shows it.
            None if editing => None,
            None => {
                annotation.widget_state_appearance(field, presentation.is_pressed(annotation.id))
            }
        })
    }

    /// This text-entry appearance as `annotation`, bound to `field`, presents it: without
    /// its value while `editing`, since the control shows it, and drawn as it is while the
    /// value is still the source `/V`, which the source appearance shows as its author
    /// laid it out.
    fn presented(self, annotation: &Annotation, field: &Field, editing: bool) -> Self {
        let text = if editing {
            self.text.map(VariableText::without_value)
        } else if annotation.shows_source_value(field) {
            None
        } else {
            self.text
        };
        Self { text, ..self }
    }

    /// The appearance of the displayed text entry `annotation` with its live value; `None`
    /// for other annotations.
    ///
    /// # Errors
    /// Returns an error when the widget's bound field is missing.
    pub(crate) fn text_entry(
        annotation: &Annotation,
        view: &DocumentView<'_>,
        optional_content: &OptionalContentState,
    ) -> AnnotationLayerResult<Option<Self>> {
        if !annotation.is_displayed(optional_content) {
            return Ok(None);
        }
        Ok(annotation
            .widget_field(view)?
            .and_then(|field| annotation.text_entry_appearance(field)))
    }

    /// The source artwork of a displayed stamp at its live bounds, of a displayed note
    /// at its source rectangle, or of displayed free text at its live bounds while its
    /// text is still the source `/Contents`; otherwise `None`.
    ///
    /// The host draws it into the annotation's own presentation rather than the page
    /// canvas, so the artwork moves with the annotation while it is dragged. Edited
    /// free text no longer matches its source appearance, so the host shows its text.
    pub(crate) fn artwork(
        annotation: &Annotation,
        optional_content: &OptionalContentState,
    ) -> Option<Self> {
        let source = annotation.source()?;
        let rect = match &annotation.content {
            Kind::Stamp(stamp) => stamp.bounds,
            Kind::TextNote(_) => source.rect?,
            Kind::FreeText(free_text) if source.contents().ok()? == free_text.text => {
                free_text.bounds
            }
            _ => return None,
        };
        if !annotation.is_displayed(optional_content) {
            return None;
        }
        source.normal_appearance(rect)
    }
}

impl PresentationState {
    /// Whether the pointer holds the widget `id` down.
    #[must_use]
    pub fn is_pressed(&self, id: AnnotationId) -> bool {
        self.pressed == Some(id)
    }

    /// Whether the text entry `id` is being edited.
    #[must_use]
    pub fn is_editing(&self, id: AnnotationId) -> bool {
        self.editing == Some(id)
    }
}

impl Annotation {
    /// The appearance this widget, bound to `field`, selects by its live state, with its
    /// down appearance while `pressed`; `None` for widgets whose control presents their
    /// value.
    fn widget_state_appearance(&self, field: &Field, pressed: bool) -> Option<PaintedAppearance> {
        let source = self.source()?;
        let state = match self.painted_state(field)? {
            PaintedState::Source => source.appearance_state.as_deref(),
            PaintedState::Checked(true) => source.button_on_state().ok(),
            PaintedState::Checked(false) => Some(OFF_STATE),
        };
        source.widget_appearance(state, pressed, source.rect?)
    }

    /// The normal appearance of this text entry, bound to `field`, with its live value;
    /// `None` for other widgets and annotations.
    fn text_entry_appearance(&self, field: &Field) -> Option<PaintedAppearance> {
        let text = self.variable_text(field)?;
        let source = self.source()?;
        Some(PaintedAppearance {
            text: Some(text),
            ..source.normal_appearance(source.rect?)?
        })
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
            text: None,
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
            text: None,
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

#[cfg(test)]
mod tests {
    use crate::{
        AnnotationCommandRequest, AnnotationOverlay, PresentationState,
        commands::{AnnotationCommand, Operation},
        fields::{ComboValue, OptionId},
        pdf_data::{
            AppearanceDictionary, AppearanceStreams, NativeAnnotation, SourceAnnotation,
            SquareAnnotation, TextAnnotation,
        },
    };
    use pdf_graphics::{point::Point, rect::Rect, size::Size, viewport::PageViewport};
    use pdf_object_reader::{
        Dictionary, EmptySource, ObjectReader, StreamObject, object_variant::ObjectVariant,
        pdf_array::PdfArray, pdf_object::PdfObject, pdf_string::PdfString, string_kind::StringKind,
    };
    use std::sync::Arc;

    #[test]
    fn text_widget_draws_its_appearance_as_is_until_its_value_is_edited() {
        let name = |bytes: &[u8]| PdfString::from(bytes, StringKind::Name);
        let rect = PdfArray::new([4, 0, 40, 842].map(ObjectVariant::Integer));
        let appearance = StreamObject::new(2, 0, Dictionary::from_entries([]), b"0 g".to_vec());
        // The widget declares no `/DA`, so it takes the interactive form's.
        let form_appearance: Arc<[u8]> = Arc::from(b"/Helv 10 Tf 0 g".as_slice());
        // Read-only (`/Ff 4097`) and editable (`/Ff 4096`) multiline fields both draw their
        // stored appearance unchanged while their value is the source `/V`.
        for flags in [4097, 4096] {
            let widget = ObjectVariant::Dictionary(Dictionary::from_entries([
                (b"Subtype".as_slice(), name(b"Widget")),
                (b"FT".as_slice(), name(b"Tx")),
                (b"Ff".as_slice(), ObjectVariant::Integer(flags)),
                (
                    b"V".as_slice(),
                    PdfString::from(b"Lorem", StringKind::Literal),
                ),
                (b"Rect".as_slice(), ObjectVariant::Array(rect.clone())),
                (
                    b"AP".as_slice(),
                    ObjectVariant::Dictionary(Dictionary::from_entries([(
                        b"N".as_slice(),
                        ObjectVariant::Stream(appearance.clone()),
                    )])),
                ),
            ]));
            let source: SourceAnnotation = ObjectReader::new(EmptySource).read(&widget).unwrap();
            let mut overlay = AnnotationOverlay::new(
                [std::slice::from_ref(&source)],
                None,
                Some(&form_appearance),
            )
            .unwrap();
            let appearances = overlay
                .painted_appearances(0, PresentationState::default())
                .unwrap();
            let painted = appearances.first().unwrap();
            assert_eq!(appearances.len(), 1, "/Ff {flags}");
            assert_eq!(Some(painted.rect), source.rect);
            assert_eq!(
                painted.stream.value(),
                &ObjectVariant::Stream(appearance.clone())
            );
            assert!(painted.text.is_none(), "/Ff {flags}");
            if flags == 4096 {
                // An edited value is laid out in the form's `/DA` font instead.
                let page = Rect {
                    left: 0.0,
                    top: 0.0,
                    right: 612.0,
                    bottom: 842.0,
                };
                let viewport = PageViewport::new(page, 0, Size::new(612.0, 842.0)).unwrap();
                let prepared = overlay.prepare(0, &viewport, 0).unwrap();
                let field = prepared
                    .entries()
                    .first()
                    .unwrap()
                    .control
                    .as_ref()
                    .unwrap()
                    .field;
                let request = AnnotationCommandRequest {
                    target: None,
                    command: AnnotationCommand {
                        expected_revision: overlay.revision(),
                        operation: Operation::SetText {
                            field,
                            value: "Ipsum".into(),
                        },
                    },
                };
                overlay.accept_event(request, None, None).unwrap();
                let appearances = overlay
                    .painted_appearances(0, PresentationState::default())
                    .unwrap();
                let text = appearances.first().unwrap().text.as_ref().unwrap();
                assert_eq!(text.value, "Ipsum");
                assert_eq!(text.font.as_ref(), b"Helv");
            }
        }
    }

    #[test]
    fn combo_box_draws_its_appearance_as_is_until_its_value_changes() {
        let name = |bytes: &[u8]| PdfString::from(bytes, StringKind::Name);
        let literal = |bytes: &[u8]| PdfString::from(bytes, StringKind::Literal);
        // The appearance shows `/V` as its format action laid it out: `3.00`, not `3`.
        let appearance = StreamObject::new(
            2,
            0,
            Dictionary::from_entries([]),
            b"/Tx BMC BT /Helv 12 Tf (3.00) Tj ET EMC".to_vec(),
        );
        // `/Ff 131072` is a closed combo box, shown by a select. The second holds a value a
        // calculation stored outside its (absent) options.
        let options: [&[&[u8]]; 2] = [&[b"1", b"2", b"3"], &[]];
        for (options, value) in options.into_iter().zip([b"3", b"6"]) {
            let opt: Vec<_> = options.iter().map(|option| literal(option)).collect();
            let widget = ObjectVariant::Dictionary(Dictionary::from_entries([
                (b"Subtype".as_slice(), name(b"Widget")),
                (b"FT".as_slice(), name(b"Ch")),
                (b"Ff".as_slice(), ObjectVariant::Integer(131_072)),
                (b"Opt".as_slice(), ObjectVariant::Array(PdfArray::new(opt))),
                (b"V".as_slice(), literal(value)),
                (b"DA".as_slice(), literal(b"/Helv 12 Tf 0 g")),
                (
                    b"Rect".as_slice(),
                    ObjectVariant::Array(PdfArray::new(
                        [255, 631, 372, 703].map(ObjectVariant::Integer),
                    )),
                ),
                (
                    b"AP".as_slice(),
                    ObjectVariant::Dictionary(Dictionary::from_entries([(
                        b"N".as_slice(),
                        ObjectVariant::Stream(appearance.clone()),
                    )])),
                ),
            ]));
            let source: SourceAnnotation = ObjectReader::new(EmptySource).read(&widget).unwrap();
            let mut overlay =
                AnnotationOverlay::new([std::slice::from_ref(&source)], None, None).unwrap();
            let page = Rect {
                left: 0.0,
                top: 0.0,
                right: 612.0,
                bottom: 792.0,
            };
            let viewport = PageViewport::new(page, 0, Size::new(612.0, 792.0)).unwrap();
            let prepared = overlay.prepare(0, &viewport, 0).unwrap();
            let control = prepared.entries().first().unwrap().control.clone().unwrap();
            assert!(control.painted);
            let appearances = overlay
                .painted_appearances(0, PresentationState::default())
                .unwrap();
            let painted = appearances.first().unwrap();
            assert_eq!(appearances.len(), 1);
            assert_eq!(
                painted.stream.value(),
                &ObjectVariant::Stream(appearance.clone())
            );
            assert!(painted.text.is_none());
            if options.is_empty() {
                continue;
            }

            // Another option no longer matches the appearance, so the select shows it.
            let request = AnnotationCommandRequest {
                target: None,
                command: AnnotationCommand {
                    expected_revision: overlay.revision(),
                    operation: Operation::SetComboValue {
                        field: control.field,
                        value: ComboValue::Option(OptionId(0)),
                    },
                },
            };
            overlay.accept_event(request, None, None).unwrap();
            let appearances = overlay
                .painted_appearances(0, PresentationState::default())
                .unwrap();
            assert!(appearances.is_empty());
            let prepared = overlay.prepare(0, &viewport, 0).unwrap();
            let control = prepared
                .entries()
                .first()
                .unwrap()
                .control
                .as_ref()
                .unwrap();
            assert!(!control.painted);
        }
    }

    #[test]
    fn signature_widget_paints_its_normal_appearance_without_placeholder() {
        let name = |bytes: &[u8]| PdfString::from(bytes, StringKind::Name);
        let appearance = StreamObject::new(2, 0, Dictionary::from_entries([]), b"q Q".to_vec());
        let widget = ObjectVariant::Dictionary(Dictionary::from_entries([
            (b"Subtype".as_slice(), name(b"Widget")),
            (b"FT".as_slice(), name(b"Sig")),
            (
                b"Rect".as_slice(),
                ObjectVariant::Array(PdfArray::new(
                    [224, 429, 373, 496].map(ObjectVariant::Integer),
                )),
            ),
            (
                b"AP".as_slice(),
                ObjectVariant::Dictionary(Dictionary::from_entries([(
                    b"N".as_slice(),
                    ObjectVariant::Stream(appearance.clone()),
                )])),
            ),
        ]));
        let source: SourceAnnotation = ObjectReader::new(EmptySource).read(&widget).unwrap();
        let mut overlay =
            AnnotationOverlay::new([std::slice::from_ref(&source)], None, None).unwrap();
        let page = Rect {
            left: 0.0,
            top: 0.0,
            right: 612.0,
            bottom: 792.0,
        };
        let viewport = PageViewport::new(page, 0, Size::new(612.0, 792.0)).unwrap();
        let prepared = overlay.prepare(0, &viewport, 0).unwrap();
        let entry = prepared.entries().first().unwrap();
        assert_eq!(entry.unsupported, None);
        assert!(entry.control.is_none());
        let appearances = overlay
            .painted_appearances(0, PresentationState::default())
            .unwrap();
        let painted = appearances.first().unwrap();
        assert_eq!(appearances.len(), 1);
        assert_eq!(Some(painted.rect), source.rect);
        assert_eq!(painted.stream.value(), &ObjectVariant::Stream(appearance));
    }

    #[test]
    fn square_paints_its_appearance_on_the_canvas_until_moved() {
        let rect = Rect {
            left: 315.0,
            top: 285.0,
            right: 559.0,
            bottom: 405.0,
        };
        let square = SourceAnnotation {
            id: 1,
            object_number: Some(43),
            opacity: 1.0,
            subtype: b"Square".as_slice().into(),
            rect: Some(rect),
            contents: None,
            name: None,
            flags: None,
            appearance: Some(AppearanceDictionary {
                normal: Some(AppearanceStreams::Stream(PdfObject::new(
                    ObjectVariant::Null,
                ))),
                down: None,
            }),
            appearance_state: None,
            border: None,
            color: None,
            struct_parent: None,
            optional_content: None,
            kind: NativeAnnotation::Square(Box::new(SquareAnnotation {
                border_style: None,
                interior_color: None,
                border_effect: None,
                difference_rect: None,
            })),
        };
        let mut overlay =
            AnnotationOverlay::new([std::slice::from_ref(&square)], None, None).unwrap();
        let page = Rect {
            left: 0.0,
            top: 0.0,
            right: 612.0,
            bottom: 792.0,
        };
        let viewport = PageViewport::new(page, 0, Size::new(612.0, 792.0)).unwrap();
        let prepared = overlay.prepare(0, &viewport, 0).unwrap();
        let entry = prepared.entries().first().unwrap();
        assert!(entry.painted && entry.draggable);
        let id = entry.id;
        let appearances = overlay
            .painted_appearances(0, PresentationState::default())
            .unwrap();
        assert_eq!(appearances.len(), 1);
        assert_eq!(appearances.first().unwrap().rect, rect);

        let request = AnnotationCommandRequest {
            target: None,
            command: AnnotationCommand {
                expected_revision: overlay.revision(),
                operation: Operation::Translate {
                    id,
                    delta: Point::new(10.0, 0.0),
                },
            },
        };
        overlay.accept_event(request, None, None).unwrap();
        let prepared = overlay.prepare(0, &viewport, 1).unwrap();
        assert!(!prepared.entries().first().unwrap().painted);
        let appearances = overlay
            .painted_appearances(0, PresentationState::default())
            .unwrap();
        assert!(appearances.is_empty());
    }

    #[test]
    fn text_note_with_normal_appearance_is_drawn_as_artwork() {
        let rect = Rect {
            left: 117.0,
            top: 154.0,
            right: 135.0,
            bottom: 176.0,
        };
        let note = SourceAnnotation {
            id: 1,
            object_number: Some(6),
            opacity: 1.0,
            subtype: b"Text".as_slice().into(),
            rect: Some(rect),
            contents: Some(b"Note".as_slice().into()),
            name: None,
            flags: None,
            appearance: Some(AppearanceDictionary {
                normal: Some(AppearanceStreams::Stream(PdfObject::new(
                    ObjectVariant::Null,
                ))),
                down: None,
            }),
            appearance_state: None,
            border: None,
            color: None,
            struct_parent: None,
            optional_content: None,
            kind: NativeAnnotation::Text(Box::new(TextAnnotation {
                open: None,
                name: None,
                state: None,
                state_model: None,
                intent: None,
            })),
        };
        let mut overlay =
            AnnotationOverlay::new([std::slice::from_ref(&note)], None, None).unwrap();
        let page = Rect {
            left: 0.0,
            top: 0.0,
            right: 450.0,
            bottom: 250.0,
        };
        let viewport = PageViewport::new(page, 0, Size::new(450.0, 250.0)).unwrap();
        let prepared = overlay.prepare(0, &viewport, 0).unwrap();
        let entry = prepared.entries().first().unwrap();
        assert!(entry.artwork);
        assert_eq!(entry.unsupported, None);
        let artwork = overlay.artwork(entry.id).unwrap();
        assert_eq!(artwork.rect, rect.normalized());
    }

    #[test]
    fn rich_free_text_with_normal_appearance_is_drawn_as_artwork() {
        let string = |bytes: &[u8], kind| PdfString::from(bytes, kind);
        let appearance = StreamObject::new(2, 0, Dictionary::from_entries([]), b"0 g".to_vec());
        let free_text = ObjectVariant::Dictionary(Dictionary::from_entries([
            (b"Subtype".as_slice(), string(b"FreeText", StringKind::Name)),
            (
                b"Rect".as_slice(),
                ObjectVariant::Array(PdfArray::new(
                    [180, 612, 479, 626].map(ObjectVariant::Integer),
                )),
            ),
            (b"Contents".as_slice(), string(b"PDF", StringKind::Literal)),
            (
                b"DA".as_slice(),
                string(b"/Helv 12 Tf", StringKind::Literal),
            ),
            (
                b"RC".as_slice(),
                string(b"<body><p>PDF</p></body>", StringKind::Literal),
            ),
            (
                b"AP".as_slice(),
                ObjectVariant::Dictionary(Dictionary::from_entries([(
                    b"N".as_slice(),
                    ObjectVariant::Stream(appearance),
                )])),
            ),
        ]));
        let source: SourceAnnotation = ObjectReader::new(EmptySource).read(&free_text).unwrap();
        let mut overlay =
            AnnotationOverlay::new([std::slice::from_ref(&source)], None, None).unwrap();
        let page = Rect {
            left: 0.0,
            top: 0.0,
            right: 595.0,
            bottom: 842.0,
        };
        let viewport = PageViewport::new(page, 0, Size::new(595.0, 842.0)).unwrap();
        let prepared = overlay.prepare(0, &viewport, 0).unwrap();
        let entry = prepared.entries().first().unwrap();
        assert!(entry.artwork);
        assert_eq!(entry.unsupported, None);
        let artwork = overlay.artwork(entry.id).unwrap();
        assert_eq!(Some(artwork.rect), source.rect);
    }
}
