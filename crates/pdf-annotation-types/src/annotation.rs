use pdf_content_stream::ContentStreamIdAllocator;
use pdf_graphics::rect::Rect;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use pdf_resources::{object_reader::ReadCycleTracker, resource_cache::ResourceCache};

use crate::{
    AnnotationBorder, AnnotationColor, AnnotationError, AnnotationKind, AppearanceDictionary,
    OptionalContent,
};

/// A typed page annotation.
pub struct Annotation {
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
            let dictionary = annot.try_dictionary(objects)?;
            // A broken annotation can still be useful to preserve the rest of the page,
            // but without `/Subtype` we cannot dispatch it to a concrete parser.
            if dictionary.get("Subtype").is_none() {
                continue;
            }
            annotations.push(Self::from_dictionary(
                dictionary,
                objects,
                cache,
                cycle_tracker,
                id_allocator,
            )?);
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
        if let Some(annotation_type) = dictionary.get("Type") {
            match annotation_type.try_str(objects)?.as_ref() {
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
        let subtype = dictionary
            .get_or_err("Subtype")?
            .try_str(objects)?
            .into_owned();

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

        let contents = dictionary
            .get("Contents")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let name = dictionary
            .get("NM")
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let flags = dictionary
            .get("F")
            .map(|value| value.try_number::<i32>(objects))
            .transpose()?;
        let appearance_state = dictionary
            .get("AS")
            .map(|value| value.try_str(objects).map(|s| s.into_owned()))
            .transpose()?;
        let struct_parent = dictionary
            .get("StructParent")
            .map(|value| value.try_number::<usize>(objects))
            .transpose()?;

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
    }
}
