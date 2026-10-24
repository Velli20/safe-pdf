//! Subtype-specific payload decoding for the `/Subtype` vocabulary Core retains.
use std::sync::Arc;

use crate::error::SourceDecodeError;
use crate::models::{Point, Quad};
use crate::pdf_data::{
    CaretAnnotation, CaretSymbolStyle, CircleAnnotation, FreeTextAnnotation, HighlightAnnotation,
    InkAnnotation, InkList, LineAnnotation, LineEndingStyle, LinkAnnotation, LinkHighlightMode,
    NativeAnnotation, PolyLineAnnotation, PolygonAnnotation, PopupAnnotation, SquareAnnotation,
    SquigglyAnnotation, StampAnnotation, StrikeOutAnnotation, TextAnnotation, UnderlineAnnotation,
};
use crate::source_decode::DecodeResult;
use pdf_graphics::polyline::Polyline;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

/// Dispatches on `/Subtype`; unrecognized names retain their bytes unparsed.
impl FromPdfObject for NativeAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut dictionary = context.dictionary()?;
        let subtype = dictionary.required::<Arc<[u8]>>(b"Subtype")?;
        Ok(match subtype.as_ref() {
            b"Text" => Self::Text(Box::new(dictionary.read(object)?)),
            b"Link" => Self::Link(Box::new(dictionary.read(object)?)),
            b"FreeText" => Self::FreeText(Box::new(dictionary.read(object)?)),
            b"Line" => Self::Line(Box::new(dictionary.read(object)?)),
            b"Square" => Self::Square(Box::new(dictionary.read(object)?)),
            b"Circle" => Self::Circle(Box::new(dictionary.read(object)?)),
            b"Polygon" => Self::Polygon(Box::new(dictionary.read(object)?)),
            b"PolyLine" => Self::PolyLine(Box::new(dictionary.read(object)?)),
            b"Highlight" => Self::Highlight(Box::new(dictionary.read(object)?)),
            b"Underline" => Self::Underline(Box::new(dictionary.read(object)?)),
            b"Squiggly" => Self::Squiggly(Box::new(dictionary.read(object)?)),
            b"StrikeOut" => Self::StrikeOut(Box::new(dictionary.read(object)?)),
            b"Stamp" => Self::Stamp(Box::new(dictionary.read(object)?)),
            b"Caret" => Self::Caret(Box::new(dictionary.read(object)?)),
            b"Ink" => Self::Ink(Box::new(dictionary.read(object)?)),
            b"Popup" => Self::Popup(Box::new(dictionary.read(object)?)),
            b"Widget" => Self::Widget(Box::new(dictionary.read(object)?)),
            _ => Self::Unknown {
                subtype: subtype.to_vec(),
            },
        })
    }
}

impl FromPdfObject for TextAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            open: dictionary.optional(b"Open")?,
            name: dictionary
                .optional::<Arc<[u8]>>(b"Name")?
                .map(|bytes| bytes.to_vec()),
            state: dictionary
                .optional::<Arc<[u8]>>(b"State")?
                .map(|bytes| bytes.to_vec()),
            state_model: dictionary
                .optional::<Arc<[u8]>>(b"StateModel")?
                .map(|bytes| bytes.to_vec()),
            intent: dictionary
                .optional::<Arc<[u8]>>(b"IT")?
                .map(|bytes| bytes.to_vec()),
        })
    }
}

impl FromPdfObject for LinkAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            highlight_mode: dictionary.optional(b"H")?,
            destination: dictionary.optional(b"Dest")?,
            action: dictionary.optional(b"A")?,
            quad_points: dictionary
                .optional(b"QuadPoints")?
                .map(quad_points)
                .transpose()?,
            border_style: dictionary.optional(b"BS")?,
            border_effect: dictionary.optional(b"BE")?,
        })
    }
}

