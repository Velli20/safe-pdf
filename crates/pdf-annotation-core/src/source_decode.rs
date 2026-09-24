//! Decodes page annotation dictionaries directly into owned Core source data.
//!
//! Parsing is strict for any entry that is present: malformed values fail the
//! annotation. Referenced resources are retained as leaves, never fetched.
use crate::error::SourceDecodeError;
use crate::metadata::AnnotationFlags;
use crate::pdf_data::{
    AnnotationBorder, AppearanceCharacteristics, AppearanceDictionary, BorderEffect,
    BorderEffectStyle, BorderStyle, BorderStyleName, CaretAnnotation, CircleAnnotation,
    FreeTextAnnotation, HighlightAnnotation, InkAnnotation, LineAnnotation, LinkAnnotation,
    NativeAnnotation, PolyLineAnnotation, PolygonAnnotation, PopupAnnotation, SourceAnnotation,
    SquareAnnotation, SquigglyAnnotation, StampAnnotation, StrikeOutAnnotation, TextAnnotation,
    UnderlineAnnotation, WidgetAnnotation,
};
use pdf_graphics::{DashPattern, color::Color, rect::Rect};
use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult,
    object_lookup::ObjectLookupExt, object_resolver::ObjectResolver, object_variant::ObjectVariant,
    pdf_array::PdfArray,
};

pub(crate) type DecodeResult<T> = Result<T, SourceDecodeError>;

