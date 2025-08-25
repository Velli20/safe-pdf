use std::str::FromStr;

use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use thiserror::Error;

use crate::xobject::{XObject, XObjectError, XObjectReader};
use num_traits::FromPrimitive;
use pdf_graphics::{LineCap, LineJoin};

/// Errors that can occur during parsing of an External Graphics State dictionary.
#[derive(Error, Debug)]
pub enum ExternalGraphicsStateError {
    #[error("Failed to parse blend mode string '{value}' for key '{key_name}': {source}")]
    BlendModeParseError {
        key_name: String,
        value: String,
        source: ParseBlendModeError,
    },
    #[error(
        "Invalid array structure for key '{key_name}': expected {expected_desc}, found {actual_desc}"
    )]
    InvalidArrayStructureError {
        key_name: String,
        expected_desc: &'static str,
        actual_desc: String,
    },
    #[error("Invalid value for key '{key_name}': {description}")]
    InvalidValueError {
        key_name: String,
        description: String,
    },
    #[error(
        "Unsupported PDF object type for key '{key_name}': expected {expected_type}, found {found_type}"
    )]
    UnsupportedTypeError {
        key_name: String,
        expected_type: &'static str,
        found_type: String,
    },
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
    #[error("Error parsing Dash Array: {0}")]
    DashArrayParsingError(#[source] ObjectError),
    #[error("Error reading Soft Mask XObject: {0}")]
    SMaskReadError(#[from] XObjectError),
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
}

/// Type of soft mask specified by an ExtGState `SMask` dictionary.
///
/// This corresponds to the value of the `S` entry in the soft mask
/// dictionary (PDF 1.4+, Transparency). It determines how the mask is
/// derived from the associated `G` transparency group XObject.
pub enum MaskType {
    /// Derive the mask from the luminance (perceived brightness) of the
    /// transparency group's colors. The color components are converted to a
    /// single grayscale value; the group's alpha is ignored.
    Luminosity,
    /// Derive the mask from the alpha (shape/opacity) values of the
    /// transparency group. The group's color is ignored; only its opacity
    /// contributes to the mask.
    Alpha,
}

/// Represents the standard blend modes allowed in PDF.
#[derive(Debug, PartialEq, Clone)]
pub enum BlendMode {
    // Standard separable blend modes
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    // Standard nonseparable blend modes
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// Error type for blend mode parsing.
#[derive(Debug, PartialEq)]
pub struct ParseBlendModeError {
    invalid_value: String,
}

impl std::fmt::Display for ParseBlendModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown blend mode: '{}'", self.invalid_value)
    }
}
impl std::error::Error for ParseBlendModeError {}

impl FromStr for BlendMode {
    type Err = ParseBlendModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
            _ => Err(ParseBlendModeError {
                invalid_value: s.to_string(),
            }),
        }
    }
}

pub struct SoftMask {
    pub mask_type: MaskType,
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
    Font(i32, f32),
    /// Blend mode (`BM`). A name or array of names specifying the blend mode to be used
    /// when compositing objects.
    BlendMode(Vec<BlendMode>),
    /// Soft mask (`SMask`). A dictionary specifying the soft mask to be used, or the name `None`.
    SoftMask(Option<SoftMask>),
    /// Stroking alpha constant (`CA`). A number in the range 0.0 to 1.0 specifying the constant
    /// opacity value to be used for stroking operations.
    StrokingAlpha(f32),
    /// Nonstroking alpha constant (`ca`). A number in the range 0.0 to 1.0 specifying the constant
    /// opacity value to be used for non-stroking operations.
    NonStrokingAlpha(f32),
    /// Stroke adjustment (`SA`). A boolean that specifies whether to adjust stroke endpoints
    /// and joins to the device pixel grid to produce thinner or more consistent strokes.
    StrokeAdjustment(bool),
}

pub struct ExternalGraphicsState {
    pub params: Vec<ExternalGraphicsStateKey>,
}

impl FromDictionary for ExternalGraphicsState {
    const KEY: &'static str = "ExtGState";

    type ResultType = Self;

    type ErrorType = ExternalGraphicsStateError;

    /// Parse an ExtGState dictionary into a strongly-typed `ExternalGraphicsState`.
    ///
    /// This delegates each key's parsing to small helpers to keep control flow
    /// and error handling readable. Unknown keys are logged and skipped.
    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let mut params: Vec<ExternalGraphicsStateKey> = Vec::new();

        for (name, value) in &dictionary.dictionary {
            match parse_entry(name, value, objects)? {
                Some(key) => params.push(key),
                None => continue, // known-skipped (e.g., Type) or unknown already logged
            }
        }

