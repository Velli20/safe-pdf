//! Decodes page annotation dictionaries directly into owned Core source data.
//!
//! Parsing is strict for any entry that is present: malformed values fail the
//! annotation. Referenced resources are retained as leaves, never fetched.
use crate::error::SourceDecodeError;
use crate::metadata::AnnotationFlags;
use crate::pdf_data::{
    AnnotationBorder, AppearanceCharacteristics, AppearanceDictionary, AppearanceState,
    AppearanceStreams, BorderEffect, BorderEffectStyle, BorderStyle, BorderStyleName,
    NativeAnnotation, PageAnnotations, SourceAnnotation,
};
use pdf_graphics::{AlphaConstant, DashPattern};
use pdf_object_reader::pdf_name::PdfName;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_kind::ObjectKind,
    object_variant::ObjectVariant, pdf_object::PdfObject, stream::StreamObject,
};
use pdf_optional_content::OptionalContentMembership;

pub(crate) type DecodeResult<T> = Result<T, SourceDecodeError>;

impl FromPdfObject for SourceAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut dictionary = context.dictionary()?;
        // Some PDFs omit `/Type` on annotation dictionaries even though the
        // entry is nominally expected to be `/Annot`, so only validate it when
        // the key is actually present.
        if let Some(annotation_type) = dictionary.optional::<PdfName>(b"Type")?
            && annotation_type.as_ref() != b"Annot"
        {
            return Err(SourceDecodeError::InvalidEntry {
                entry: b"Type",
                reason: format!("expected /Annot, found {annotation_type:?}"),
            }
            .into());
        }

        Ok(Self {
            id: 0,
            object_number: None,
            opacity: dictionary
                .optional::<AlphaConstant>(b"CA")?
                .unwrap_or_default()
                .value(),
            // `/Subtype` identifies the concrete annotation kind.
            subtype: dictionary.required(b"Subtype")?,
            // The source edges are kept as-is so inverted rectangles survive decoding.
            rect: dictionary.optional(b"Rect")?,
            // `/Contents` is descriptive text only; like PDFium, a malformed value (e.g. an
            // integer) is treated as absent rather than rejecting the annotation.
            contents: dictionary.lenient(b"Contents"),
            name: dictionary.optional(b"NM")?,
            flags: dictionary
                .optional::<i32>(b"F")?
                .map(AnnotationFlags::from_bits_retain),
            appearance: dictionary.optional(b"AP")?,
            appearance_state: dictionary.optional(b"AS")?,
            border: dictionary.optional(b"Border")?,
            color: dictionary.optional(b"C")?,
            struct_parent: dictionary.optional(b"StructParent")?,
            optional_content: dictionary.optional::<OptionalContentMembership>(b"OC")?,
            kind: dictionary.read(object)?,
        })
    }
}

/// Reads a page's `/Annots` array, assigning stable page-local identifiers.
///
/// `/Annots` that resolves to anything but an array (e.g. a reference whose object
/// number was reused by a later update) holds no annotations.
impl FromPdfObject for PageAnnotations {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let ObjectVariant::Array(_) = context.object() else {
            return Ok(Self(None));
        };
        let mut annots = context.array()?;
        let mut annotations = Vec::with_capacity(annots.array().len());
        for value in annots.array() {
            // Stale entries in `/Annots` resolve to null (ISO 32000 §7.3.10).
            let resolved = annots.resolve(value)?;
            if resolved.kind() == ObjectKind::Null {
                continue;
            }

            let dictionary = resolved.value().dictionary()?;
            if dictionary.get(b"Subtype").is_none() {
                continue;
            }
            let object_number = match value {
                ObjectVariant::Reference(id) => Some(id.number),
                _ => dictionary.object_number,
            };
            let defines_field = dictionary.get(b"T").is_some();
            let mut annotation: SourceAnnotation = annots.read(value)?;
            // A merged field/widget with its own partial name is a terminal
            // field. Its Parent may be a nonterminal name-tree node, not the
            // owner of a shared value; grouping by that parent merges siblings.
            if defines_field && let NativeAnnotation::Widget(widget) = &mut annotation.kind {
                widget.field_id = object_number;
            }
            annotation.id =
                u64::try_from(annotations.len()).map_err(|_| SourceDecodeError::ResourceLimit)?;
            annotation.object_number = object_number;
            annotations.push(annotation);
        }
        Ok(Self(Some(annotations)))
    }
}