impl FromPdfObject for SourceAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let context = context.dictionary()?;
        let dictionary = context.dictionary();
        let objects = context.source();
        // Some PDFs omit `/Type` on annotation dictionaries even though the
        // entry is nominally expected to be `/Annot`, so only validate it when
        // the key is actually present.
        if let Some(annotation_type) = dictionary.optional_bytes(b"Type", objects)? {
            match annotation_type {
                b"Annot" => {}
                other => {
                    return Err(SourceDecodeError::InvalidEntry {
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
                // `/Rect` is `[left, bottom, right, top]`; keep the source
                // edges as-is so inverted rectangles survive decoding.
                value.try_array_of::<f64, 4>(objects).map(|arr| {
                    let [left, bottom, right, top] = arr;
                    Rect {
                        left,
                        top,
                        right,
                        bottom,
                    }
                })
            })
            .transpose()?;

        let kind = NativeAnnotation::from_dictionary(&subtype, dictionary, objects)?;

        let contents = dictionary.optional_bytes_vec(b"Contents", objects)?;
        let name = dictionary.optional_bytes_vec(b"NM", objects)?;
        let flags = dictionary
            .optional_number::<i32>(b"F", objects)?
            .map(AnnotationFlags::from_bits_retain);
        let appearance_state = dictionary
            .get(b"AS")
            .map(|value| value.try_bytes(objects).map(Vec::from))
            .transpose()?;
        let struct_parent = dictionary.optional_number::<u64>(b"StructParent", objects)?;
        let optional_content =
            crate::source_decode_optional_content::optional_content(dictionary, objects)?;

        let border = AnnotationBorder::from_dictionary(dictionary, objects)?;
        let color = color(dictionary, b"C", objects)?;

        let opacity = dictionary
            .optional_number::<f64>(b"CA", objects)?
            .filter(|v| v.is_finite())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let appearance = AppearanceDictionary::from_dictionary(dictionary, objects)?;

        Ok(Self {
            id: 0,
            object_number: None,
            opacity,
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

impl SourceAnnotation {
    /// Read page annotations and assign stable page-local identifiers.
    pub fn from_page_dictionary<A: ObjectAccess + ?Sized>(
        context: &mut DictionaryContext<'_, A>,
    ) -> ReadResult<Option<Vec<Self>>> {
        let Some(annots) = context.optional::<PdfArray>(b"Annots")? else {
            return Ok(None);
        };
        let mut annotations = Vec::with_capacity(annots.len());
        for value in annots.iter() {
            if value.is_null(context.source())? {
                continue;
            }

            let dictionary = value.try_dictionary(context.source())?;
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
            let mut annotation: Self = context.read(value)?;
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
        Ok(Some(annotations))
    }
}

impl FromPdfObject for NativeAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(SourceAnnotation::from_pdf_object(context)?.kind)
    }
}

macro_rules! native_subtype_decoder {
    ($type:ident, $variant:ident) => {
        impl FromPdfObject for $type {
            fn from_pdf_object(
                context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
            ) -> ReadResult<Self> {
                match SourceAnnotation::from_pdf_object(context)?.kind {
                    NativeAnnotation::$variant(value) => Ok(*value),
                    _ => Err(SourceDecodeError::InvalidEntry {
                        entry: b"Subtype",
                        reason: concat!("expected /", stringify!($variant), " annotation").into(),
                    }
                    .into()),
                }
            }
        }
    };
}

native_subtype_decoder!(TextAnnotation, Text);
native_subtype_decoder!(LinkAnnotation, Link);
native_subtype_decoder!(FreeTextAnnotation, FreeText);
native_subtype_decoder!(LineAnnotation, Line);
native_subtype_decoder!(SquareAnnotation, Square);
native_subtype_decoder!(CircleAnnotation, Circle);
native_subtype_decoder!(PolygonAnnotation, Polygon);
native_subtype_decoder!(PolyLineAnnotation, PolyLine);
native_subtype_decoder!(HighlightAnnotation, Highlight);
native_subtype_decoder!(UnderlineAnnotation, Underline);
native_subtype_decoder!(SquigglyAnnotation, Squiggly);
native_subtype_decoder!(StrikeOutAnnotation, StrikeOut);
native_subtype_decoder!(StampAnnotation, Stamp);
native_subtype_decoder!(CaretAnnotation, Caret);
native_subtype_decoder!(InkAnnotation, Ink);
native_subtype_decoder!(PopupAnnotation, Popup);
native_subtype_decoder!(WidgetAnnotation, Widget);

impl AppearanceDictionary {
    /// Reads the normal-state names from `/AP`; drawing streams are never decoded.
    pub(crate) fn from_dictionary(
        dictionary: &pdf_object_reader::dictionary::Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        let Some(appearance) = dictionary.optional_dictionary(b"AP", objects)? else {
            return Ok(None);
        };
        let Some(normal) = appearance.get(b"N") else {
            return Ok(Some(Self {
                normal_states: Vec::new(),
            }));
        };
        let states = match objects.resolve_object(normal)? {
            ObjectVariant::Dictionary(states) => states,
            ObjectVariant::Stream(_) | ObjectVariant::Null => {
                return Ok(Some(Self {
                    normal_states: Vec::new(),
                }));
            }
            _ => {
                return Err(SourceDecodeError::InvalidEntry {
                    entry: b"N",
                    reason: "expected a drawing stream or state dictionary".into(),
                });
            }
        };
        let mut normal_states = Vec::with_capacity(states.len());
        for (name, value) in states.iter() {
            if !matches!(objects.resolve_object(value)?, ObjectVariant::Stream(_)) {
                return Err(SourceDecodeError::InvalidEntry {
                    entry: b"N",
                    reason: "expected a stream for each normal state".into(),
                });
            }
            normal_states.push(name.to_vec());
        }
        Ok(Some(Self { normal_states }))
    }
}

impl AppearanceCharacteristics {
    pub(crate) fn from_dictionary(
        dictionary: &pdf_object_reader::dictionary::Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        let Some(value) = dictionary.get(b"MK") else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let rotation = dictionary.optional_number::<i32>(b"R", objects)?;
        let border_color = color(dictionary, b"BC", objects)?;
        let background_color = color(dictionary, b"BG", objects)?;
        let normal_caption = dictionary.optional_bytes_vec(b"CA", objects)?;
        let rollover_caption = dictionary.optional_bytes_vec(b"RC", objects)?;
        let alternate_caption = dictionary.optional_bytes_vec(b"AC", objects)?;

        Ok(Some(Self {
            rotation,
            border_color,
            background_color,
            normal_caption,
            rollover_caption,
            alternate_caption,
        }))
    }
}

impl AnnotationBorder {
    pub(crate) fn from_dictionary(
        dictionary: &pdf_object_reader::dictionary::Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        let Some(value) = dictionary.optional_array(b"Border", objects)? else {
            return Ok(None);
        };

        let [horizontal_radius, vertical_radius, width, rest @ ..] = value.as_slice() else {
            return Err(SourceDecodeError::InvalidEntry {
                entry: b"Border",
                reason: "expected an array with at least 3 numbers".to_owned(),
            });
        };

        let horizontal_radius = horizontal_radius.try_number::<f64>(objects)?;
        let vertical_radius = vertical_radius.try_number::<f64>(objects)?;
        let width = width.try_number::<f64>(objects)?;

        // The dash array is nominally nested, but some writers flatten it into the
        // `/Border` array itself.
        let intervals = match rest {
            [] => Vec::new(),
            [first, ..] => match first.try_array(objects) {
                Ok(dash_array) => dash_array
                    .iter()
                    .map(|value| value.try_number::<f32>(objects))
                    .collect::<Result<Vec<_>, _>>()?,
                Err(_) => rest
                    .iter()
                    .map(|value| value.try_number::<f32>(objects))
                    .collect::<Result<Vec<_>, _>>()?,
            },
        };
        // `/Border` carries no dash phase.
        let dash_pattern =
            DashPattern::new(&intervals, 0.0).map_err(|error| SourceDecodeError::InvalidEntry {
                entry: b"Border",
                reason: error.to_string(),
            })?;

        Ok(Some(Self {
            horizontal_radius,
            vertical_radius,
            width,
            dash_pattern,
        }))
    }
}

/// Decodes an optional device color array (gray, RGB, or CMYK) into sRGB.
///
/// An empty array means transparent per the PDF specification and is retained
/// with alpha `0.0`. Nonfinite components and unsupported component counts fail
/// the entry.
pub(crate) fn color(
    dictionary: &pdf_object_reader::dictionary::Dictionary,
    key: &'static [u8],
    objects: &dyn ObjectResolver,
) -> DecodeResult<Option<Color>> {
    let Some(value) = dictionary.get(key) else {
        return Ok(None);
    };
    let components = value.try_vec_of::<f32>(objects)?;
    if components.iter().any(|v| !v.is_finite()) {
        return Err(SourceDecodeError::InvalidEntry {
            entry: key,
            reason: "color components must be finite".into(),
        });
    }
    if components.is_empty() {
        return Ok(Some(Color::from_rgba(0.0, 0.0, 0.0, 0.0)));
    }
    Color::from_device_components(&components)
        .map(Some)
        .ok_or_else(|| SourceDecodeError::InvalidEntry {
            entry: key,
            reason: format!(
                "expected one, three, or four color components, found {}",
                components.len()
            ),
        })
}

impl BorderStyle {
    pub(crate) fn from_dictionary(
        dictionary: &pdf_object_reader::dictionary::Dictionary,
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        let Some(value) = dictionary.get(key) else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let width = dictionary.optional_number::<f64>(b"W", objects)?;
        let style = dictionary
            .get(b"S")
            .map(|value| value.try_bytes(objects).map(BorderStyleName::from))
            .transpose()?;
        let dash_pattern = dictionary.optional_vec_of::<f64>(b"D", objects)?;

        Ok(Some(Self {
            width,
            style,
            dash_pattern,
        }))
    }
}

impl BorderEffect {
    pub(crate) fn from_dictionary(
        dictionary: &pdf_object_reader::dictionary::Dictionary,
        key: &'static [u8],
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        let Some(value) = dictionary.get(key) else {
            return Ok(None);
        };

        let dictionary = value.try_dictionary(objects)?;
        let style = dictionary
            .get(b"S")
            .map(|value| value.try_bytes(objects).map(BorderEffectStyle::from))
            .transpose()?;
        let intensity = dictionary.optional_number::<f64>(b"I", objects)?;

        Ok(Some(Self { style, intensity }))
    }
}
