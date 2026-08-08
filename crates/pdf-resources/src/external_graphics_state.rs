use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    error::PdfPagesError,
    object_reader::{ReadCycleTracker, ReadFromDictionary},
    resource::Resource,
    resource_cache::{ResourceCache, read_resource_lazy},
    resources::read_font_resource,
    soft_mask::SoftMask,
};
use num_traits::FromPrimitive;
use pdf_content_stream::ContentStreamIdAllocator;
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
    RenderingIntent(String),
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
    Font(Resource, f32),
    /// Blend mode (`BM`). A name or array of names specifying the blend mode to be used
    /// when compositing objects.
    BlendMode(Vec<BlendMode>),
    /// Soft mask (`SMask`). A dictionary specifying the soft mask to be used, or the name `None`.
    SoftMask(Option<Box<SoftMask>>),
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

impl ReadFromDictionary for ExternalGraphicsState {
    type Output = Self;

    fn read_dictionary_inner(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        let mut params: Vec<ExternalGraphicsStateKey> = Vec::new();

        for (name, value) in &dictionary.dictionary {
            if name == "Type" {
                // The "Type" entry is optional and, if present, must be "ExtGState".
                // We can safely ignore it during parsing.
                continue;
            }
            // Resolve reference (if any).
            let resolved = match value {
                ObjectVariant::Reference(_) => objects.resolve_object(value)?,
                _ => value,
            };

            if let Some(param) =
                parse_entry(name, resolved, objects, cache, cycle_tracker, id_allocator)?
            {
                params.push(param);
            }
        }

        Ok(ExternalGraphicsState { params })
    }
}

fn invalid_ext_gstate_entry_structure(
    entry: &str,
    expected_structure: &'static str,
    actual_structure: String,
) -> PdfPagesError {
    PdfPagesError::InvalidExtGStateEntryStructure {
        entry: entry.to_string(),
        expected_structure,
        actual_structure,
    }
}

fn invalid_ext_gstate_entry_value(entry: &str, reason: impl Into<String>) -> PdfPagesError {
    PdfPagesError::InvalidExtGStateEntryValue {
        entry: entry.to_string(),
        reason: reason.into(),
    }
}