        Ok(ExternalGraphicsState { params })
    }
}

/// Small helper to map numeric conversion errors to a tagged error.
fn f32_or_err(
    value: &ObjectVariant,
    entry: &'static str,
) -> Result<f32, ExternalGraphicsStateError> {
    value
        .as_number::<f32>()
        .map_err(|e| ExternalGraphicsStateError::NumericConversionError {
            entry_description: entry,
            source: e,
        })
}

/// Small helper to map numeric conversion errors to a tagged error.
fn i32_or_err(
    value: &ObjectVariant,
    entry: &'static str,
) -> Result<i32, ExternalGraphicsStateError> {
    value
        .as_number::<i32>()
        .map_err(|e| ExternalGraphicsStateError::NumericConversionError {
            entry_description: entry,
            source: e,
        })
}

/// Parse the dash pattern array `D` -> DashPattern(Vec<f32>, phase)
fn parse_dash_pattern(
    key_name: &str,
    value: &ObjectVariant,
) -> Result<ExternalGraphicsStateKey, ExternalGraphicsStateError> {
    let arr = value.try_array()?;
    if arr.len() != 2 {
        return Err(ExternalGraphicsStateError::InvalidArrayStructureError {
            key_name: key_name.to_string(),
            expected_desc: "array with 2 elements",
            actual_desc: format!("array with {} elements", arr.len()),
        });
    }

    let dash_array_f32 = arr[0]
        .as_vec_of::<f32>()
        .map_err(ExternalGraphicsStateError::DashArrayParsingError)?;
    let dash_phase = f32_or_err(&arr[1], "Dash phase")?;
    Ok(ExternalGraphicsStateKey::DashPattern(
        dash_array_f32,
        dash_phase,
    ))
}

/// Parse font tuple `Font` -> Font(object_ref, size)
fn parse_font(
    key_name: &str,
    value: &ObjectVariant,
) -> Result<ExternalGraphicsStateKey, ExternalGraphicsStateError> {
    let arr = value.try_array()?;
    if arr.len() != 2 {
        return Err(ExternalGraphicsStateError::InvalidArrayStructureError {
            key_name: key_name.to_string(),
            expected_desc: "array with 2 elements",
            actual_desc: format!("array with {} elements", arr.len()),
        });
    }
    let font_ref = arr[0].try_reference()?;
    let font_size = f32_or_err(&arr[1], "Font size")?;
    Ok(ExternalGraphicsStateKey::Font(font_ref, font_size))
}

/// Parse blend modes `BM` -> BlendMode(Vec<BlendMode>)
fn parse_blend_mode(
    key_name: &str,
    value: &ObjectVariant,
) -> Result<ExternalGraphicsStateKey, ExternalGraphicsStateError> {
    let blend_modes_vec: Vec<BlendMode> = if let Some(name_str) = value.as_str() {
        let mode = name_str.parse::<BlendMode>().map_err(|e| {
            ExternalGraphicsStateError::BlendModeParseError {
                key_name: key_name.to_string(),
                value: name_str.to_string(),
                source: e,
            }
        })?;
        vec![mode]
    } else if let Some(pdf_array) = value.as_array() {
        pdf_array
            .iter()
            .map(|obj| {
                let name_str = obj.try_str()?;
                name_str.parse::<BlendMode>().map_err(|e| {
                    ExternalGraphicsStateError::BlendModeParseError {
                        key_name: key_name.to_string(),
                        value: name_str.to_string(),
                        source: e,
                    }
                })
            })
            .collect::<Result<Vec<BlendMode>, _>>()?
    } else {
        return Err(ExternalGraphicsStateError::UnsupportedTypeError {
            key_name: key_name.to_string(),
            expected_type: "Name or Array of Names",
            found_type: format!("{:?}", value),
        });
    };
    Ok(ExternalGraphicsStateKey::BlendMode(blend_modes_vec))
}

