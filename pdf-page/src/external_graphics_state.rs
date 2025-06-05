use std::str::FromStr;

use crate::error::PageError;
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
};

/// Represents the standard blend modes allowed in PDF.
/// Reference: PDF 32000-1:2008, Tables 72 & 73.
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

    type ErrorType = PageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        _objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let mut params: Vec<ExternalGraphicsStateKey> = Vec::new();

        for (name, pdf_obj_value) in &dictionary.dictionary {
            let key_variant = match name.as_str() {
                // Line width (number)
                "LW" => ExternalGraphicsStateKey::LineWidth(pdf_obj_value.as_number::<f32>()?),
                // Line cap style (integer)
                // Note: The PDF spec defines these as integers 0, 1, 2.
                // Consider creating enums for LineCap and LineJoin for type safety.
                "LC" => ExternalGraphicsStateKey::LineCap(pdf_obj_value.as_number::<i32>()?),
                // Line join style (integer)
                "LJ" => ExternalGraphicsStateKey::LineJoin(pdf_obj_value.as_number::<i32>()?),
                // Miter limit (number)
                "ML" => ExternalGraphicsStateKey::MiterLimit(pdf_obj_value.as_number::<f32>()?),
                // Dash pattern (array and number)
                "D" => {
                    let arr = pdf_obj_value.as_array().unwrap();
                    // TODO: Replace unwrap with ok_or_else for robust error handling
                    // e.g., .ok_or_else(|| PageError::General("Dash pattern /D expects an array".to_string()))?;
                    if arr.0.len() != 2 {
                        panic!("Dash pattern /D expects an array with 2 elements");
                    }
                    let dash_array_obj = arr.0[0].as_array().unwrap(); // TODO: Replace unwrap
                    let dash_array_f32 = dash_array_obj
                        .0
                        .iter()
                        .map(|obj| obj.as_number::<f32>())
                        .collect::<Result<Vec<f32>, _>>()?;

                    let dash_phase = arr.0[1].as_number::<f32>()?; // TODO: Replace unwrap if as_number can fail other than type
                    ExternalGraphicsStateKey::DashPattern(dash_array_f32, dash_phase)
                }
                // Rendering intent (name)
                "RI" => ExternalGraphicsStateKey::RenderingIntent(
                    pdf_obj_value.as_str().unwrap().to_string(),
                ),
                // Overprint for stroke (boolean)
                "OP" => {
                    ExternalGraphicsStateKey::OverprintStroke(pdf_obj_value.as_boolean().unwrap())
                }
                // Overprint for fill (boolean)
                "op" => {
                    ExternalGraphicsStateKey::OverprintFill(pdf_obj_value.as_boolean().unwrap())
                }
                // Overprint mode (integer)
                "OPM" => ExternalGraphicsStateKey::OverprintMode(pdf_obj_value.as_number::<i32>()?),
                // Font (array: [font reference, size])
                "Font" => {
                    let arr = pdf_obj_value.as_array().unwrap();
                    // TODO: Replace unwrap with ok_or_else
                    if arr.0.len() != 2 {
                        panic!("Font entry /Font expects an array with 2 elements");
                    }
                    let font_ref = arr.0[0].as_object().unwrap(); // TODO: Replace unwrap
                    let font_size = arr.0[1].as_number::<f32>()?; // TODO: Replace unwrap if applicable
                    ExternalGraphicsStateKey::Font(font_ref.object_number(), font_size)
                }
                // Blend mode (name or array of names)
                "BM" => {
                    let blend_modes_vec: Vec<BlendMode>;
                    if let Some(name_str) = pdf_obj_value.as_str() {
                        // Assumes as_str() is suitable for PDF Names
                        let mode = name_str.parse::<BlendMode>().unwrap();
                        blend_modes_vec = vec![mode];
                    } else if let Some(pdf_array) = pdf_obj_value.as_array() {
                        blend_modes_vec = pdf_array
                            .0
                            .iter()
                            .map(|obj| {
                                let name_str = obj.as_str().unwrap();
                                name_str.parse::<BlendMode>().unwrap()
                            })
                            .collect::<Vec<BlendMode>>();
                    } else {
                        panic!(
                            "Blend mode /BM expects a Name or an Array of Names. Found unexpected type."
                        );
                    }
                    ExternalGraphicsStateKey::BlendMode(blend_modes_vec)
                }
                // Soft mask (dictionary or name)
                "SMask" => {
                    panic!()
                    // if pdf_obj_value.is_name() {
                    //     if pdf_obj_value.as_name_str()? == "None" {
                    //         ExtGStateKey::SoftMask(None)
                    //     } else {
                    //         return Err(PageError::General(format!(
                    //             "Invalid name for /SMask: {}",
                    //             pdf_obj_value.as_name_str()?
                    //         )));
                    //     }
                    // } else if pdf_obj_value.is_dictionary() {
                    //     // Cloning the dictionary. Consider if a reference or more detailed parsing is needed.
                    //     ExtGStateKey::SoftMask(Some(pdf_obj_value.as_dictionary()?.clone()))
                    // } else {
                    //     return Err(PageError::General(
                    //         "Soft mask /SMask expects a Dictionary or the Name 'None'"
                    //             .to_string(),
                    //     ));
                    // }
                }
                // Stroking alpha constant (number)
                "CA" => ExternalGraphicsStateKey::StrokingAlpha(pdf_obj_value.as_number::<f32>()?),
                // Nonstroking alpha constant (number)
                "ca" => {
                    ExternalGraphicsStateKey::NonStrokingAlpha(pdf_obj_value.as_number::<f32>()?)
                }
                // Add other ExtGState parameters as needed
                unknown_key => {
                    // It's generally better to ignore unknown keys or log a warning
                    // than to panic, as PDF files can contain custom keys.
                    // For now, let's keep the panic to match original behavior for unhandled knowns.
                    eprintln!(
                        "Warning: Unknown ExtGState parameter encountered: {}",
                        unknown_key
                    );
                    continue; // Skip unknown keys
                }
            };
            params.push(key_variant);
        }

        Ok(ExternalGraphicsState { params })
    }
}