impl FromPdfObject for FreeTextAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            default_appearance: dictionary
                .optional::<Arc<[u8]>>(b"DA")?
                .map(|bytes| bytes.to_vec()),
            quadding: dictionary.optional(b"Q")?,
            rich_contents: dictionary
                .optional::<Arc<[u8]>>(b"RC")?
                .map(|bytes| bytes.to_vec()),
            default_style: dictionary
                .optional::<Arc<[u8]>>(b"DS")?
                .map(|bytes| bytes.to_vec()),
            callout_line: dictionary.optional(b"CL")?.map(callout_line).transpose()?,
            border_effect: dictionary.optional(b"BE")?,
            difference_rect: dictionary.lenient(b"RD"),
            intent: dictionary
                .optional::<Arc<[u8]>>(b"IT")?
                .map(|bytes| bytes.to_vec()),
        })
    }
}

impl FromPdfObject for LineAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            line: dictionary.required(b"L")?,
            line_endings: dictionary.optional(b"LE")?,
            border_style: dictionary.optional(b"BS")?,
            interior_color: dictionary.optional(b"IC")?,
            leader_line_length: dictionary.optional(b"LL")?,
            leader_line_extension: dictionary.optional(b"LLE")?,
            caption: dictionary.optional(b"Cap")?,
            intent: dictionary
                .optional::<Arc<[u8]>>(b"IT")?
                .map(|bytes| bytes.to_vec()),
        })
    }
}

impl FromPdfObject for SquareAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            border_style: dictionary.optional(b"BS")?,
            interior_color: dictionary.optional(b"IC")?,
            border_effect: dictionary.optional(b"BE")?,
            difference_rect: dictionary.lenient(b"RD"),
        })
    }
}

impl FromPdfObject for CircleAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            border_style: dictionary.optional(b"BS")?,
            interior_color: dictionary.optional(b"IC")?,
            border_effect: dictionary.optional(b"BE")?,
            difference_rect: dictionary.lenient(b"RD"),
        })
    }
}

impl FromPdfObject for PolygonAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            vertices: Polyline {
                points: point_list(b"Vertices", dictionary.required(b"Vertices")?)?,
                closed: true,
            },
            line_endings: dictionary.optional(b"LE")?,
            interior_color: dictionary.optional(b"IC")?,
            border_style: dictionary.optional(b"BS")?,
            intent: dictionary
                .optional::<Arc<[u8]>>(b"IT")?
                .map(|bytes| bytes.to_vec()),
        })
    }
}

impl FromPdfObject for PolyLineAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            vertices: Polyline {
                points: point_list(b"Vertices", dictionary.required(b"Vertices")?)?,
                closed: false,
            },
            line_endings: dictionary.optional(b"LE")?,
            interior_color: dictionary.optional(b"IC")?,
            border_style: dictionary.optional(b"BS")?,
            intent: dictionary
                .optional::<Arc<[u8]>>(b"IT")?
                .map(|bytes| bytes.to_vec()),
        })
    }
}

macro_rules! text_markup_decoder {
    ($type:ident) => {
        impl FromPdfObject for $type {
            fn from_pdf_object(
                context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
            ) -> ReadResult<Self> {
                let mut dictionary = context.dictionary()?;
                Ok(Self {
                    quad_points: quad_points(dictionary.required(b"QuadPoints")?)?,
                    color: dictionary.optional(b"C")?,
                    constant_opacity: dictionary.optional(b"CA")?,
                })
            }
        }
    };
}

text_markup_decoder!(HighlightAnnotation);
text_markup_decoder!(UnderlineAnnotation);
text_markup_decoder!(SquigglyAnnotation);
text_markup_decoder!(StrikeOutAnnotation);

impl FromPdfObject for StampAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(Self {
            name: context
                .dictionary()?
                .optional::<Arc<[u8]>>(b"Name")?
                .map(|bytes| bytes.to_vec()),
        })
    }
}

impl FromPdfObject for CaretAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            difference_rect: dictionary.lenient(b"RD"),
            style: dictionary.optional(b"Sy")?,
        })
    }
}

impl FromPdfObject for InkAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            // Producers omit `/InkList` when the strokes live only in the appearance stream,
            // which still draws the annotation, so a missing entry reads as no strokes.
            ink_list: dictionary.optional(b"InkList")?.unwrap_or_default(),
            border_style: dictionary.optional(b"BS")?,
            interior_color: dictionary.optional(b"IC")?,
        })
    }
}