fn parse_dash_pattern(
    key_name: &str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Option<ExternalGraphicsStateKey>, PdfPagesError> {
    let arr = value.try_array(objects)?;
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

fn parse_font(
    key_name: &str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
    let arr = value.try_array(objects)?;
    let [font_ref, font_size] = arr else {
        return Err(invalid_ext_gstate_entry_structure(
            key_name,
            "an array with exactly 2 elements",
            format!("an array with {} elements", arr.len()),
        ));
    };
    let font_size = font_size.try_number::<f32>(objects)?;

    let dict = font_ref.try_dictionary(objects)?;
    let resource = read_resource_lazy(cache, dict.object_number, |cache| {
        read_font_resource(dict, objects, cache, cycle_tracker, id_allocator)
    })?;

    Ok(ExternalGraphicsStateKey::Font(resource, font_size))
}

fn parse_blend_mode(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
    let blend_modes_vec: Vec<BlendMode> = if value.is_array() {
        value
            .try_array(objects)?
            .iter()
            .map(|obj| obj.try_str(objects).map(BlendMode::from))
            .collect::<Result<Vec<BlendMode>, _>>()?
    } else {
        let mode = BlendMode::from(value.try_str(objects)?);
        vec![mode]
    };

    Ok(ExternalGraphicsStateKey::BlendMode(blend_modes_vec))
}

/// Parse a single key/value pair of the ExtGState dictionary.
///
/// Returns `Ok(None)` for unrecognized keys, which are silently ignored
/// per the PDF specification.
fn parse_entry(
    name: &str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<Option<ExternalGraphicsStateKey>, PdfPagesError> {
    let parsed = match name {
        "TR" => ExternalGraphicsStateKey::TransferFunction,
        "TR2" => ExternalGraphicsStateKey::TransferFunctionNew,
        "SM" => ExternalGraphicsStateKey::SmoothnessTolerance(value.try_number::<f32>(objects)?),
        "LW" => ExternalGraphicsStateKey::LineWidth(value.try_number::<f32>(objects)?),
        "LC" => {
            let cap_val = value.try_number::<i32>(objects)?;
            let cap = LineCap::from_i32(cap_val).ok_or_else(|| {
                invalid_ext_gstate_entry_value(
                    name,
                    format!("unsupported line cap value {cap_val} (expected 0, 1, or 2)"),
                )
            })?;
            ExternalGraphicsStateKey::LineCap(cap)
        }
        "LJ" => {
            let join_val = value.try_number::<i32>(objects)?;
            let join = LineJoin::from_i32(join_val).ok_or_else(|| {
                invalid_ext_gstate_entry_value(
                    name,
                    format!("unsupported line join value {join_val} (expected 0, 1, or 2)"),
                )
            })?;
            ExternalGraphicsStateKey::LineJoin(join)
        }
        "ML" => ExternalGraphicsStateKey::MiterLimit(value.try_number::<f32>(objects)?),
        "D" => match parse_dash_pattern(name, value, objects)? {
            Some(param) => param,
            None => return Ok(None),
        },
        "RI" => ExternalGraphicsStateKey::RenderingIntent(value.try_str(objects)?.to_string()),
        "OP" => ExternalGraphicsStateKey::OverprintStroke(value.try_boolean(objects)?),
        "op" => ExternalGraphicsStateKey::OverprintFill(value.try_boolean(objects)?),
        "OPM" => ExternalGraphicsStateKey::OverprintMode(value.try_number::<i32>(objects)?),
        "Font" => parse_font(name, value, objects, cache, cycle_tracker, id_allocator)?,
        "BM" => parse_blend_mode(value, objects)?,
        "SMask" => {
            let soft_mask = match value {
                ObjectVariant::Dictionary(dictionary) => SoftMask::from_dictionary(
                    dictionary,
                    objects,
                    cache,
                    cycle_tracker,
                    id_allocator,
                )?
                .map(Box::new),
                other => match other.try_str(objects)? {
                    "None" => None,
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
        "CA" => ExternalGraphicsStateKey::StrokingAlpha(value.try_number::<f32>(objects)?),
        "ca" => ExternalGraphicsStateKey::NonStrokingAlpha(value.try_number::<f32>(objects)?),
        "SA" => ExternalGraphicsStateKey::StrokeAdjustment(value.try_boolean(objects)?),
        "AAPL:AA" => ExternalGraphicsStateKey::AppleAntiAliasing(value.try_boolean(objects)?),
        "AIS" => ExternalGraphicsStateKey::AlphaIsShape(value.try_boolean(objects)?),
        _ => return Ok(None),
    };

    Ok(Some(parsed))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_content_stream::ContentStreamIdAllocator;
    use pdf_object::{
        dictionary::Dictionary, object_resolver::PassthroughResolver, object_variant::ObjectVariant,
    };

    use crate::object_reader::ReadFromDictionary;
    use crate::{
        error::PdfPagesError, object_reader::ReadCycleTracker, resource_cache::DefaultResourceCache,
    };

    use super::{ExternalGraphicsState, ExternalGraphicsStateKey};

    fn dash_dict(dash_array: Vec<ObjectVariant>, dash_phase: f32) -> Dictionary {
        Dictionary::new(BTreeMap::from([(
            "D".to_string(),
            ObjectVariant::Array(vec![
                ObjectVariant::Array(dash_array),
                ObjectVariant::Real(f64::from(dash_phase)),
            ]),
        )]))
    }

    fn parse_ext_gstate(
        dictionary: &Dictionary,
    ) -> Result<Option<ExternalGraphicsState>, PdfPagesError> {
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        ExternalGraphicsState::from_dictionary(
            dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
    }

    #[test]
    fn extgstate_dash_entry_is_typed() {
        let dictionary = dash_dict(
            vec![ObjectVariant::Real(3.0), ObjectVariant::Real(1.0)],
            2.0,
        );
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let ext_gstate = ExternalGraphicsState::from_dictionary(
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
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
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let error = match ExternalGraphicsState::from_dictionary(
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        ) {
            Err(error) => error,
            Ok(_) => panic!("invalid dash pattern should fail"),
        };

        assert!(matches!(
            error,
            PdfPagesError::InvalidExtGStateEntryValue { entry, .. } if entry == "D"
        ));
    }

    #[test]
    fn empty_dash_array_is_ignored() {
        let dictionary = dash_dict(Vec::new(), 0.0);
        let mut cache = DefaultResourceCache::default();
        let mut cycle_tracker = ReadCycleTracker::default();
        let mut id_allocator = ContentStreamIdAllocator::new();

        let ext_gstate = ExternalGraphicsState::from_dictionary(
            &dictionary,
            &PassthroughResolver,
            &mut cache,
            &mut cycle_tracker,
            &mut id_allocator,
        )
        .expect("extgstate should parse")
        .expect("extgstate should be present");

        assert!(ext_gstate.params.is_empty());
    }

    #[test]
    fn soft_mask_none_is_preserved() {
        let dictionary = Dictionary::new(BTreeMap::from([(
            "SMask".to_string(),
            ObjectVariant::Name(b"None".to_vec()),
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
            "SMask".to_string(),
            ObjectVariant::Name(b"Invalid".to_vec()),
        )]));

        let error = match parse_ext_gstate(&dictionary) {
            Err(error) => error,
            Ok(_) => panic!("invalid soft mask should fail"),
        };

        assert!(matches!(
            error,
            PdfPagesError::InvalidExtGStateEntryValue { entry, .. } if entry == "SMask"
        ));
    }
}
