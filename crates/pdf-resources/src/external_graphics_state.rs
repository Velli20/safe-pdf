use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ReadResult,
};
use pdf_object_reader::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::{error::PdfPagesError, resource::Resource, soft_mask::SoftMask};
use num_traits::FromPrimitive;
use pdf_graphics::{BlendMode, DashPattern, LineCap, LineJoin};

/// Represents a key-value pair from a PDF External Graphics State dictionary (`ExtGState`).
///
/// An `ExtGState` dictionary contains parameters that control the graphics state,
/// such as line styles, color rendering, and alpha transparency. This enum
/// enumerates the possible keys (parameters) found in such a dictionary and
/// holds the corresponding parsed value.
pub enum ExternalGraphicsStateKey {
    /// Line width (`LW`). A number specifying the thickness of stroked lines.
    LineWidth(f32),
    /// Line cap style (`LC`). An integer specifying the shape to be used at the ends of open subpaths
    /// when they are stroked (0: butt, 1: round, 2: projecting square).
    LineCap(LineCap),
    /// Line join style (`LJ`). An integer specifying the shape to be used at the corners of paths
    /// when they are stroked (0: miter, 1: round, 2: bevel).
    LineJoin(LineJoin),
    /// Miter limit (`ML`). A number specifying the maximum ratio of the miter length to the line width
    /// for mitered line joins.
    MiterLimit(f32),
    /// Dash pattern (`D`). An array of numbers specifying the lengths of alternating dashes and gaps
    /// (the dash array) and a number specifying the phase (the dash phase).
    DashPattern(DashPattern),
    /// Rendering intent (`RI`). A name specifying the color rendering intent.
    RenderingIntent(Vec<u8>),
    /// Overprint for stroke (`OP`). A boolean specifying whether stroking operations are to be
    /// performed in overprint mode.
    OverprintStroke(bool),
    /// Overprint for fill (`op`). A boolean specifying whether non-stroking operations are to be
    /// performed in overprint mode.
    OverprintFill(bool),
    /// Overprint mode (`OPM`). An integer specifying the overprint mode (0 or 1).
    OverprintMode(i32),
    /// Font (`Font`). An array containing a font dictionary or stream and a font size.
    /// Represented here as the object number of the font resource and the font size.
    Font(pdf_object_reader::ObjectHandle<Resource>, f32),
    /// Blend mode (`BM`). A name or array of names specifying the blend mode to be used
    /// when compositing objects.
    BlendMode(Vec<BlendMode>),
    /// Soft mask (`SMask`). A dictionary specifying the soft mask to be used, or the name `None`.
    SoftMask(Option<pdf_object_reader::ObjectHandle<SoftMask>>),
    /// Stroking alpha constant (`CA`). A number in the range 0.0 to 1.0 specifying the constant
    /// opacity value to be used for stroking operations.
    StrokingAlpha(f32),
    /// Nonstroking alpha constant (`ca`). A number in the range 0.0 to 1.0 specifying the constant
    /// opacity value to be used for non-stroking operations.
    NonStrokingAlpha(f32),
    /// Stroke adjustment (`SA`). A boolean that specifies whether to adjust stroke endpoints
    /// and joins to the device pixel grid to produce thinner or more consistent strokes.
    StrokeAdjustment(bool),
    /// Apple-specific anti-aliasing flag (`AAPL:AA`).
    ///
    /// This is a Quartz PDF (Apple) extension, used to control anti-aliasing
    /// in Apple-generated PDFs.
    AppleAntiAliasing(bool),
    /// Indicates whether the alpha or shape of the current painting operation
    /// is used when computing a soft mask.
    AlphaIsShape(bool),
    /// Smoothness tolerance (`SM`). Defines the tolerance used when rendering smooth curves.
    SmoothnessTolerance(f32),
    /// Transfer function (`TR`). A function that modifies the color values
    /// of the current painting operation.
    TransferFunction,
    /// New transfer function (`TR2`). A more advanced function that modifies
    /// the color values of the current painting operation.
    TransferFunctionNew,
}

pub struct ExternalGraphicsState {
    pub params: Vec<ExternalGraphicsStateKey>,
}

impl FromPdfObject for ExternalGraphicsState {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let dictionary = context.dictionary().clone();
        let mut params = Vec::new();
        for (name, value) in &dictionary.dictionary {
            if name == b"Type" {
                continue;
            }
            if let Some(param) = parse_entry(name, value, &mut context)? {
                params.push(param);
            }
        }
        Ok(Self { params })
    }
}

fn invalid_ext_gstate_entry_structure(
    entry: &[u8],
    expected_structure: &'static str,
    actual_structure: String,
) -> PdfPagesError {
    PdfPagesError::InvalidExtGStateEntryStructure {
        entry: String::from_utf8_lossy(entry).into_owned(),
        expected_structure,
        actual_structure,
    }
}

