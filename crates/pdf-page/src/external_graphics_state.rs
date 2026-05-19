use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    error::PdfPagesError,
    object_reader::{ReadCycleTracker, ReadFromDictionary, ReadXObject},
    resource_cache::ResourceCache,
    xobject::XObject,
};
use num_traits::FromPrimitive;
use pdf_content_stream::ContentStreamIdAllocator;
use pdf_graphics::{BlendMode, LineCap, LineJoin, MaskMode};

/// Soft mask extracted from an ExtGState `SMask` entry.
pub struct SoftMask {
    /// How the mask is derived from the transparency group output: from color
    /// luminance (`Luminosity`) or from alpha/shape (`Alpha`).
    pub mask_type: MaskMode,
    /// The transparency group XObject (`G`) whose rendered result provides the
    /// input used to compute the soft mask.
    pub shape: XObject,
}

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
    DashPattern(Vec<f32>, f32),
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
    Font(usize, f32),
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

fn parse_mask_mode(value: &str) -> Result<MaskMode, PdfPagesError> {
    match value {
        "Luminosity" => Ok(MaskMode::Luminosity),
        "Alpha" => Ok(MaskMode::Alpha),
        other => Err(invalid_ext_gstate_entry_value(
            "SMask/S",
            format!("unsupported soft mask mode '{other}' (expected 'Alpha' or 'Luminosity')"),
        )),
    }
}

fn parse_dash_pattern(
    key_name: &str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
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
    Ok(ExternalGraphicsStateKey::DashPattern(
        dash_array, dash_phase,
    ))
}

fn parse_font(
    key_name: &str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
    let arr = value.try_array(objects)?;
    let [font_ref, font_size] = arr else {
        return Err(invalid_ext_gstate_entry_structure(
            key_name,
            "an array with exactly 2 elements",
            format!("an array with {} elements", arr.len()),
        ));
    };
    let font_ref = font_ref.try_object_number()?;
    let font_size = font_size.try_number::<f32>(objects)?;
    Ok(ExternalGraphicsStateKey::Font(font_ref, font_size))
}

fn to_blend_mode(s: &str) -> Result<BlendMode, PdfPagesError> {
    match s {
        "Normal" => Ok(BlendMode::Normal),
        "Multiply" => Ok(BlendMode::Multiply),
        "Screen" => Ok(BlendMode::Screen),
        "Overlay" => Ok(BlendMode::Overlay),
        "Darken" => Ok(BlendMode::Darken),
        "Lighten" => Ok(BlendMode::Lighten),
        "ColorDodge" => Ok(BlendMode::ColorDodge),
        "ColorBurn" => Ok(BlendMode::ColorBurn),
        "HardLight" => Ok(BlendMode::HardLight),
        "SoftLight" => Ok(BlendMode::SoftLight),
        "Difference" => Ok(BlendMode::Difference),
        "Exclusion" => Ok(BlendMode::Exclusion),
        "Hue" => Ok(BlendMode::Hue),
        "Saturation" => Ok(BlendMode::Saturation),
        "Color" => Ok(BlendMode::Color),
        "Luminosity" => Ok(BlendMode::Luminosity),
        _ => Err(invalid_ext_gstate_entry_value(
            "BM",
            format!("unsupported blend mode '{s}'"),
        )),
    }
}

fn parse_blend_mode(
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
    let blend_modes_vec: Vec<BlendMode> = if value.is_array() {
        value
            .try_array(objects)?
            .iter()
            .map(|obj| to_blend_mode(obj.try_str(objects)?.as_ref()))
            .collect::<Result<Vec<BlendMode>, _>>()?
    } else {
        let mode = to_blend_mode(value.try_str(objects)?.as_ref())?;
        vec![mode]
    };

    Ok(ExternalGraphicsStateKey::BlendMode(blend_modes_vec))
}

fn parse_soft_mask(
    key_name: &str,
    value: &ObjectVariant,
    objects: &dyn ObjectResolver,
    cache: &mut dyn ResourceCache,
    cycle_tracker: &mut ReadCycleTracker,
    id_allocator: &mut ContentStreamIdAllocator,
) -> Result<ExternalGraphicsStateKey, PdfPagesError> {
    let smask = match value {
        ObjectVariant::Dictionary(dict) => {
            let mask_type = parse_mask_mode(dict.get_or_err("S")?.try_str(objects)?.as_ref())?;

            // Parse the "G" key for the `XObject`
            let stream = dict.get_or_err("G")?.try_stream(objects)?;

            let shape = match XObject::read_xobject(
                &ObjectVariant::Stream(stream.clone()),
                &stream.dictionary,
                stream,
                objects,
                cache,
                cycle_tracker,
                id_allocator,
            ) {
                Ok(shape) => shape,
                Err(err) if err.is_cyclic_dependency() => {
                    return Ok(ExternalGraphicsStateKey::SoftMask(None));
                }
                Err(err) => return Err(err),
            };

            Some(Box::new(SoftMask { mask_type, shape }))
        }
        other => match other.try_str(objects)?.as_ref() {
            "None" => None,
            _ => {
                return Err(invalid_ext_gstate_entry_value(
                    key_name,
                    "expected a soft mask dictionary or the name 'None'",
                ));
            }
        },
    };

    Ok(ExternalGraphicsStateKey::SoftMask(smask))
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
        "D" => parse_dash_pattern(name, value, objects)?,
        "RI" => ExternalGraphicsStateKey::RenderingIntent(value.try_str(objects)?.to_string()),
        "OP" => ExternalGraphicsStateKey::OverprintStroke(value.try_boolean(objects)?),
        "op" => ExternalGraphicsStateKey::OverprintFill(value.try_boolean(objects)?),
        "OPM" => ExternalGraphicsStateKey::OverprintMode(value.try_number::<i32>(objects)?),
        "Font" => parse_font(name, value, objects)?,
        "BM" => parse_blend_mode(value, objects)?,
        "SMask" => parse_soft_mask(name, value, objects, cache, cycle_tracker, id_allocator)?,
        "CA" => ExternalGraphicsStateKey::StrokingAlpha(value.try_number::<f32>(objects)?),
        "ca" => ExternalGraphicsStateKey::NonStrokingAlpha(value.try_number::<f32>(objects)?),
        "SA" => ExternalGraphicsStateKey::StrokeAdjustment(value.try_boolean(objects)?),
        "AAPL:AA" => ExternalGraphicsStateKey::AppleAntiAliasing(value.try_boolean(objects)?),
        "AIS" => ExternalGraphicsStateKey::AlphaIsShape(value.try_boolean(objects)?),
        _ => return Ok(None),
    };

    Ok(Some(parsed))
}
