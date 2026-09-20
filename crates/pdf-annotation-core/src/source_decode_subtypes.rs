//! Subtype-specific payload decoding for the `/Subtype` vocabulary Core retains.
use crate::error::SourceDecodeError;
use crate::models::{Point, Quad};
use crate::pdf_data::{
    AnnotationAction, AnnotationDestination, BorderEffect, BorderStyle, CaretAnnotation,
    CaretSymbolStyle, CircleAnnotation, FreeTextAnnotation, HighlightAnnotation, InkAnnotation,
    InkList, LineAnnotation, LineEndingStyle, LinkAnnotation, LinkHighlightMode, NativeAnnotation,
    PolyLineAnnotation, PolygonAnnotation, PopupAnnotation, SquareAnnotation, SquigglyAnnotation,
    StampAnnotation, StrikeOutAnnotation, TextAnnotation, UnderlineAnnotation, WidgetAnnotation,
};
use crate::source_decode::{DecodeResult, color};
use pdf_graphics::polyline::Polyline;
use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

impl NativeAnnotation {
    /// Dispatches on `/Subtype`; unrecognized names retain their bytes unparsed.
    pub(crate) fn from_dictionary(
        subtype: &[u8],
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        Ok(match subtype {
            b"Text" => Self::Text(Box::new(TextAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Link" => Self::Link(Box::new(LinkAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"FreeText" => Self::FreeText(Box::new(FreeTextAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Line" => Self::Line(Box::new(LineAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Square" => Self::Square(Box::new(SquareAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Circle" => Self::Circle(Box::new(CircleAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Polygon" => Self::Polygon(Box::new(PolygonAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"PolyLine" => Self::PolyLine(Box::new(PolyLineAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Highlight" => Self::Highlight(Box::new(HighlightAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Underline" => Self::Underline(Box::new(UnderlineAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Squiggly" => Self::Squiggly(Box::new(SquigglyAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"StrikeOut" => Self::StrikeOut(Box::new(StrikeOutAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Stamp" => Self::Stamp(Box::new(StampAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Caret" => Self::Caret(Box::new(CaretAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Ink" => Self::Ink(Box::new(InkAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Popup" => Self::Popup(Box::new(PopupAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            b"Widget" => Self::Widget(Box::new(WidgetAnnotation::from_dictionary(
                dictionary, objects,
            )?)),
            _ => Self::Unknown {
                subtype: subtype.to_vec(),
            },
        })
    }
}

impl TextAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let open = dictionary.optional_boolean(b"Open", objects)?;
        let name = dictionary.optional_bytes(b"Name", objects)?.map(Vec::from);
        let state = dictionary.optional_bytes_vec(b"State", objects)?;
        let state_model = dictionary.optional_bytes_vec(b"StateModel", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);

        Ok(Self {
            open,
            name,
            state,
            state_model,
            intent,
        })
    }
}

impl LinkAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let highlight_mode = dictionary
            .get(b"H")
            .map(|value| value.try_bytes(objects).map(LinkHighlightMode::from))
            .transpose()?;
        let destination = AnnotationDestination::from_dictionary(dictionary, b"Dest", objects)?;
        let action = AnnotationAction::from_dictionary(dictionary, b"A", objects)?;
        let quad_points = quad_points(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let border_effect = BorderEffect::from_dictionary(dictionary, b"BE", objects)?;

        Ok(Self {
            highlight_mode,
            destination,
            action,
            quad_points,
            border_style,
            border_effect,
        })
    }
}

impl FreeTextAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let default_appearance = dictionary.optional_bytes_vec(b"DA", objects)?;
        let quadding = dictionary.optional_number::<i32>(b"Q", objects)?;
        let rich_contents = dictionary.optional_bytes_vec(b"RC", objects)?;
        let default_style = dictionary.optional_bytes_vec(b"DS", objects)?;
        let callout_line = dictionary
            .get(b"CL")
            .map(|value| callout_line(value, objects))
            .transpose()?;
        let difference_rect = dictionary.optional_array_of::<f64, 4>(b"RD", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);
        let border_effect = BorderEffect::from_dictionary(dictionary, b"BE", objects)?;

        Ok(Self {
            default_appearance,
            quadding,
            rich_contents,
            default_style,
            callout_line,
            border_effect,
            difference_rect,
            intent,
        })
    }
}

impl LineAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let line = dictionary
            .get_or_err(b"L")?
            .try_array_of::<f64, 4>(objects)?;
        let line_endings = line_endings(dictionary, objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let interior_color = color(dictionary, b"IC", objects)?;
        let leader_line_length = dictionary.optional_number::<f64>(b"LL", objects)?;
        let leader_line_extension = dictionary.optional_number::<f64>(b"LLE", objects)?;
        let caption = dictionary.optional_boolean(b"Cap", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);

        Ok(Self {
            line,
            line_endings,
            border_style,
            interior_color,
            leader_line_length,
            leader_line_extension,
            caption,
            intent,
        })
    }
}

impl SquareAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let interior_color = color(dictionary, b"IC", objects)?;
        let border_effect = BorderEffect::from_dictionary(dictionary, b"BE", objects)?;
        let difference_rect = dictionary.optional_array_of::<f64, 4>(b"RD", objects)?;

        Ok(Self {
            border_style,
            interior_color,
            border_effect,
            difference_rect,
        })
    }
}

impl CircleAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let interior_color = color(dictionary, b"IC", objects)?;
        let border_effect = BorderEffect::from_dictionary(dictionary, b"BE", objects)?;
        let difference_rect = dictionary.optional_array_of::<f64, 4>(b"RD", objects)?;

        Ok(Self {
            border_style,
            interior_color,
            border_effect,
            difference_rect,
        })
    }
}

impl PolygonAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let vertices = Polyline {
            points: point_list(b"Vertices", dictionary.get_or_err(b"Vertices")?, objects)?,
            closed: true,
        };
        let line_endings = line_endings(dictionary, objects)?;
        let interior_color = color(dictionary, b"IC", objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);

        Ok(Self {
            vertices,
            line_endings,
            interior_color,
            border_style,
            intent,
        })
    }
}

impl PolyLineAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let vertices = Polyline {
            points: point_list(b"Vertices", dictionary.get_or_err(b"Vertices")?, objects)?,
            closed: false,
        };
        let line_endings = line_endings(dictionary, objects)?;
        let interior_color = color(dictionary, b"IC", objects)?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let intent = dictionary.optional_bytes(b"IT", objects)?.map(Vec::from);

        Ok(Self {
            vertices,
            line_endings,
            interior_color,
            border_style,
            intent,
        })
    }
}

impl HighlightAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let quad_points = required_quad_points(dictionary, objects)?;
        let color = color(dictionary, b"C", objects)?;
        let constant_opacity = dictionary.optional_number::<f64>(b"CA", objects)?;

        Ok(Self {
            quad_points,
            color,
            constant_opacity,
        })
    }
}

