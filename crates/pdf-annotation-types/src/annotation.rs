use pdf_graphics::rect::Rect;
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{
    AnnotationBorder, AnnotationColor, AnnotationError, AnnotationKind, AppearanceDictionary,
    AppearanceField, ButtonStateError, FreeTextAnnotation, OptionalContent, WidgetFieldValue,
    annotation_id::AnnotationId,
};

const OFF_STATE: &[u8] = b"Off";

/// A typed page annotation.
pub struct Annotation {
    id: AnnotationId,
    /// The annotation subtype name.
    pub subtype: Vec<u8>,
    /// The optional annotation rectangle from `/Rect`.
    pub rect: Option<Rect>,
    /// The optional annotation contents.
    pub contents: Option<Vec<u8>>,
    /// The optional annotation name from `/NM`.
    pub name: Option<Vec<u8>>,
    /// The optional annotation flags from `/F`.
    pub flags: Option<i32>,
    /// The optional appearance dictionary from `/AP`.
    pub appearance: Option<AppearanceDictionary>,
    /// The optional appearance state from `/AS`.
    pub appearance_state: Option<Vec<u8>>,
    /// The optional border array from `/Border`.
    pub border: Option<AnnotationBorder>,
    /// The optional annotation color from `/C`.
    pub color: Option<AnnotationColor>,
    /// The optional structure parent index from `/StructParent`.
    pub struct_parent: Option<usize>,
    /// The optional optional-content membership dictionary from `/OC`.
    pub optional_content: Option<OptionalContent>,
    /// The parsed subtype-specific payload.
    pub kind: AnnotationKind,
}

impl Annotation {
    /// Reads page annotations while preserving page-local identifier assignment.
    pub fn from_page_dictionary<A: ObjectAccess + ?Sized>(
        context: &mut pdf_object_reader::DictionaryContext<'_, A>,
    ) -> ReadResult<Option<Vec<Self>>> {
        let Some(annots) = context.optional::<pdf_object_reader::pdf_array::PdfArray>(b"Annots")?
        else {
            return Ok(None);
        };
        let mut annotations = Vec::with_capacity(annots.len());
        for value in annots.iter() {
            let Ok(dictionary) = value.try_dictionary(context.source()) else {
                continue;
            };
            if dictionary.get(b"Subtype").is_none() {
                continue;
            }
            let mut annotation: Self = context.read(value)?;
            annotation.id = AnnotationId::from_page_value(annotations.len());
            annotations.push(annotation);
        }
        Ok(Some(annotations))
    }

    /// Returns this annotation's stable, page-scoped runtime identifier.
    pub const fn id(&self) -> AnnotationId {
        self.id
    }

    /// Returns this button's active non-`/Off` normal appearance state.
    ///
    /// A valid current `/AS` state is preferred. Otherwise, this selects the
    /// first non-`/Off` key from the normal appearance subdictionary. Returns
    /// [`ButtonStateError::MissingOnState`] when no such state is available.
    pub fn button_on_state(&self) -> Result<Vec<u8>, ButtonStateError> {
        let Some(appearance) = self.appearance.as_ref() else {
            return Err(ButtonStateError::MissingOnState { id: self.id });
        };

        let Some(AppearanceField::Subdictionary(states)) = appearance.normal.as_ref() else {
            return Err(ButtonStateError::MissingOnState { id: self.id });
        };

        if let Some(state) = self
            .appearance_state
            .as_deref()
            .filter(|state| *state != OFF_STATE && states.contains_key(*state))
        {
            return Ok(state.to_owned());
        }

        states
            .keys()
            .find(|state| state.as_slice() != OFF_STATE)
            .cloned()
            .ok_or(ButtonStateError::MissingOnState { id: self.id })
    }

    /// Applies a button appearance state to this annotation's `/AS` entry.
    ///
    /// `None` represents the PDF button off state and is stored as `/Off`.
    pub fn set_button_appearance_state(&mut self, state: Option<&[u8]>) {
        self.appearance_state = Some(state.unwrap_or(OFF_STATE).to_vec());
    }

    /// Synchronizes this widget annotation's `/V` field with a button state.
    ///
    /// `None` represents the PDF button off state and is stored as `/Off`.
    /// This method leaves non-widget annotations unchanged.
    pub fn set_button_value(&mut self, state: Option<&[u8]>) {
        let AnnotationKind::Widget(widget) = &mut self.kind else {
            return;
        };
        widget.value = Some(WidgetFieldValue::Bytes(state.unwrap_or(OFF_STATE).to_vec()));
    }