/// Reads `/InkList`, an array of flat `[x1 y1 x2 y2 ...]` stroke arrays.
impl FromPdfObject for InkList {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let strokes: Vec<Vec<f32>> = context.array()?.read_all()?;
        Ok(Self {
            strokes: strokes
                .into_iter()
                .map(|stroke| {
                    Ok(Polyline {
                        points: point_list(b"InkList", stroke)?,
                        closed: false,
                    })
                })
                .collect::<DecodeResult<_>>()?,
        })
    }
}

impl FromPdfObject for PopupAnnotation {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut dictionary = context.dictionary()?;
        Ok(Self {
            // The parent is identified by its object number and never resolved.
            parent: dictionary
                .dictionary()
                .get(b"Parent")
                .map(|parent| {
                    let number = parent.try_object_number()?;
                    u64::try_from(number).map_err(|_| SourceDecodeError::ResourceLimit)
                })
                .transpose()?,
            open: dictionary.optional(b"Open")?,
        })
    }
}

/// Reads a `/Sy` caret symbol name, preserving unknown names.
impl FromPdfObject for CaretSymbolStyle {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(match context.object().try_bytes(context.source())? {
            b"P" => Self::P,
            b"None" => Self::None,
            other => Self::Unknown(Vec::from(other)),
        })
    }
}

/// Reads a link's `/H` highlight mode name, preserving unknown names.
impl FromPdfObject for LinkHighlightMode {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(match context.object().try_bytes(context.source())? {
            b"I" => Self::Invert,
            b"O" => Self::Outline,
            b"P" => Self::Push,
            b"N" => Self::None,
            b"T" => Self::Toggle,
            other => Self::Unknown(Vec::from(other)),
        })
    }
}

/// Reads one `/LE` line ending name, preserving unknown names.
impl FromPdfObject for LineEndingStyle {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(match context.object().try_bytes(context.source())? {
            b"S" => Self::Slash,
            b"B" | b"Square" => Self::Square,
            b"C" | b"Circle" => Self::Circle,
            b"D" | b"Diamond" => Self::Diamond,
            b"OpenArrow" => Self::OpenArrow,
            b"ClosedArrow" => Self::ClosedArrow,
            b"Butt" => Self::Butt,
            b"None" => Self::None,
            other => Self::Unknown(Vec::from(other)),
        })
    }
}

/// Pairs a flat `[x1 y1 x2 y2 ...]` coordinate list into points.
fn point_list(key: &'static [u8], values: Vec<f32>) -> DecodeResult<Vec<Point>> {
    if !values.len().is_multiple_of(2) {
        return Err(SourceDecodeError::InvalidEntry {
            entry: key,
            reason: format!("expected an even number of values, found {}", values.len()),
        });
    }
    Ok(values
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[x, y]| Point { x, y })
        .collect())
}

/// Parses a free-text callout's required two or three vertices.
fn callout_line(values: Vec<f32>) -> DecodeResult<Polyline> {
    let points = point_list(b"CL", values)?;
    if !matches!(points.len(), 2 | 3) {
        return Err(SourceDecodeError::InvalidEntry {
            entry: b"CL",
            reason: format!("expected two or three points, found {}", points.len()),
        });
    }
    Ok(Polyline {
        points,
        closed: false,
    })
}

/// Groups a flat `/QuadPoints` list into quadrilaterals of eight coordinates each.
fn quad_points(values: Vec<f32>) -> DecodeResult<Vec<Quad>> {
    if !values.len().is_multiple_of(8) {
        return Err(SourceDecodeError::InvalidEntry {
            entry: b"QuadPoints",
            reason: format!("expected a multiple of 8 numbers, found {}", values.len()),
        });
    }
    Ok(values
        .as_chunks::<8>()
        .0
        .iter()
        // Convert the PDF's top-left, top-right, bottom-left, bottom-right
        // points into perimeter order.
        .map(|&[x1, y1, x2, y2, x3, y3, x4, y4]| Quad {
            corners: [
                Point { x: x1, y: y1 },
                Point { x: x2, y: y2 },
                Point { x: x4, y: y4 },
                Point { x: x3, y: y3 },
            ],
        })
        .collect())
}
