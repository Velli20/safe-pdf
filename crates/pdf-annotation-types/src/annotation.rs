use pdf_content_stream::ContentStreamIdAllocator;
use pdf_graphics::rect::Rect;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};
use pdf_resources::{object_reader::ReadCycleTracker, resource_cache::ResourceCache};

use crate::{
    AnnotationBorder, AnnotationColor, AnnotationError, AnnotationKind, AppearanceDictionary,
    AppearanceField, ButtonStateError, FreeTextAnnotation, OptionalContent, WidgetFieldValue,
    annotation_id::AnnotationId,
};

const OFF_STATE: &str = "Off";

/// A typed page annotation.
pub struct Annotation {
    id: AnnotationId,
    /// The annotation subtype name.
    pub subtype: String,
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
    pub appearance_state: Option<String>,
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
    /// Returns this annotation's stable, page-scoped runtime identifier.
    pub const fn id(&self) -> AnnotationId {
        self.id
    }

    /// Returns this button's active non-`/Off` normal appearance state.
    ///
    /// A valid current `/AS` state is preferred. Otherwise, this selects the
    /// first non-`/Off` key from the normal appearance subdictionary. Returns
    /// [`ButtonStateError::MissingOnState`] when no such state is available.
    pub fn button_on_state(&self) -> Result<String, ButtonStateError> {
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
            .find(|state| state.as_str() != OFF_STATE)
            .cloned()
            .ok_or(ButtonStateError::MissingOnState { id: self.id })
    }

    /// Applies a button appearance state to this annotation's `/AS` entry.
    ///
    /// `None` represents the PDF button off state and is stored as `/Off`.
    pub fn set_button_appearance_state(&mut self, state: Option<&str>) {
        self.appearance_state = Some(state.unwrap_or(OFF_STATE).to_owned());
    }

    /// Synchronizes this widget annotation's `/V` field with a button state.
    ///
    /// `None` represents the PDF button off state and is stored as `/Off`.
    /// This method leaves non-widget annotations unchanged.
    pub fn set_button_value(&mut self, state: Option<&str>) {
        let AnnotationKind::Widget(widget) = &mut self.kind else {
            return;
        };
        widget.value = Some(WidgetFieldValue::Bytes(
            state.unwrap_or(OFF_STATE).as_bytes().to_vec(),
        ));
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
            subtype: "FreeText".to_owned(),
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

    /// Reads all page annotations from the optional `/Annots` array.
    pub fn from_page_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Option<Vec<Self>>, AnnotationError> {
        let Some(annots) = dictionary.get("Annots") else {
            return Ok(None);
        };

        let annots = annots.try_array(objects)?;
        let mut annotations = Vec::with_capacity(annots.len());

        for annot in annots {
            let Ok(dictionary) = annot.try_dictionary(objects) else {
                continue;
            };
            // A broken annotation can still be useful to preserve the rest of the page,
            // but without `/Subtype` we cannot dispatch it to a concrete parser.
            if dictionary.get("Subtype").is_none() {
                continue;
            }
            let mut annotation =
                Self::from_dictionary(dictionary, objects, cache, cycle_tracker, id_allocator)?;
            annotation.id = AnnotationId::from_page_value(annotations.len());
            annotations.push(annotation);
        }

        Ok(Some(annotations))
    }

    /// Parses a single resolved annotation dictionary.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, AnnotationError> {
        // Some PDFs omit `/Type` on annotation dictionaries even though the
        // entry is nominally expected to be `/Annot`, so only validate it when
        // the key is actually present.
        if let Some(annotation_type) = dictionary.optional_str("Type", objects)? {
            match annotation_type {
                "Annot" => {}
                other => {
                    return Err(AnnotationError::InvalidEntry {
                        entry: "Type",
                        reason: format!("expected /Annot, found /{other}"),
                    });
                }
            }
        }

        // `/Subtype` identifies the concrete annotation kind and is required
        // for dispatching to the subtype-specific parser.
        let subtype = dictionary.required_str("Subtype", objects)?.to_owned();

        let rect = dictionary
            .get("Rect")
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

        let contents = dictionary.optional_bytes_vec("Contents", objects)?;
        let name = dictionary.optional_bytes_vec("NM", objects)?;
        let flags = dictionary.optional_number::<i32>("F", objects)?;
        let appearance_state = dictionary
            .optional_str("AS", objects)?
            .map(|s| s.to_owned());
        let struct_parent = dictionary.optional_number::<usize>("StructParent", objects)?;

        let appearance = AppearanceDictionary::from_dictionary(
            dictionary,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
        )?;
        let optional_content = OptionalContent::from_dictionary(dictionary, objects)?;
        let border = AnnotationBorder::from_dictionary(dictionary, objects)?;
        let color = AnnotationColor::from_dictionary(dictionary, "C", objects)?;

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
    use std::collections::BTreeMap;

    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        error::ObjectError,
        object_resolver::{ObjectResolver, PassthroughResolver},
        object_variant::ObjectVariant,
        stream::StreamObject,
    };
    use pdf_resources::{object_reader::ReadCycleTracker, resource_cache::DefaultResourceCache};

    use super::*;

    struct TestResolver {
        objects: BTreeMap<usize, ObjectVariant>,
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
                current = self.objects.get(object_number).ok_or(
                    ObjectError::FailedResolveObjectReference {
                        obj_num: *object_number,
                    },
                )?;
            }

