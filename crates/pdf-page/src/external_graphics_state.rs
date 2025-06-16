use std::str::FromStr;

use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};

use thiserror::Error;

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
    /// Error converting a PDF value to a number.
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },
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

/// Represents a key-value pair from a PDF External Graphics State dictionary (`ExtGState`).
///
/// An `ExtGState` dictionary contains parameters that control the graphics state,
/// such as line styles, color rendering, and alpha transparency. This enum
/// enumerates the possible keys (parameters) found in such a dictionary and
/// holds the corresponding parsed value.
#[derive(Debug, PartialEq)]
pub enum ExternalGraphicsStateKey {
    /// Line width (`LW`). A number specifying the thickness of stroked lines.
    LineWidth(f32),
    /// Line cap style (`LC`). An integer specifying the shape to be used at the ends of open subpaths
    /// when they are stroked (0: butt, 1: round, 2: projecting square).
    LineCap(i32),
    /// Line join style (`LJ`). An integer specifying the shape to be used at the corners of paths
    /// when they are stroked (0: miter, 1: round, 2: bevel).
    LineJoin(i32),
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
    /// The dictionary itself is complex and defines the mask's properties (type, subtype, etc.).
    /// Represented here as an optional `Dictionary`.
    SoftMask(Option<Dictionary>),
    /// Stroking alpha constant (`CA`). A number in the range 0.0 to 1.0 specifying the constant
    /// opacity value to be used for stroking operations.
    StrokingAlpha(f32),
    /// Nonstroking alpha constant (`ca`). A number in the range 0.0 to 1.0 specifying the constant
    /// opacity value to be used for non-stroking operations.
    NonStrokingAlpha(f32),
}

pub struct ExternalGraphicsState {
    pub params: Vec<ExternalGraphicsStateKey>,
}

impl FromDictionary for ExternalGraphicsState {
    const KEY: &'static str = "ExtGState";

    type ResultType = Self;

    type ErrorType = ExternalGraphicsStateError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let mut params: Vec<ExternalGraphicsStateKey> = Vec::new();