    /// Assigns an identifier when attaching an annotation to a page.
    #[doc(hidden)]
    pub fn set_id(&mut self, id: AnnotationId) {
        self.id = id;
    }

    /// Creates a detached free text annotation with generated appearance data.
    #[doc(hidden)]
    pub fn new_free_text(
        rect: Rect,
        contents: Vec<u8>,
        appearance: AppearanceDictionary,
        border: Option<AnnotationBorder>,
        color: Option<AnnotationColor>,
        free_text: FreeTextAnnotation,
    ) -> Self {
        Self {
            id: AnnotationId::from_page_value(0),
            subtype: b"FreeText".to_vec(),
            rect: Some(rect),
            contents: Some(contents),
            name: None,
            flags: None,
            appearance: Some(appearance),
            appearance_state: None,
            border,
            color,
            struct_parent: None,
            optional_content: None,
            kind: AnnotationKind::FreeText(free_text),
        }
    }
}

impl FromPdfObject for Annotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let raw = context.dictionary().clone();
        let dictionary = &raw;
        let objects = context.source();
        // Some PDFs omit `/Type` on annotation dictionaries even though the
        // entry is nominally expected to be `/Annot`, so only validate it when
        // the key is actually present.
        if let Some(annotation_type) = dictionary.optional_bytes(b"Type", objects)? {
            match annotation_type {
                b"Annot" => {}
                other => {
                    return Err(AnnotationError::InvalidEntry {
                        entry: b"Type",
                        reason: format!("expected /Annot, found /{other:?}"),
                    }
                    .into());
                }
            }
        }

        // `/Subtype` identifies the concrete annotation kind and is required
        // for dispatching to the subtype-specific parser.
        let subtype = dictionary
            .get_or_err(b"Subtype")?
            .try_bytes(objects)?
            .to_vec();

        let rect = dictionary
            .get(b"Rect")
            .map(|value| {
                value.try_array_of::<f32, 4>(objects).map(|arr| {
                    let [left, bottom, right, top] = arr;
                    Rect {
                        left,
                        bottom,
                        right,
                        top,
                    }
                })
            })
            .transpose()?;

        let kind = AnnotationKind::from_dictionary(&subtype, dictionary, objects)?;

        let contents = dictionary.optional_bytes_vec(b"Contents", objects)?;
        let name = dictionary.optional_bytes_vec(b"NM", objects)?;
        let flags = dictionary.optional_number::<i32>(b"F", objects)?;
        let appearance_state = dictionary
            .get(b"AS")
            .map(|value| value.try_bytes(objects).map(Vec::from))
            .transpose()?;
        let struct_parent = dictionary.optional_number::<usize>(b"StructParent", objects)?;

        let optional_content = OptionalContent::from_dictionary(dictionary, objects)?;
        let border = AnnotationBorder::from_dictionary(dictionary, objects)?;
        let color = AnnotationColor::from_dictionary(dictionary, b"C", objects)?;

        let appearance = context.optional(b"AP")?;

        Ok(Self {
            id: AnnotationId::from_page_value(0),
            subtype,
            rect,
            contents,
            name,
            flags,
            appearance,
            appearance_state,
            border,
            color,
            struct_parent,
            optional_content,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use pdf_object_reader::Dictionary;
    use std::collections::BTreeMap;

    use pdf_object_reader::{
        object_error::ObjectError,
        object_resolver::{ObjectResolver, PassthroughResolver},
        object_variant::ObjectVariant,
        stream::StreamObject,
    };

    use super::*;

    struct TestResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl pdf_object_reader::ObjectSource for TestResolver {
        type Error = ObjectError;
        fn read_object(
            &self,
            id: pdf_object_reader::object_id::ObjectId,
        ) -> Result<Option<pdf_object_reader::pdf_object::PdfObject>, Self::Error> {
            Ok(self
                .objects
                .get(&id.number())
                .cloned()
                .map(pdf_object_reader::pdf_object::PdfObject::new))
        }
    }

    impl ObjectResolver for TestResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            let mut current = obj;

            // Follow reference chains the same way the real reader does so the
            // page-level annotation test exercises indirect annotation objects.
            while let ObjectVariant::Reference(object_number) = current {
                current = self.objects.get(&object_number.number).ok_or(
                    ObjectError::FailedResolveObjectReference {
                        obj_num: object_number.number,
                    },
                )?;
            }

            Ok(current)
        }
    }

    fn parse_annotation(dictionary: Dictionary) -> pdf_object_reader::ReadResult<Annotation> {
        let objects = PassthroughResolver;

        let reader = pdf_object_reader::ObjectReader::new(&objects);

        reader.read::<Annotation>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((&dictionary).clone()),
        )
    }

    fn annotation_dictionary(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
        let mut values = BTreeMap::new();
        values.insert(
            Vec::from(b"Subtype"),
            ObjectVariant::Name(b"Popup".to_vec()),
        );
        for (key, value) in entries {
            values.insert(Vec::from(key.as_bytes()), value);
        }
        Dictionary::new(values)
    }

    fn appearance_stream(object_number: usize) -> ObjectVariant {
        let dictionary = Dictionary::new(BTreeMap::from([(
            Vec::from(b"BBox"),
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(0),
                ObjectVariant::Integer(0),
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(1),
            ]),
        )]));
        ObjectVariant::Stream(StreamObject::new(object_number, 0, dictionary, Vec::new()))
    }

    fn button_annotation(appearance_state: Option<&str>, states: &[&str]) -> Annotation {
        let states = states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                (
                    Vec::from(state.as_bytes()),
                    appearance_stream(index.saturating_add(1)),
                )
            })
            .collect();
        let normal = ObjectVariant::Dictionary(Dictionary::new(states));
        let appearance =
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(Vec::from(b"N"), normal)])));

        let mut entries = vec![
            ("Subtype", ObjectVariant::Name(b"Widget".to_vec())),
            ("FT", ObjectVariant::Name(b"Btn".to_vec())),
            ("AP", appearance),
        ];
        if let Some(appearance_state) = appearance_state {
            entries.push((
                "AS",
                ObjectVariant::Name(appearance_state.as_bytes().to_vec()),
            ));
        }

        parse_annotation(Dictionary::new(
            entries
                .into_iter()
                .map(|(key, value)| (Vec::from(key.as_bytes()), value))
                .collect(),
        ))
        .expect("button annotation should parse")
    }

    fn widget_value(annotation: &Annotation) -> Option<&[u8]> {
        let AnnotationKind::Widget(widget) = &annotation.kind else {
            return None;
        };
        let WidgetFieldValue::Bytes(value) = widget.value.as_ref()? else {
            return None;
        };
        Some(value)
    }

    #[test]
    fn missing_type_is_preserved_as_none() {
        let dictionary = annotation_dictionary(vec![(
            "Parent",
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(3, 0)),
        )]);

        let annotation = parse_annotation(dictionary).expect("annotation should parse");

        assert_eq!(annotation.subtype, b"Popup".to_vec());
    }

    #[test]
    fn valid_type_is_still_accepted() {
        let mut values = BTreeMap::new();
        values.insert(Vec::from(b"Type"), ObjectVariant::Name(b"Annot".to_vec()));
        values.insert(
            Vec::from(b"Subtype"),
            ObjectVariant::Name(b"Popup".to_vec()),
        );
        values.insert(
            Vec::from(b"Parent"),
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(3, 0)),
        );

        let annotation =
            parse_annotation(Dictionary::new(values)).expect("annotation should parse");

        assert_eq!(annotation.subtype, b"Popup".to_vec());
    }

    #[test]
    fn missing_rect_is_preserved_as_none() {
        let dictionary = annotation_dictionary(vec![(
            "Parent",
            ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(3, 0)),
        )]);

        let annotation = parse_annotation(dictionary).expect("annotation should parse");

        assert!(annotation.rect.is_none());
    }

    #[test]
    fn button_on_state_prefers_the_current_valid_appearance_state() {
        let annotation = button_annotation(Some("B"), &["Off", "A", "B"]);

        assert_eq!(annotation.button_on_state(), Ok(b"B".to_vec()));
    }

    #[test]
    fn button_on_state_falls_back_to_the_first_non_off_state() {
        let annotation = button_annotation(Some("Missing"), &["Off", "B", "A"]);

        assert_eq!(annotation.button_on_state(), Ok(b"A".to_vec()));
    }

    #[test]
    fn button_on_state_requires_a_non_off_normal_appearance() {
        let annotation = button_annotation(None, &["Off"]);

        assert_eq!(
            annotation.button_on_state(),
            Err(ButtonStateError::MissingOnState {
                id: annotation.id()
            })
        );
    }

    #[test]
    fn button_appearance_and_field_values_are_updated_independently() {
        let mut annotation = button_annotation(None, &[]);

        annotation.set_button_appearance_state(Some(b"Yes"));
        annotation.set_button_value(Some(b"Yes"));
        assert_eq!(
            annotation.appearance_state.as_deref(),
            Some(b"Yes".as_slice())
        );
        assert_eq!(widget_value(&annotation), Some(b"Yes".as_slice()));

        annotation.set_button_appearance_state(None);
        annotation.set_button_value(None);
        assert_eq!(annotation.appearance_state.as_deref(), Some(OFF_STATE));
        assert_eq!(widget_value(&annotation), Some(OFF_STATE));
    }

    #[test]
    fn malformed_present_rect_remains_an_error() {
        let mut values = BTreeMap::new();
        values.insert(Vec::from(b"Type"), ObjectVariant::Name(b"Annot".to_vec()));
        values.insert(
            Vec::from(b"Subtype"),
            ObjectVariant::Name(b"Popup".to_vec()),
        );
        values.insert(
            Vec::from(b"Rect"),
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(0),
                ObjectVariant::Integer(0),
                ObjectVariant::Integer(10),
            ]),
        );

        let error = match parse_annotation(Dictionary::new(values)) {
            Ok(_) => panic!("invalid rect should fail"),
            Err(error) => error,
        };

        assert!(
            matches!(error, pdf_object_reader::ObjectReadError::Decode { source, .. } if source.downcast_ref::<ObjectError>().is_some())
        );
    }

    #[test]
    fn invalid_present_type_remains_an_error() {
        let mut values = BTreeMap::new();
        values.insert(Vec::from(b"Type"), ObjectVariant::Name(b"Page".to_vec()));
        values.insert(
            Vec::from(b"Subtype"),
            ObjectVariant::Name(b"Popup".to_vec()),
        );

        let error = match parse_annotation(Dictionary::new(values)) {
            Ok(_) => panic!("invalid annotation type should fail"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<AnnotationError>(), Some(AnnotationError::InvalidEntry { entry: b"Type", .. }))
        ));
    }

    #[test]
    fn page_dictionary_skips_annotations_without_subtype() {
        let mut objects = BTreeMap::new();
        objects.insert(
            4,
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([(
                Vec::from(b"Type"),
                ObjectVariant::Name(b"Annot".to_vec()),
            )]))),
        );

        objects.insert(
            5,
            ObjectVariant::Dictionary(Dictionary::new(BTreeMap::from([
                (Vec::from(b"Type"), ObjectVariant::Name(b"Annot".to_vec())),
                (
                    Vec::from(b"Subtype"),
                    ObjectVariant::Name(b"Popup".to_vec()),
                ),
            ]))),
        );

        let mut page_values = BTreeMap::new();
        page_values.insert(
            Vec::from(b"Annots"),
            ObjectVariant::Array(vec![
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(4, 0)),
                ObjectVariant::Reference(pdf_object_reader::object_id::ObjectId::new(5, 0)),
            ]),
        );

        let resolver = TestResolver { objects };

        let reader = pdf_object_reader::ObjectReader::new(&resolver);

        let annotations = Annotation::from_page_dictionary(
            &mut pdf_object_reader::ObjectContext::new(
                pdf_object_reader::resolved_object::ResolvedObject::try_from(
                    pdf_object_reader::pdf_object::PdfObject::new(
                        pdf_object_reader::object_variant::ObjectVariant::Dictionary(
                            (&Dictionary::new(page_values)).clone(),
                        ),
                    ),
                )
                .expect("direct page"),
                &mut reader.session(),
            )
            .dictionary()
            .expect("page dictionary"),
        )
        .expect("page annotations should parse");

        let annotations = annotations.expect("annotations should be present");
        // The broken object is skipped, but the valid annotation remains available.
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].subtype, b"Popup".to_vec());
        assert_eq!(annotations[0].id().get(), 0);
    }
}
