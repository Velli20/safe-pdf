use pdf_graphics::color::Color;
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    color_space::ColorSpace,
    function::{Function, FunctionReadError},
};

/// Errors that can occur while parsing a Shading object.
#[derive(Debug, Error)]
pub enum ShadingError {
    #[error("Missing /ShadingType key")]
    MissingShadingType,
    #[error("Unsupported /ShadingType value: {0}")]
    UnsupportedShadingType(ShadingType),
    #[error("Unknown /ShadingType value: {0}")]
    InvalidShadingType(i32),
    #[error("Invalid type for entry '{entry_name}': expected {expected_type}, found {found_type}")]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Missing required entry in Shading: /{0}")]
    MissingRequiredEntry(&'static str),
    #[error("Error parsing Function: {0}")]
    FunctionReadError(#[from] FunctionReadError),
    #[error("Error parsing Dictionary: {0}")]
    ObjectError(#[from] ObjectError),
}

/// Represents the `/ShadingType` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingType {
    FunctionBased = 1,
    Axial = 2,
    Radial = 3,
    FreeFormTriangleMesh = 4,
    LatticeFormTriangleMesh = 5,
    CoonsPatchMesh = 6,
    TensorProductPatchMesh = 7,
}

impl std::fmt::Display for ShadingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShadingType::FunctionBased => write!(f, "FunctionBased"),
            ShadingType::Axial => write!(f, "Axial"),
            ShadingType::Radial => write!(f, "Radial"),
            ShadingType::FreeFormTriangleMesh => write!(f, "FreeFormTriangleMesh"),
            ShadingType::LatticeFormTriangleMesh => write!(f, "LatticeFormTriangleMesh"),
            ShadingType::CoonsPatchMesh => write!(f, "CoonsPatchMesh"),
            ShadingType::TensorProductPatchMesh => write!(f, "TensorProductPatchMesh"),
        }
    }
}

impl ShadingType {
    /// Attempts to create a `ShadingType` from an integer value, returning `None` if the
    /// value is not a valid tiling type.
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(ShadingType::FunctionBased),
            2 => Some(ShadingType::Axial),
            3 => Some(ShadingType::Radial),
            4 => Some(ShadingType::FreeFormTriangleMesh),
            5 => Some(ShadingType::LatticeFormTriangleMesh),
            6 => Some(ShadingType::CoonsPatchMesh),
            7 => Some(ShadingType::TensorProductPatchMesh),
            _ => None,
        }
    }
}

pub struct ColorStops {
    pub colors: Vec<Color>,
    pub positions: Vec<f32>,
}

impl ColorStops {
    pub fn from(function: &Function) -> Self {
        // Number of stops to sample
        let num_stops = 16;
        let mut positions = vec![];
        let mut colors = vec![];
        for i in 0..num_stops {
            let t = i as f32 / num_stops as f32;
            // Map t to the function's domain
            let domain = function.domain().unwrap_or([0.0, 1.0]);
            let x = domain[0] + t * (domain[1] - domain[0]);
            // Evaluate the function
            let color_components = function
                .interpolate(x)
                .unwrap_or_else(|_| vec![0.0, 0.0, 0.0]);
            // Convert to Color.
            let color = Color::from_rgb(
                color_components.first().copied().unwrap_or(0.0),
                color_components.get(1).copied().unwrap_or(0.0),
                color_components.get(2).copied().unwrap_or(0.0),
            );
            positions.push(t);
            colors.push(color);
        }
        Self { colors, positions }
    }
}

/// Represents a PDF Shading object, which defines a smooth transition between colors
/// across an area, used for creating gradient fills.
#[derive(Debug)]
pub enum Shading {
    /// A function-based shading, where the color at every point is defined
    /// by a mathematical function of its coordinates.
    FunctionBased {
        /// The color space in which the function's results are interpreted.
        color_space: Option<ColorSpace>,
        /// An array of color components specifying a background color.
        background: Option<Vec<f32>>,
        /// A rectangle specifying the domain of the shading.
        bbox: Option<[f32; 4]>,
        /// A flag indicating whether to apply anti-aliasing.
        anti_alias: Option<bool>,
        /// The domain of the function(s).
        domain: Option<Vec<f32>>,
        /// A 2-in, n-out function or an array of n 2-in, 1-out functions
        /// that define the color at each point.
        functions: Vec<Function>,
    },
    /// An axial shading, where color transitions along a line between
    /// two points, extending infinitely perpendicular to that line.
    Axial {
        /// The color space in which color values are expressed.
        color_space: ColorSpace,
        /// An array of four numbers `[x0, y0, x1, y1]` specifying the
        /// starting and ending coordinates of the axis.
        coords: [f32; 4],
        /// A 1-in, n-out function that maps a parameter `t` (from 0.0 to 1.0)
        /// along the axis to a color.
        function: Function,
        colors: Vec<Color>,
        positions: Vec<f32>,
    },
    /// A radial shading, where color transitions between two circles.
    Radial {
        /// The color space in which color values are expressed.
        color_space: ColorSpace,
        /// An array of six numbers `[x0, y0, r0, x1, y1, r1]` specifying
        /// the centers and radii of the starting and ending circles.
        coords: [f32; 6],
        /// A 1-in, n-out function that maps a parameter `t` (from 0.0 to 1.0)
        /// between the circles to a color.
        function: Function,
        colors: Vec<Color>,
        positions: Vec<f32>,
    },
}