/// Reads an `/AP` appearance dictionary.
impl FromPdfObject for AppearanceDictionary {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut appearance = context.dictionary()?;
        Ok(Self {
            normal: appearance.optional(b"N")?,
            // A malformed down appearance only loses the pressed look.
            down: appearance.lenient(b"D"),
        })
    }
}

/// Reads an `/AP` entry: one stream, or a dictionary of state streams with null states skipped.
impl FromPdfObject for AppearanceStreams {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let ObjectVariant::Dictionary(_) = context.object() else {
            let stream = context.decode::<StreamObject>()?;
            return Ok(Self::Stream(PdfObject::new(ObjectVariant::Stream(stream))));
        };
        let mut states = context.dictionary()?;
        states
            .keys()
            .filter_map(|name| {
                let stream = states.optional::<StreamObject>(name).transpose()?;
                Some(stream.map(|stream| AppearanceState {
                    name: PdfName::from_bytes(name),
                    stream: PdfObject::new(ObjectVariant::Stream(stream)),
                }))
            })
            .collect::<ReadResult<_>>()
            .map(Self::States)
    }
}

/// Reads a widget's `/MK` appearance characteristics dictionary.
impl FromPdfObject for AppearanceCharacteristics {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            rotation: dictionary.optional(b"R")?,
            border_color: dictionary.optional(b"BC")?,
            background_color: dictionary.optional(b"BG")?,
            normal_caption: dictionary.optional(b"CA")?,
            rollover_caption: dictionary.optional(b"RC")?,
            alternate_caption: dictionary.optional(b"AC")?,
        })
    }
}

/// Reads a `/Border` array `[horizontal_radius vertical_radius width dash_array?]`. Some
/// writers flatten the dash array into the `/Border` array itself.
impl FromPdfObject for AnnotationBorder {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut array = context.array()?;
        let [horizontal_radius, vertical_radius, width] = array.read_array()?;
        // `/Border` carries no dash phase.
        let dash_pattern =
            DashPattern::new(array.read_flattened_from(3)?, 0.0).map_err(|error| {
                SourceDecodeError::InvalidEntry {
                    entry: b"Border",
                    reason: error.to_string(),
                }
            })?;
        Ok(Self {
            horizontal_radius,
            vertical_radius,
            width,
            dash_pattern,
        })
    }
}

/// Reads a `/BS` border style dictionary.
impl FromPdfObject for BorderStyle {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            width: dictionary.optional(b"W")?,
            style: dictionary.optional(b"S")?,
            dash_pattern: dictionary.optional(b"D")?,
        })
    }
}

/// Reads a `/BE` border effect dictionary.
impl FromPdfObject for BorderEffect {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            style: dictionary.optional(b"S")?,
            intensity: dictionary.optional(b"I")?,
        })
    }
}

/// Reads a `/BS` `/S` border style name, preserving unknown names.
impl FromPdfObject for BorderStyleName {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let name = context.decode::<PdfName>()?;
        Ok(match &*name {
            b"S" => Self::Solid,
            b"D" => Self::Dashed,
            b"B" => Self::Beveled,
            b"I" => Self::Inset,
            b"U" => Self::Underline,
            other => Self::Unknown(Vec::from(other)),
        })
    }
}

/// Reads a `/BE` `/S` border effect name, preserving unknown names.
impl FromPdfObject for BorderEffectStyle {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let name = context.decode::<PdfName>()?;
        Ok(match &*name {
            b"S" => Self::None,
            b"C" => Self::Cloudy,
            other => Self::Unknown(Vec::from(other)),
        })
    }
}