            Ok(current)
        }
    }

    fn parse_annotation(dictionary: Dictionary) -> Result<Annotation, AnnotationError> {
        let objects = PassthroughResolver;
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        Annotation::from_dictionary(
            &dictionary,
            &objects,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
    }

    fn annotation_dictionary(entries: Vec<(&str, ObjectVariant)>) -> Dictionary {
        let mut values = BTreeMap::new();
        values.insert("Subtype".to_owned(), ObjectVariant::Name(b"Popup".to_vec()));
        for (key, value) in entries {
            values.insert(key.to_owned(), value);
        }
        Dictionary::new(values)
    }

    fn appearance_stream(object_number: usize) -> ObjectVariant {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "BBox".to_owned(),
            ObjectVariant::Array(vec![
                ObjectVariant::Integer(0),
                ObjectVariant::Integer(0),
                ObjectVariant::Integer(1),
                ObjectVariant::Integer(1),
            ]),
        )]));
        ObjectVariant::Stream(StreamObject::new(
            object_number,
            0,
            Box::new(dictionary),
            Vec::new(),
        ))
    }

    fn button_annotation(appearance_state: Option<&str>, states: &[&str]) -> Annotation {
        let states = states
            .iter()
            .enumerate()
            .map(|(index, state)| {
                (
                    (*state).to_owned(),
                    appearance_stream(index.saturating_add(1)),
                )
            })
            .collect();
        let normal = ObjectVariant::Dictionary(Box::new(Dictionary::new(states)));
        let appearance = ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
            "N".to_owned(),
            normal,
        )]))));

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
                .map(|(key, value)| (key.to_owned(), value))
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
        let dictionary = annotation_dictionary(vec![("Parent", ObjectVariant::Reference(3))]);

        let annotation = parse_annotation(dictionary).expect("annotation should parse");

        assert_eq!(annotation.subtype, "Popup");
    }

    #[test]
    fn valid_type_is_still_accepted() {
        let mut values = BTreeMap::new();
        values.insert("Type".to_owned(), ObjectVariant::Name(b"Annot".to_vec()));
        values.insert("Subtype".to_owned(), ObjectVariant::Name(b"Popup".to_vec()));
        values.insert("Parent".to_owned(), ObjectVariant::Reference(3));

        let annotation =
            parse_annotation(Dictionary::new(values)).expect("annotation should parse");

        assert_eq!(annotation.subtype, "Popup");
    }

    #[test]
    fn missing_rect_is_preserved_as_none() {
        let dictionary = annotation_dictionary(vec![("Parent", ObjectVariant::Reference(3))]);

        let annotation = parse_annotation(dictionary).expect("annotation should parse");

        assert!(annotation.rect.is_none());
    }

    #[test]
    fn button_on_state_prefers_the_current_valid_appearance_state() {
        let annotation = button_annotation(Some("B"), &["Off", "A", "B"]);

        assert_eq!(annotation.button_on_state(), Ok("B".to_owned()));
    }

    #[test]
    fn button_on_state_falls_back_to_the_first_non_off_state() {
        let annotation = button_annotation(Some("Missing"), &["Off", "B", "A"]);

        assert_eq!(annotation.button_on_state(), Ok("A".to_owned()));
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

        annotation.set_button_appearance_state(Some("Yes"));
        annotation.set_button_value(Some("Yes"));
        assert_eq!(annotation.appearance_state.as_deref(), Some("Yes"));
        assert_eq!(widget_value(&annotation), Some(b"Yes".as_slice()));

        annotation.set_button_appearance_state(None);
        annotation.set_button_value(None);
        assert_eq!(annotation.appearance_state.as_deref(), Some(OFF_STATE));
        assert_eq!(widget_value(&annotation), Some(OFF_STATE.as_bytes()));
    }

    #[test]
    fn malformed_present_rect_remains_an_error() {
        let mut values = BTreeMap::new();
        values.insert("Type".to_owned(), ObjectVariant::Name(b"Annot".to_vec()));
        values.insert("Subtype".to_owned(), ObjectVariant::Name(b"Popup".to_vec()));
        values.insert(
            "Rect".to_owned(),
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

        assert!(matches!(error, AnnotationError::Object(_)));
    }

    #[test]
    fn invalid_present_type_remains_an_error() {
        let mut values = BTreeMap::new();
        values.insert("Type".to_owned(), ObjectVariant::Name(b"Page".to_vec()));
        values.insert("Subtype".to_owned(), ObjectVariant::Name(b"Popup".to_vec()));

        let error = match parse_annotation(Dictionary::new(values)) {
            Ok(_) => panic!("invalid annotation type should fail"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AnnotationError::InvalidEntry { entry: "Type", .. }
        ));
    }

    #[test]
    fn page_dictionary_skips_annotations_without_subtype() {
        let mut objects = BTreeMap::new();
        objects.insert(
            4,
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([(
                "Type".to_owned(),
                ObjectVariant::Name(b"Annot".to_vec()),
            )])))),
        );

        objects.insert(
            5,
            ObjectVariant::Dictionary(Box::new(Dictionary::new(BTreeMap::from([
                ("Type".to_owned(), ObjectVariant::Name(b"Annot".to_vec())),
                ("Subtype".to_owned(), ObjectVariant::Name(b"Popup".to_vec())),
            ])))),
        );

        let mut page_values = BTreeMap::new();
        page_values.insert(
            "Annots".to_owned(),
            ObjectVariant::Array(vec![
                ObjectVariant::Reference(4),
                ObjectVariant::Reference(5),
            ]),
        );

        let resolver = TestResolver { objects };
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let annotations = Annotation::from_page_dictionary(
            &Dictionary::new(page_values),
            &resolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("page annotations should parse");

        let annotations = annotations.expect("annotations should be present");
        // The broken object is skipped, but the valid annotation remains available.
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].subtype, "Popup");
        assert_eq!(annotations[0].id().get(), 0);
    }
}
