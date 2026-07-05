use pdf_graphics::rect::Rect;
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

use crate::{
    AnnotationBorder, AnnotationColor, AnnotationError, AnnotationKind, AppearanceDictionary,
    OptionalContent,
};

/// A typed page annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    /// The annotation subtype name.
    pub subtype: String,
    /// The required annotation rectangle.
    pub rect: Rect,
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
    /// The original annotation dictionary.
    pub dictionary: Dictionary,
}

impl Annotation {
    /// Reads all page annotations from the optional `/Annots` array.
    pub fn from_page_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Option<Vec<Self>>, AnnotationError> {
        let Some(annots) = dictionary.get("Annots") else {
            return Ok(None);
        };

        let annots = annots.try_array(objects)?;
        let mut annotations = Vec::with_capacity(annots.len());

        for annot in annots {
            let dictionary = annot.try_dictionary(objects)?;
            annotations.push(Self::from_dictionary(dictionary, objects)?);
        }

        Ok(Some(annotations))
    }

    /// Parses a single resolved annotation dictionary.
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, AnnotationError> {
        match dictionary.get_or_err("Type")?.try_str(objects)?.as_ref() {
            "Annot" => {}
            other => {
                return Err(AnnotationError::InvalidEntry {
                    entry: "Type",
                    reason: format!("expected /Annot, found /{other}"),
                });
            }
        }

        let subtype = dictionary
            .get_or_err("Subtype")?
            .try_str(objects)?
            .into_owned();

        let rect = dictionary
            .get_or_err("Rect")?
            .try_array_of::<f32, 4>(objects)
            .map(|arr| {
                let [left, bottom, right, top] = arr;
                Rect {
                    left,
                    bottom,
                    right,
                    top,
                }
            })?;

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
            .map(|value| value.try_bytes_vec(objects))
            .transpose()?;
        let struct_parent = dictionary
            .get("StructParent")
            .map(|value| value.try_number::<usize>(objects))
            .transpose()?;

        let appearance = AppearanceDictionary::from_dictionary(dictionary, objects)?;
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
            dictionary: dictionary.clone(),
        })
    }
}