        for (name, value) in &dictionary.dictionary {
            let key_variant =
                match name.as_str() {
                    "LW" => ExternalGraphicsStateKey::LineWidth(value.as_number::<f32>().map_err(
                        |e| ExternalGraphicsStateError::NumericConversionError {
                            entry_description: "LW",
                            source: e,
                        },
                    )?),
                    "LC" => ExternalGraphicsStateKey::LineCap(value.as_number::<i32>().map_err(
                        |e| ExternalGraphicsStateError::NumericConversionError {
                            entry_description: "LC",
                            source: e,
                        },
                    )?),
                    "LJ" => ExternalGraphicsStateKey::LineJoin(value.as_number::<i32>().map_err(
                        |e| ExternalGraphicsStateError::NumericConversionError {
                            entry_description: "LJ",
                            source: e,
                        },
                    )?),
                    "ML" => {
                        ExternalGraphicsStateKey::MiterLimit(value.as_number::<f32>().map_err(
                            |e| ExternalGraphicsStateError::NumericConversionError {
                                entry_description: "ML",
                                source: e,
                            },
                        )?)
                    }
                    "D" => {
                        let arr = value.as_array().ok_or(
                            ExternalGraphicsStateError::UnsupportedTypeError {
                                key_name: name.clone(),
                                expected_type: "Array",
                                found_type: format!("{:?}", value),
                            },
                        )?;
                        if arr.len() != 2 {
                            return Err(ExternalGraphicsStateError::InvalidArrayStructureError {
                                key_name: name.clone(),
                                expected_desc: "array with 2 elements",
                                actual_desc: format!("array with {} elements", arr.len()),
                            });
                        }
                        let dash_array_obj = arr[0].as_array().ok_or(
                            ExternalGraphicsStateError::UnsupportedTypeError {
                                key_name: name.clone(),
                                expected_type: "Array",
                                found_type: format!("{:?}", arr[0]),
                            },
                        )?;
                        let dash_array_f32 = dash_array_obj
                            .iter()
                            .map(|obj| {
                                obj.as_number::<f32>().map_err(|e| {
                                    ExternalGraphicsStateError::NumericConversionError {
                                        entry_description: "Dash array",
                                        source: e,
                                    }
                                })
                            })
                            .collect::<Result<Vec<f32>, _>>()?;

                        let dash_phase = arr[1].as_number::<f32>().map_err(|e| {
                            ExternalGraphicsStateError::NumericConversionError {
                                entry_description: "Dash phase",
                                source: e,
                            }
                        })?;
                        ExternalGraphicsStateKey::DashPattern(dash_array_f32, dash_phase)
                    }
                    "RI" => ExternalGraphicsStateKey::RenderingIntent(
                        value
                            .as_str()
                            .ok_or(ExternalGraphicsStateError::UnsupportedTypeError {
                                key_name: name.clone(),
                                expected_type: "String",
                                found_type: format!("{:?}", value),
                            })?
                            .to_string(),
                    ),
                    "OP" => ExternalGraphicsStateKey::OverprintStroke(value.as_boolean().ok_or(
                        ExternalGraphicsStateError::UnsupportedTypeError {
                            key_name: name.clone(),
                            expected_type: "Boolean",
                            found_type: format!("{:?}", value),
                        },
                    )?),
                    "op" => ExternalGraphicsStateKey::OverprintFill(value.as_boolean().ok_or(
                        ExternalGraphicsStateError::UnsupportedTypeError {
                            key_name: name.clone(),
                            expected_type: "Boolean",
                            found_type: format!("{:?}", value),
                        },
                    )?),
                    "OPM" => {
                        ExternalGraphicsStateKey::OverprintMode(value.as_number::<i32>().map_err(
                            |e| ExternalGraphicsStateError::NumericConversionError {
                                entry_description: "OPM",
                                source: e,
                            },
                        )?)
                    }
                    "Font" => {
                        let arr = value.as_array().ok_or(
                            ExternalGraphicsStateError::UnsupportedTypeError {
                                key_name: name.clone(),
                                expected_type: "Array",
                                found_type: format!("{:?}", value),
                            },
                        )?;
                        if arr.len() != 2 {
                            return Err(ExternalGraphicsStateError::InvalidArrayStructureError {
                                key_name: name.clone(),
                                expected_desc: "array with 2 elements",
                                actual_desc: format!("array with {} elements", arr.len()),
                            });
                        }
                        let font_ref = arr[0].as_object().ok_or(
                            ExternalGraphicsStateError::UnsupportedTypeError {
                                key_name: name.clone(),
                                expected_type: "Object",
                                found_type: format!("{:?}", arr[0]),
                            },
                        )?;
                        let font_size = arr[1].as_number::<f32>().map_err(|e| {
                            ExternalGraphicsStateError::NumericConversionError {
                                entry_description: "Font size",
                                source: e,
                            }
                        })?;
                        ExternalGraphicsStateKey::Font(font_ref.object_number(), font_size)
                    }
                    "BM" => {
                        let blend_modes_vec: Vec<BlendMode>;
                        if let Some(name_str) = value.as_str() {
                            let mode = name_str.parse::<BlendMode>().map_err(|e| {
                                ExternalGraphicsStateError::BlendModeParseError {
                                    key_name: name.clone(),
                                    value: name_str.to_string(),
                                    source: e,
                                }
                            })?;
                            blend_modes_vec = vec![mode];
                        } else if let Some(pdf_array) = value.as_array() {
                            blend_modes_vec = pdf_array
                                .iter()
                                .map(|obj| {
                                    let name_str = obj.as_str().ok_or(
                                        ExternalGraphicsStateError::UnsupportedTypeError {
                                            key_name: name.clone(),
                                            expected_type: "String",
                                            found_type: format!("{:?}", obj),
                                        },
                                    )?;
                                    name_str.parse::<BlendMode>().map_err(|e| {
                                        ExternalGraphicsStateError::BlendModeParseError {
                                            key_name: name.clone(),
                                            value: name_str.to_string(),
                                            source: e,
                                        }
                                    })
                                })
                                .collect::<Result<Vec<BlendMode>, _>>()?;
                        } else {
                            return Err(ExternalGraphicsStateError::UnsupportedTypeError {
                                key_name: name.clone(),
                                expected_type: "Name or Array of Names",
                                found_type: format!("{:?}", value),
                            });
                        }
                        ExternalGraphicsStateKey::BlendMode(blend_modes_vec)
                    }
                    "SMask" => {
                        return Err(ExternalGraphicsStateError::UnsupportedTypeError {
                            key_name: name.clone(),
                            expected_type: "Dictionary or Name",
                            found_type: format!("{:?}", value),
                        });
                    }
                    "CA" => {
                        ExternalGraphicsStateKey::StrokingAlpha(value.as_number::<f32>().map_err(
                            |e| ExternalGraphicsStateError::NumericConversionError {
                                entry_description: "CA",
                                source: e,
                            },
                        )?)
                    }
                    "ca" => ExternalGraphicsStateKey::NonStrokingAlpha(
                        value.as_number::<f32>().map_err(|e| {
                            ExternalGraphicsStateError::NumericConversionError {
                                entry_description: "ca",
                                source: e,
                            }
                        })?,
                    ),
                    unknown_key => {
                        eprintln!(
                            "Warning: Unknown ExtGState parameter encountered: {}",
                            unknown_key
                        );
                        continue;
                    }
                };
            params.push(key_variant);
        }

        Ok(ExternalGraphicsState { params })
    }
}
