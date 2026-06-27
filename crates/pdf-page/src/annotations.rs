//! Reads page annotation dictionaries from the `/Annots` entry.
//!
//! This module parses annotation dictionaries and preserves the resolved
//! dictionary for future consumers. It does not render appearance streams,
//! execute annotation actions, or interpret subtype-specific behavior.

use crate::error::PdfPagesError;
use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

/// A rectangular annotation boundary in PDF user space.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationRect {
    /// The left edge of the rectangle.
    pub left: f32,
    /// The bottom edge of the rectangle.
    pub bottom: f32,
    /// The right edge of the rectangle.
    pub right: f32,
    /// The top edge of the rectangle.
    pub top: f32,
}

impl AnnotationRect {
    /// Parses a rectangle from a resolved `/Rect` object.
    ///
    /// The PDF rectangle array is interpreted as `[left, bottom, right, top]`.
    ///
    /// # Parameters
    ///
    /// - `object`: The `/Rect` value to parse.
    /// - `objects`: Object resolver used to follow indirect references.
    ///
    /// # Returns
    ///
    /// Returns the parsed [`AnnotationRect`] when the object resolves to an
    /// array of four numbers.
    ///
    /// # Errors
    ///
    /// Returns [`PdfPagesError::Object`] when the object is missing,
    /// unresolved, or not an array of four numeric values.
    pub fn from_object(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfPagesError> {
        let [left, bottom, right, top] = object.try_array_of::<f32, 4>(objects)?;

        Ok(Self {
            left,
            bottom,
            right,
            top,
        })
    }
}

/// A resolved page annotation dictionary with the common PDF annotation fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// The annotation subtype from `/Subtype`, if present.
    pub subtype: Option<String>,
    /// The annotation rectangle from `/Rect`, if present.
    pub rect: Option<AnnotationRect>,
    /// The annotation contents from `/Contents`, if present.
    pub contents: Option<Vec<u8>>,
    /// The annotation flags from `/F`, if present.
    pub flags: Option<i32>,
    /// The annotation name from `/NM`, if present.
    pub name: Option<String>,
    /// The modification timestamp from `/M`, if present.
    pub modified: Option<Vec<u8>>,
    /// The resolved annotation dictionary, preserving all entries.
    pub dictionary: Dictionary,
}

impl Annotation {
    /// Parses a single resolved annotation dictionary.
    ///
    /// Common annotation fields are extracted into typed fields while the
    /// complete dictionary is preserved for downstream consumers.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The resolved annotation dictionary.
    /// - `objects`: Object resolver used to resolve indirect field values.
    ///
    /// # Returns
    ///
    /// Returns a materialized [`Annotation`] with common fields parsed.
    ///
    /// # Errors
    ///
    /// Returns [`PdfPagesError::Object`] when any supported field has an
    /// invalid type or malformed array structure.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfPagesError> {
        let subtype = dictionary
            .get("Subtype")
            .map(|value| value.try_str(objects).map(|name| name.into_owned()))
            .transpose()?;
        let rect = dictionary
            .get("Rect")
            .map(|value| AnnotationRect::from_object(value, objects))
            .transpose()?;
        let contents = dictionary
            .get("Contents")
            .map(|value| value.try_bytes(objects).map(|bytes| bytes.to_vec()))
            .transpose()?;
        let flags = dictionary
            .get("F")
            .map(|value| value.try_number::<i32>(objects))
            .transpose()?;
        let name = dictionary
            .get("NM")
            .map(|value| value.try_str(objects).map(|name| name.into_owned()))
            .transpose()?;
        let modified = dictionary
            .get("M")
            .map(|value| value.try_bytes(objects).map(|bytes| bytes.to_vec()))
            .transpose()?;

        Ok(Self {
            subtype,
            rect,
            contents,
            flags,
            name,
            modified,
            dictionary: dictionary.clone(),
        })
    }
}

/// Reads the `/Annots` entry from a page dictionary.
pub struct Annotations;

impl Annotations {
    /// Reads all page annotations from the optional `/Annots` array.
    ///
    /// Missing `/Annots` returns an empty vector. When present, the `/Annots`
    /// entry must resolve to an array, and each array entry must resolve to a
    /// dictionary. Indirect arrays and indirect annotation dictionaries are
    /// both supported.
    ///
    /// # Parameters
    ///
    /// - `dictionary`: The page dictionary that may contain `/Annots`.
    /// - `objects`: Object resolver used to follow indirect references.
    ///
    /// # Returns
    ///
    /// Returns a vector of parsed [`Annotation`] values. The vector is empty
    /// when `/Annots` is absent.
    ///
    /// # Errors
    ///
    /// Returns [`PdfPagesError::Object`] when `/Annots` is malformed or when
    /// an array entry does not resolve to a dictionary.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<Annotation>, PdfPagesError> {
        const KEY: &str = "Annots";

        let Some(annots) = dictionary.get(KEY) else {
            return Ok(Vec::new());
        };

        let annots = annots.try_array(objects)?;
        let mut annotations = Vec::with_capacity(annots.len());

        for annot in annots {
            let resolved = objects.resolve_object(annot)?;
            let resolved_dictionary = match resolved {
                ObjectVariant::Dictionary(dict) => dict.as_ref(),
                other => {
                    return Err(ObjectError::TypeMismatch("Dictionary", other.name()).into());
                }
            };

            annotations.push(Annotation::from_dictionary(resolved_dictionary, objects)?);
        }