impl FromDictionary for Shading {
    const KEY: &'static str = "Shading";
    type ResultType = Self;
    type ErrorType = ShadingError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, ShadingError> {
        // Extract the required `/ShadingType` entry.
        let shading_type = dictionary.get_or_err("ShadingType")?.as_number::<i32>()?;

        match ShadingType::from_i32(shading_type) {
            Some(ShadingType::FunctionBased) => {
                // Read optional `/ColorSpace` entry, which defines the color space for the shading.
                let color_space = dictionary.get("ColorSpace").map(ColorSpace::from);

                // Read optional `/Background` entry, specifying a background color as an array of numbers.
                let background = dictionary
                    .get("Background")
                    .map(|obj| obj.as_vec_of::<f32>())
                    .transpose()?;

                // Read optional `/BBox` entry, which defines the bounding box for the shading.
                let bbox = dictionary
                    .get("BBox")
                    .map(|obj| obj.as_array_of::<f32, 4>())
                    .transpose()?;

                // Read optional `/Domain` entry, specifying the valid input range for the function(s).
                let domain = dictionary
                    .get("Domain")
                    .map(|obj| obj.as_vec_of::<f32>())
                    .transpose()?;

                // Read required `/Function` entry, which may be a single function or an array of functions.
                let functions = match dictionary.get("Function") {
                    // If the `/Function` is an array, read each function object.
                    Some(obj) if obj.is_array() => {
                        let mut functions = Vec::new();
                        if let Some(obj) = obj.as_array() {
                            for value in obj.iter() {
                                match objects.resolve_object(value)? {
                                    ObjectVariant::Dictionary(dictionary) => {
                                        functions.push(Function::from_dictionary(
                                            dictionary, objects, None,
                                        )?);
                                    }
                                    ObjectVariant::Stream(stream) => {
                                        functions.push(Function::from_dictionary(
                                            &stream.dictionary,
                                            objects,
                                            Some(&stream.data),
                                        )?);
                                    }
                                    obj => {
                                        return Err(ShadingError::InvalidEntryType {
                                            entry_name: "Function",
                                            expected_type: "Dictionary or Stream",
                                            found_type: obj.name(),
                                        });
                                    }
                                }
                            }
                        }
                        functions
                    }
                    // If `/Function` is a single object, read it directly.
                    Some(obj) => {
                        let function = match objects.resolve_object(obj)? {
                            ObjectVariant::Dictionary(dictionary) => {
                                Function::from_dictionary(dictionary, objects, None)?
                            }
                            ObjectVariant::Stream(stream) => Function::from_dictionary(
                                &stream.dictionary,
                                objects,
                                Some(&stream.data),
                            )?,
                            obj => {
                                return Err(ShadingError::InvalidEntryType {
                                    entry_name: "Function",
                                    expected_type: "Dictionary or Stream",
                                    found_type: obj.name(),
                                });
                            }
                        };
                        vec![function]
                    }
                    // `/Function` entry is required for FunctionBased shading.
                    None => return Err(ShadingError::MissingRequiredEntry("Function")),
                };

                Ok(Shading::FunctionBased {
                    color_space,
                    background,
                    bbox,
                    anti_alias: None,
                    domain,
                    functions,
                })
            }
            Some(ShadingType::Axial) => {
                // Read required `/Coords` entry, which defines the axis for the gradient.
                let coords = dictionary.get_or_err("Coords")?;

                let coords = coords.as_array_of::<f32, 4>()?;

                // Read required `/ColorSpace` entry.
                let color_space = dictionary
                    .get("ColorSpace")
                    .ok_or(ShadingError::MissingRequiredEntry("ColorSpace"))
                    .map(ColorSpace::from)?;

                // Read required `/Function` entry as a dictionary.
                let function = if let Some(f) = dictionary
                    .get("Function")
                    .map(|d| d.try_dictionary())
                    .transpose()?
                {
                    Function::from_dictionary(f, objects, None)?
                } else {
                    return Err(ShadingError::MissingRequiredEntry("Function"));
                };

                let ColorStops { colors, positions } = ColorStops::from(&function);

                Ok(Shading::Axial {
                    color_space,
                    function,
                    coords,
                    colors,
                    positions,
                })
            }
            Some(ShadingType::Radial) => {
                // Read required `/Coords` entry, which defines the two circles for the radial gradient.
                let coords = dictionary
                    .get("Coords")
                    .ok_or(ShadingError::MissingRequiredEntry("Coords"))?;

                let coords = coords.as_array_of::<f32, 6>()?;

                // Read required `/ColorSpace` entry.
                let color_space = dictionary
                    .get("ColorSpace")
                    .ok_or(ShadingError::MissingRequiredEntry("ColorSpace"))
                    .map(ColorSpace::from)?;

                // Read required `/Function` entry as a dictionary.
                let function = if let Some(dictionary) = dictionary
                    .get("Function")
                    .map(|d| d.try_dictionary())
                    .transpose()?
                {
                    Function::from_dictionary(dictionary, objects, None)?
                } else {
                    return Err(ShadingError::MissingRequiredEntry("Function"));
                };

                let ColorStops { colors, positions } = ColorStops::from(&function);

                Ok(Shading::Radial {
                    color_space,
                    function,
                    coords,
                    colors,
                    positions,
                })
            }
            // If the shading type is not recognized, return an error.
            _ => Err(ShadingError::InvalidShadingType(shading_type)),
        }
    }
}