fn invalid_ext_gstate_entry_value(entry: &[u8], reason: impl Into<String>) -> PdfPagesError {
    PdfPagesError::InvalidExtGStateEntryValue {
        entry: String::from_utf8_lossy(entry).into_owned(),
        reason: reason.into(),
    }
}

fn parse_dash_pattern(
    key_name: &[u8],
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Option<ExternalGraphicsStateKey>, PdfPagesError> {
    let arr = value.try_array(objects)?.as_slice();
    let [dash_array, dash_phase] = arr else {
        return Err(invalid_ext_gstate_entry_structure(
            key_name,
            "an array with exactly 2 elements",
            format!("an array with {} elements", arr.len()),
        ));
    };

    let dash_array = dash_array.try_vec_of::<f32>(objects)?;
    let dash_phase = dash_phase.try_number::<f32>(objects)?;
    let Some(dash_pattern) = DashPattern::new(&dash_array, dash_phase)
        .map_err(|err| invalid_ext_gstate_entry_value(key_name, err.to_string()))?
    else {
        return Ok(None);
    };

    Ok(Some(ExternalGraphicsStateKey::DashPattern(dash_pattern)))
}

fn parse_font<A: ObjectAccess + ?Sized>(
    key_name: &[u8],
    value: &ObjectVariant,
    context: &mut DictionaryContext<'_, A>,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
    let arr = value.try_array(context.source())?.to_vec();
    let [font_ref, font_size] = arr.as_slice() else {
        return Err(invalid_ext_gstate_entry_structure(
            key_name,
            "an array with exactly 2 elements",
            format!("an array with {} elements", arr.len()),
        ));
    };
    let font_size = font_size.try_number::<f32>(context.source())?;
    let font = context.read_shared::<Resource>(font_ref)?;
    Ok(ExternalGraphicsStateKey::Font(font, font_size))
}

fn parse_blend_mode(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
    let blend_modes_vec: Vec<BlendMode> = if value.is_array() {
        value
            .try_array(objects)?
            .iter()
            .map(|obj| obj.try_bytes(objects).map(BlendMode::from))
            .collect::<Result<Vec<BlendMode>, _>>()?
    } else {
        let mode = BlendMode::from(value.try_bytes(objects)?);
        vec![mode]
    };

    Ok(ExternalGraphicsStateKey::BlendMode(blend_modes_vec))
}

/// Parse a single key/value pair of the ExtGState dictionary.
///
/// Returns `Ok(None)` for unrecognized keys, which are silently ignored
/// per the PDF specification.
fn parse_entry<A: ObjectAccess + ?Sized>(
    name: &[u8],
    value: &ObjectVariant,
    context: &mut DictionaryContext<'_, A>,
) -> Result<Option<ExternalGraphicsStateKey>, PdfPagesError> {
    let raw_value = value;
    let resolved = context.resolve(value)?;
    let value = resolved.value();
    let objects = context.source();
    let parsed = match name {
        b"TR" => ExternalGraphicsStateKey::TransferFunction,
        b"TR2" => ExternalGraphicsStateKey::TransferFunctionNew,
        b"SM" => ExternalGraphicsStateKey::SmoothnessTolerance(value.try_number::<f32>(objects)?),
        b"LW" => ExternalGraphicsStateKey::LineWidth(value.try_number::<f32>(objects)?),
        b"LC" => {
            let cap_val = value.try_number::<i32>(objects)?;
            let cap = LineCap::from_i32(cap_val).ok_or_else(|| {
                invalid_ext_gstate_entry_value(
                    name,
                    format!("unsupported line cap value {cap_val} (expected 0, 1, or 2)"),
                )
            })?;
            ExternalGraphicsStateKey::LineCap(cap)
        }
        b"LJ" => {
            let join_val = value.try_number::<i32>(objects)?;
            let join = LineJoin::from_i32(join_val).ok_or_else(|| {
                invalid_ext_gstate_entry_value(
                    name,
                    format!("unsupported line join value {join_val} (expected 0, 1, or 2)"),
                )
            })?;
            ExternalGraphicsStateKey::LineJoin(join)
        }
        b"ML" => ExternalGraphicsStateKey::MiterLimit(value.try_number::<f32>(objects)?),
        b"D" => match parse_dash_pattern(name, value, objects)? {
            Some(param) => param,
            None => return Ok(None),
        },
        b"RI" => ExternalGraphicsStateKey::RenderingIntent(Vec::from(value.try_bytes(objects)?)),
        b"OP" => ExternalGraphicsStateKey::OverprintStroke(value.try_boolean(objects)?),
        b"op" => ExternalGraphicsStateKey::OverprintFill(value.try_boolean(objects)?),
        b"OPM" => ExternalGraphicsStateKey::OverprintMode(value.try_number::<i32>(objects)?),
        b"Font" => parse_font(name, value, context)?,
        b"BM" => parse_blend_mode(value, objects)?,
        b"SMask" => {
            let soft_mask = match value {
                ObjectVariant::Dictionary(_) => Some(context.read_shared::<SoftMask>(raw_value)?),
                other => match other.try_bytes(objects)? {
                    b"None" => None,
                    _ => {
                        return Err(invalid_ext_gstate_entry_value(
                            name,
                            "expected a soft mask dictionary or the name 'None'",
                        ));
                    }
                },
            };
            ExternalGraphicsStateKey::SoftMask(soft_mask)
        }
        b"CA" => ExternalGraphicsStateKey::StrokingAlpha(value.try_number::<f32>(objects)?),
        b"ca" => ExternalGraphicsStateKey::NonStrokingAlpha(value.try_number::<f32>(objects)?),
        b"SA" => ExternalGraphicsStateKey::StrokeAdjustment(value.try_boolean(objects)?),
        b"AAPL:AA" => ExternalGraphicsStateKey::AppleAntiAliasing(value.try_boolean(objects)?),
        b"AIS" => ExternalGraphicsStateKey::AlphaIsShape(value.try_boolean(objects)?),
        _ => return Ok(None),
    };

    Ok(Some(parsed))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_object_reader::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use crate::error::PdfPagesError;

    use super::{ExternalGraphicsState, ExternalGraphicsStateKey};

    fn dash_dict(dash_array: Vec<ObjectVariant>, dash_phase: f32) -> Dictionary {
        Dictionary::new(BTreeMap::from([(
            Vec::from(b"D"),
            ObjectVariant::Array(
                vec![
                    ObjectVariant::Array(dash_array.into()),
                    ObjectVariant::Real(f64::from(dash_phase)),
                ]
                .into(),
            ),
        )]))
    }

    fn parse_ext_gstate(
        dictionary: &Dictionary,
    ) -> pdf_object_reader::ReadResult<Option<ExternalGraphicsState>> {
        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        reader.read::<Option<ExternalGraphicsState>>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((dictionary).clone()),
        )
    }

    #[test]
    fn extgstate_dash_entry_is_typed() {
        let dictionary = dash_dict(
            vec![ObjectVariant::Real(3.0), ObjectVariant::Real(1.0)],
            2.0,
        );

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let ext_gstate = reader
            .read::<Option<ExternalGraphicsState>>(
                &pdf_object_reader::object_variant::ObjectVariant::Dictionary(
                    (&dictionary).clone(),
                ),
            )
            .expect("extgstate should parse")
            .expect("extgstate should be present");

        assert_eq!(ext_gstate.params.len(), 1);
        match &ext_gstate.params[0] {
            ExternalGraphicsStateKey::DashPattern(pattern) => {
                assert_eq!(pattern.intervals, vec![3.0, 1.0]);
                assert_eq!(pattern.phase, 2.0);
            }
            _ => panic!("unexpected extgstate entry"),
        }
    }

    #[test]
    fn invalid_dash_entry_surfaces_as_value_error() {
        let dictionary = dash_dict(
            vec![ObjectVariant::Real(0.0), ObjectVariant::Real(0.0)],
            0.0,
        );

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let error = match reader.read::<Option<ExternalGraphicsState>>(
            &pdf_object_reader::object_variant::ObjectVariant::Dictionary((&dictionary).clone()),
        ) {
            Err(error) => error,
            Ok(_) => panic!("invalid dash pattern should fail"),
        };

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<PdfPagesError>(), Some(PdfPagesError::InvalidExtGStateEntryValue { entry, .. }) if entry == "D")
        ));
    }

    #[test]
    fn empty_dash_array_is_ignored() {
        let dictionary = dash_dict(Vec::new(), 0.0);

        let reader = pdf_object_reader::ObjectReader::new(&PassthroughResolver);

        let ext_gstate = reader
            .read::<Option<ExternalGraphicsState>>(
                &pdf_object_reader::object_variant::ObjectVariant::Dictionary(
                    (&dictionary).clone(),
                ),
            )
            .expect("extgstate should parse")
            .expect("extgstate should be present");

        assert!(ext_gstate.params.is_empty());
    }

    #[test]
    fn soft_mask_none_is_preserved() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            Vec::from(b"SMask"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"None".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        )]));

        let ext_gstate = parse_ext_gstate(&dictionary)
            .expect("extgstate should parse")
            .expect("extgstate should be present");

        assert!(matches!(
            ext_gstate.params.as_slice(),
            [ExternalGraphicsStateKey::SoftMask(None)]
        ));
    }

    #[test]
    fn invalid_soft_mask_name_is_rejected() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            Vec::from(b"SMask"),
            pdf_object_reader::pdf_string::PdfString::from(
                b"Invalid".to_vec(),
                pdf_object_reader::string_kind::StringKind::Name,
            ),
        )]));

        let error = match parse_ext_gstate(&dictionary) {
            Err(error) => error,
            Ok(_) => panic!("invalid soft mask should fail"),
        };

        assert!(matches!(
            error,
            pdf_object_reader::ObjectReadError::Decode { source, .. } if matches!(source.downcast_ref::<PdfPagesError>(), Some(PdfPagesError::InvalidExtGStateEntryValue { entry, .. }) if entry == "SMask")
        ));
    }
}