        Ok(annotations)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::error::PdfPagesError;
    use pdf_object::{
        dictionary::Dictionary, error::ObjectError, object_resolver::ObjectResolver,
        object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use super::{AnnotationRect, Annotations};

    struct MapResolver {
        objects: BTreeMap<usize, ObjectVariant>,
    }

    impl ObjectResolver for MapResolver {
        fn resolve_object<'a>(
            &'a self,
            obj: &'a ObjectVariant,
        ) -> Result<&'a ObjectVariant, ObjectError> {
            match obj {
                ObjectVariant::Reference(object_number) => self.objects.get(object_number).ok_or(
                    ObjectError::FailedResolveObjectReference {
                        obj_num: *object_number,
                    },
                ),
                _ => Ok(obj),
            }
        }
    }

    fn annotation_dictionary() -> Dictionary {
        Dictionary::new(BTreeMap::from([
            ("Subtype".to_owned(), ObjectVariant::Name(b"Text".to_vec())),
            (
                "Rect".to_owned(),
                ObjectVariant::Array(vec![
                    ObjectVariant::Real(1.0),
                    ObjectVariant::Real(2.0),
                    ObjectVariant::Real(3.0),
                    ObjectVariant::Real(4.0),
                ]),
            ),
            (
                "Contents".to_owned(),
                ObjectVariant::LiteralString(b"Note".to_vec()),
            ),
            ("F".to_owned(), ObjectVariant::Integer(4)),
            ("NM".to_owned(), ObjectVariant::Name(b"anno-1".to_vec())),
            (
                "M".to_owned(),
                ObjectVariant::LiteralString(b"D:20240627120000".to_vec()),
            ),
            (
                "Custom".to_owned(),
                ObjectVariant::LiteralString(b"preserve-me".to_vec()),
            ),
        ]))
    }

    #[test]
    fn missing_annots_returns_empty_vector() {
        let page = Dictionary::new(BTreeMap::new());

        let annotations = Annotations::from_dictionary(&page, &PassthroughResolver)
            .expect("missing annots should parse");

        assert!(annotations.is_empty());
    }

    #[test]
    fn inline_annotation_dictionary_parses_common_fields_and_preserves_dictionary() {
        let page = Dictionary::new(BTreeMap::from([(
            "Annots".to_owned(),
            ObjectVariant::Array(vec![ObjectVariant::Dictionary(Box::new(
                annotation_dictionary(),
            ))]),
        )]));

        let annotations = Annotations::from_dictionary(&page, &PassthroughResolver)
            .expect("inline annotation should parse");

        assert_eq!(annotations.len(), 1);

        let annotation = &annotations[0];
        assert_eq!(annotation.subtype.as_deref(), Some("Text"));
        assert_eq!(
            annotation.rect.as_ref(),
            Some(&AnnotationRect {
                left: 1.0,
                bottom: 2.0,
                right: 3.0,
                top: 4.0,
            })
        );
        assert_eq!(annotation.contents.as_deref(), Some(b"Note".as_ref()));
        assert_eq!(annotation.flags, Some(4));
        assert_eq!(annotation.name.as_deref(), Some("anno-1"));
        assert_eq!(
            annotation.modified.as_deref(),
            Some(b"D:20240627120000".as_ref())
        );
        assert_eq!(annotation.dictionary, annotation_dictionary());
    }

    #[test]
    fn indirect_annots_array_and_indirect_annotation_dictionary_resolve_correctly() {
        let annot_dict = annotation_dictionary();
        let page = Dictionary::new(BTreeMap::from([(
            "Annots".to_owned(),
            ObjectVariant::Reference(1),
        )]));

        let resolver = MapResolver {
            objects: BTreeMap::from([
                (1, ObjectVariant::Array(vec![ObjectVariant::Reference(2)])),
                (2, ObjectVariant::Dictionary(Box::new(annot_dict.clone()))),
            ]),
        };

        let annotations = Annotations::from_dictionary(&page, &resolver)
            .expect("indirect annotation should parse");

        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].dictionary, annot_dict);
    }

    #[test]
    fn invalid_rect_length_returns_error() {
        let page = Dictionary::new(BTreeMap::from([(
            "Annots".to_owned(),
            ObjectVariant::Array(vec![ObjectVariant::Dictionary(Box::new(Dictionary::new(
                BTreeMap::from([(
                    "Rect".to_owned(),
                    ObjectVariant::Array(vec![
                        ObjectVariant::Integer(1),
                        ObjectVariant::Integer(2),
                        ObjectVariant::Integer(3),
                    ]),
                )]),
            )))]),
        )]));

        let err = Annotations::from_dictionary(&page, &PassthroughResolver)
            .expect_err("invalid rect length should fail");

        assert!(matches!(
            err,
            PdfPagesError::Object(ObjectError::InvalidArrayLength {
                expected: 4,
                found: 3
            })
        ));
    }

    #[test]
    fn non_dictionary_annotation_array_entry_returns_error() {
        let page = Dictionary::new(BTreeMap::from([(
            "Annots".to_owned(),
            ObjectVariant::Array(vec![ObjectVariant::Integer(7)]),
        )]));

        let err = Annotations::from_dictionary(&page, &PassthroughResolver)
            .expect_err("non-dictionary annotation entry should fail");

        assert!(matches!(
            err,
            PdfPagesError::Object(ObjectError::TypeMismatch("Dictionary", "Integer"))
        ));
    }
}