impl UnderlineAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let quad_points = required_quad_points(dictionary, objects)?;
        let color = color(dictionary, b"C", objects)?;
        let constant_opacity = dictionary.optional_number::<f64>(b"CA", objects)?;

        Ok(Self {
            quad_points,
            color,
            constant_opacity,
        })
    }
}

impl SquigglyAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let quad_points = required_quad_points(dictionary, objects)?;
        let color = color(dictionary, b"C", objects)?;
        let constant_opacity = dictionary.optional_number::<f64>(b"CA", objects)?;

        Ok(Self {
            quad_points,
            color,
            constant_opacity,
        })
    }
}

impl StrikeOutAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let quad_points = required_quad_points(dictionary, objects)?;
        let color = color(dictionary, b"C", objects)?;
        let constant_opacity = dictionary.optional_number::<f64>(b"CA", objects)?;

        Ok(Self {
            quad_points,
            color,
            constant_opacity,
        })
    }
}

impl StampAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let name = dictionary.optional_bytes(b"Name", objects)?.map(Vec::from);
        Ok(Self { name })
    }
}

impl CaretAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let difference_rect = dictionary.optional_array_of::<f64, 4>(b"RD", objects)?;
        let style = dictionary
            .get(b"Sy")
            .map(|value| value.try_bytes(objects).map(CaretSymbolStyle::from))
            .transpose()?;

        Ok(Self {
            difference_rect,
            style,
        })
    }
}