/// Parse the soft mask `SMask` -> SoftMask(Option<SoftMask>)
fn parse_soft_mask(
    key_name: &str,
    value: &ObjectVariant,
    objects: &ObjectCollection,
) -> Result<ExternalGraphicsStateKey, ExternalGraphicsStateError> {
    let smask = match value {
        ObjectVariant::Dictionary(dict) => {
            let mask_type_str = dict
                .get("S")
                .ok_or_else(|| ExternalGraphicsStateError::InvalidValueError {
                    key_name: key_name.to_string(),
                    description: "SMask must be 'None'".to_string(),
                })?
                .try_str()?;

            let mask_type = match mask_type_str.as_ref() {
                "Luminosity" => MaskType::Luminosity,
                "Alpha" => MaskType::Alpha,
                other => {
                    return Err(ExternalGraphicsStateError::InvalidValueError {
                        key_name: key_name.to_string(),
                        description: format!("Unknown SMask type '{}'", other),
                    });
                }
            };

            // Parse the "G" key for the XObject (required)
            let shape_obj =
                dict.get("G")
                    .ok_or_else(|| ExternalGraphicsStateError::InvalidValueError {
                        key_name: key_name.to_string(),
                        description: "SMask dictionary missing 'G' key".to_string(),
                    })?;

            let smask_xobject = objects.resolve_stream(shape_obj)?;

            let shape = XObject::read_xobject(
                &smask_xobject.dictionary,
                smask_xobject.data.as_slice(),
                objects,
            )
            .map_err(|e| ExternalGraphicsStateError::InvalidValueError {
                key_name: key_name.to_string(),
                description: format!("Failed to parse SMask XObject: {:?}", e),
            })?;

            Some(SoftMask { mask_type, shape })
        }
        other => {
            if let Some(name_str) = other.as_str() {
                if name_str == "None" {
                    None
                } else {
                    return Err(ExternalGraphicsStateError::InvalidValueError {
                        key_name: key_name.to_string(),
                        description: "SMask must be 'None'".to_string(),
                    });
                }
            } else {
                return Err(ExternalGraphicsStateError::UnsupportedTypeError {
                    key_name: key_name.to_string(),
                    expected_type: "Name or Dictionary",
                    found_type: format!("{:?}", value),
                });
            }
        }
    };

    Ok(ExternalGraphicsStateKey::SoftMask(smask))
}

/// Parse a single key/value pair of the ExtGState dictionary.
fn parse_entry(
    name: &str,
    value: &ObjectVariant,
    objects: &ObjectCollection,
) -> Result<Option<ExternalGraphicsStateKey>, ExternalGraphicsStateError> {
    let parsed = match name {
        // Stroke/Fill geometry
        "LW" => Some(ExternalGraphicsStateKey::LineWidth(f32_or_err(
            value, "LW",
        )?)),
        "LC" => {
            let cap_val = i32_or_err(value, "LC")?;
            let cap = LineCap::from_i32(cap_val).ok_or_else(|| {
                ExternalGraphicsStateError::InvalidValueError {
                    key_name: name.to_string(),
                    description: format!("Invalid LineCap value: {}", cap_val),
                }
            })?;
            Some(ExternalGraphicsStateKey::LineCap(cap))
        }
        "LJ" => {
            let join_val = i32_or_err(value, "LJ")?;
            let join = LineJoin::from_i32(join_val).ok_or_else(|| {
                ExternalGraphicsStateError::InvalidValueError {
                    key_name: name.to_string(),
                    description: format!("Invalid LineJoin value: {}", join_val),
                }
            })?;
            Some(ExternalGraphicsStateKey::LineJoin(join))
        }
        "ML" => Some(ExternalGraphicsStateKey::MiterLimit(f32_or_err(
            value, "ML",
        )?)),
        "D" => Some(parse_dash_pattern(name, value)?),

        // Color rendering
        "RI" => Some(ExternalGraphicsStateKey::RenderingIntent(
            value.try_str()?.to_string(),
        )),

        // Overprint
        "OP" => Some(ExternalGraphicsStateKey::OverprintStroke(
            value.try_boolean()?,
        )),
        "op" => Some(ExternalGraphicsStateKey::OverprintFill(
            value.try_boolean()?,
        )),
        "OPM" => Some(ExternalGraphicsStateKey::OverprintMode(i32_or_err(
            value, "OPM",
        )?)),

        // Font
        "Font" => Some(parse_font(name, value)?),

        // Compositing
        "BM" => Some(parse_blend_mode(name, value)?),
        "SMask" => Some(parse_soft_mask(name, value, objects)?),

        // Transparency
        "CA" => Some(ExternalGraphicsStateKey::StrokingAlpha(f32_or_err(
            value, "CA",
        )?)),
        "ca" => Some(ExternalGraphicsStateKey::NonStrokingAlpha(f32_or_err(
            value, "ca",
        )?)),
        // Stroke adjustment
        "SA" => Some(ExternalGraphicsStateKey::StrokeAdjustment(
            value.try_boolean()?,
        )),

        // Meta
        "Type" => None,

        // Unknown keys: treat as an error to surface malformed/unsupported content
        _ => {
            return Err(ExternalGraphicsStateError::InvalidValueError {
                key_name: name.to_string(),
                description: format!("Unknown ExtGState parameter '{}'", name),
            });
        }
    };

    Ok(parsed)
}
