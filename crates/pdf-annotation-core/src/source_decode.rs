//! Decodes page annotation dictionaries directly into owned Core source data.
//!
//! Parsing is strict for any entry that is present: malformed values fail the
//! annotation. Referenced resources are retained as leaves, never fetched.
use std::sync::Arc;

use crate::error::SourceDecodeError;
use crate::metadata::AnnotationFlags;
use crate::pdf_data::{
    AnnotationBorder, AppearanceCharacteristics, AppearanceDictionary, BorderEffect,
    BorderEffectStyle, BorderStyle, BorderStyleName, NativeAnnotation, PageAnnotations,
    SourceAnnotation,
};
use pdf_graphics::{DashPattern, rect::Rect};
use pdf_object_reader::{
    ArrayContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_kind::ObjectKind,
    object_variant::ObjectVariant,
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
        if let Some(annotation_type) = dictionary.optional::<Arc<[u8]>>(b"Type")?
            && annotation_type.as_ref() != b"Annot"
        {
            return Err(SourceDecodeError::InvalidEntry {
                entry: b"Type",
                reason: format!("expected /Annot, found /{annotation_type:?}"),
            }
            .into());
        }

        Ok(Self {
            id: 0,
            object_number: None,
            opacity: dictionary
                .optional::<f64>(b"CA")?
                .filter(|v| v.is_finite())
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
            // `/Subtype` identifies the concrete annotation kind.
            subtype: dictionary.required::<Arc<[u8]>>(b"Subtype")?.to_vec(),
            // `/Rect` is `[left, bottom, right, top]`; keep the source edges
            // as-is so inverted rectangles survive decoding.
            rect: dictionary
                .optional::<[f32; 4]>(b"Rect")?
                .map(|[left, bottom, right, top]| Rect {
                    left,
                    top,
                    right,
                    bottom,
                }),
            // `/Contents` is descriptive text only; like PDFium, a malformed value (e.g. an
            // integer) is treated as absent rather than rejecting the annotation.
            contents: dictionary
                .lenient::<Arc<[u8]>>(b"Contents")
                .map(|bytes| bytes.to_vec()),
            name: dictionary
                .optional::<Arc<[u8]>>(b"NM")?
                .map(|bytes| bytes.to_vec()),
            flags: dictionary
                .optional::<i32>(b"F")?
                .map(AnnotationFlags::from_bits_retain),
            appearance: dictionary.optional(b"AP")?,
            appearance_state: dictionary
                .optional::<Arc<[u8]>>(b"AS")?
                .map(|bytes| bytes.to_vec()),
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

            let dictionary = resolved.dictionary()?;
            if dictionary.get(b"Subtype").is_none() {
                continue;
            }
            let object_number = match value {
                ObjectVariant::Reference(id) => u64::try_from(id.number).ok(),
                _ => dictionary
                    .object_number
                    .and_then(|number| u64::try_from(number).ok()),
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

/// Reads `/AP` normal-state names; drawing streams are never decoded.
impl FromPdfObject for AppearanceDictionary {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut appearance = context.dictionary()?;
        let Some(normal) = appearance.resolve_entry(b"N")? else {
            return Ok(Self {
                normal_states: Vec::new(),
            });
        };
        let states = match normal.value() {
            ObjectVariant::Dictionary(states) => states,
            ObjectVariant::Stream(_) | ObjectVariant::Null => {
                return Ok(Self {
                    normal_states: Vec::new(),
                });
            }
            _ => {
                return Err(SourceDecodeError::InvalidEntry {
                    entry: b"N",
                    reason: "expected a drawing stream or state dictionary".into(),
                }
                .into());
            }
        };
        let mut normal_states = Vec::with_capacity(states.len());
        for (name, value) in states.iter() {
            match appearance.resolve(value)?.kind() {
                ObjectKind::Null => continue,
                ObjectKind::Stream => normal_states.push(name.to_vec()),
                _ => {
                    return Err(SourceDecodeError::InvalidEntry {
                        entry: b"N",
                        reason: "expected a stream for each normal state".into(),
                    }
                    .into());
                }
            }
        }
        Ok(Self { normal_states })
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
            normal_caption: dictionary
                .optional::<Arc<[u8]>>(b"CA")?
                .map(|bytes| bytes.to_vec()),
            rollover_caption: dictionary
                .optional::<Arc<[u8]>>(b"RC")?
                .map(|bytes| bytes.to_vec()),
            alternate_caption: dictionary
                .optional::<Arc<[u8]>>(b"AC")?
                .map(|bytes| bytes.to_vec()),
        })
    }
}

/// Reads a `/Border` array `[horizontal_radius vertical_radius width dash_array?]`.
impl FromPdfObject for AnnotationBorder {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut array = context.array()?;
        let [_, _, _, rest @ ..] = array.array() else {
            return Err(SourceDecodeError::InvalidEntry {
                entry: b"Border",
                reason: "expected an array with at least 3 numbers".to_owned(),
            }
            .into());
        };
        let [horizontal_radius, vertical_radius, width] = array.read_array::<f32, 3>()?;

        // The dash array is nominally nested, but some writers flatten it into the
        // `/Border` array itself.
        let intervals: Vec<f32> = match rest.first() {
            None => Vec::new(),
            Some(first) if array.resolve(first)?.kind() == ObjectKind::Array => {
                array.read(first)?
            }
            Some(_) => ArrayContext::new(rest, array.access()).read_all()?,
        };
        // `/Border` carries no dash phase.
        let dash_pattern =
            DashPattern::new(&intervals, 0.0).map_err(|error| SourceDecodeError::InvalidEntry {
                entry: b"Border",
                reason: error.to_string(),
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
        let name = context.decode::<Arc<[u8]>>()?;
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
        let name = context.decode::<Arc<[u8]>>()?;
        Ok(match &*name {
            b"S" => Self::None,
            b"C" => Self::Cloudy,
            other => Self::Unknown(Vec::from(other)),
        })
    }
}