impl InkAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let ink_list = InkList::from_dictionary(dictionary, objects)?
            .ok_or(SourceDecodeError::MissingEntry { entry: b"InkList" })?;
        let border_style = BorderStyle::from_dictionary(dictionary, b"BS", objects)?;
        let interior_color = color(dictionary, b"IC", objects)?;

        Ok(Self {
            ink_list,
            border_style,
            interior_color,
        })
    }
}

impl InkList {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Option<Self>> {
        let Some(value) = dictionary.get(b"InkList") else {
            return Ok(None);
        };

        let strokes = value.try_array(objects)?;
        let mut parsed = Vec::with_capacity(strokes.len());

        for stroke in strokes {
            parsed.push(Polyline {
                points: point_list(b"InkList", stroke, objects)?,
                closed: false,
            });
        }

        Ok(Some(Self { strokes: parsed }))
    }
}

impl PopupAnnotation {
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> DecodeResult<Self> {
        let parent = dictionary
            .get(b"Parent")
            .map(|obj| {
                let number = obj.try_object_number()?;
                u64::try_from(number).map_err(|_| SourceDecodeError::ResourceLimit)
            })
            .transpose()?;
        let open = dictionary.optional_boolean(b"Open", objects)?;

        Ok(Self { parent, open })
    }
}

fn point_list(
    key: &'static [u8],
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Vec<Point>> {
    let values = value.try_vec_of::<f64>(objects)?;
    if !values.len().is_multiple_of(2) {
        return Err(SourceDecodeError::InvalidEntry {
            entry: key,
            reason: format!("expected an even number of values, found {}", values.len()),
        });
    }

    let mut points = Vec::with_capacity(values.len() / 2);
    for &[x, y] in values.as_chunks::<2>().0 {
        points.push(Point { x, y });
    }

    Ok(points)
}

/// Parses a free-text callout's required two or three vertices.
fn callout_line(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Polyline<f64>> {
    let points = point_list(b"CL", value, objects)?;
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

fn quad_points(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Option<Vec<Quad>>> {
    let Some(value) = dictionary.get(b"QuadPoints") else {
        return Ok(None);
    };
    let values = value.try_vec_of::<f64>(objects)?;
    if !values.len().is_multiple_of(8) {
        return Err(SourceDecodeError::InvalidEntry {
            entry: b"QuadPoints",
            reason: format!("expected a multiple of 8 numbers, found {}", values.len()),
        });
    }
    let mut quads = Vec::with_capacity(values.len() / 8);
    for &[x1, y1, x2, y2, x3, y3, x4, y4] in values.as_chunks::<8>().0 {
        // Convert the PDF's top-left, top-right, bottom-left, bottom-right
        // points into perimeter order.
        quads.push(Quad {
            corners: [
                Point { x: x1, y: y1 },
                Point { x: x2, y: y2 },
                Point { x: x4, y: y4 },
                Point { x: x3, y: y3 },
            ],
        });
    }
    Ok(Some(quads))
}

fn required_quad_points(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Vec<Quad>> {
    quad_points(dictionary, objects)?.ok_or(SourceDecodeError::MissingEntry {
        entry: b"QuadPoints",
    })
}

fn line_endings(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> DecodeResult<Option<[LineEndingStyle; 2]>> {
    let Some(value) = dictionary.get(b"LE") else {
        return Ok(None);
    };

    let endings = value.try_array(objects)?;
    if endings.len() != 2 {
        return Err(SourceDecodeError::InvalidEntry {
            entry: b"LE",
            reason: format!("expected 2 line ending names, found {}", endings.len()),
        });
    }

    let mut parsed = [LineEndingStyle::None, LineEndingStyle::None];
    for (slot, item) in parsed.iter_mut().zip(endings.iter()) {
        *slot = LineEndingStyle::from(item.try_bytes(objects)?);
    }

    Ok(Some(parsed))
}
