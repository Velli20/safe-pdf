use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
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
    #[error("Invalid /ShadingType value")]
    InvalidShadingType,
    #[error("Invalid value for key: {0}")]
    InvalidValue(String),
    #[error("Invalid type for entry '{entry_name}': expected {expected_type}, found {found_type}")]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
    #[error("Missing required key: {0}")]
    MissingKey(&'static str),
    #[error("Other error: {0}")]
    Other(String),

    #[error("Error parsing /BBox array: Expected 4 elements, got {count}")]
    InvalidElementCount { count: usize },

    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },

    #[error("Error parsing Function: {0}")]
    FunctionReadError(#[from] FunctionReadError),
    #[error("Error parsing Dictionary: {0}")]
    ObjectError(#[from] ObjectError),
}

/// ShadingType as defined in PDF 1.7 Table 106
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

impl ShadingType {
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

/// Represents a Shading object.
#[derive(Debug)]
pub enum Shading {
    FunctionBased {
        color_space: Option<String>,
        background: Option<Vec<f32>>,
        bbox: Option<Vec<f32>>,
        anti_alias: Option<bool>,
        domain: Option<Vec<f32>>,
        functions: Option<Vec<i32>>, // Should be Function objects, simplified here
    },
    Axial {
        color_space: Option<ColorSpace>,
        // background: Option<Vec<f32>>,
        // bbox: Option<Vec<f32>>,
        // anti_alias: Option<bool>,
        coords: [f32; 4],
        // domain: Option<[f32; 2]>,
        function: Function,
        // extend: Option<[bool; 2]>,
    },
    Radial {
        color_space: Option<ColorSpace>,
        coords: [f32; 6],
        function: Function,
        // TODO background: Option<Vec<f32>>,
        // TODO bbox: Option<Vec<f32>>,
        // TODO anti_alias: Option<bool>,
        // TODO domain: Option<[f32; 2]>,
        // TODO extend: Option<[bool; 2]>,
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
        let shading_type = dictionary
            .get("ShadingType")
            .ok_or(ShadingError::MissingShadingType)?;

        let shading_type_int = shading_type
            .as_number::<i32>()
            .map_err(|_| ShadingError::InvalidShadingType)?;

        match ShadingType::from_i32(shading_type_int) {
            Some(ShadingType::FunctionBased) => {
                // Parse fields for FunctionBased shading as needed
                todo!()
            }
            Some(ShadingType::Axial) => {
                let coords = dictionary
                    .get("Coords")
                    .ok_or(ShadingError::InvalidShadingType)?;

                let coords = coords.as_array_of::<f32, 4>()?;

                let color_space = if let Some(obj) = dictionary.get("ColorSpace") {
                    Some(ColorSpace::from(obj.as_ref()))
                } else {
                    None
                };

                if let Some(bg) = dictionary.get("Background") {
                    println!("Background: {bg:?}");
                }
                if let Some(bg) = dictionary.get("Domain") {
                    println!("Domain: {bg:?}");
                }

                let function = if let Some(f) = dictionary.get_dictionary("Function") {
                    Function::from_dictionary(f, objects)?
                } else {
                    panic!("No function found");
                };

                Ok(Shading::Axial {
                    color_space,
                    function,
                    coords,
                })
            }
            Some(ShadingType::Radial) => {
                println!("Radial {:?}", dictionary);

                let coords = dictionary
                    .get("Coords")
                    .ok_or(ShadingError::InvalidShadingType)?;

                let coords = coords.as_array_of::<f32, 6>()?;

                let color_space = if let Some(obj) = dictionary.get("ColorSpace") {
                    Some(ColorSpace::from(obj.as_ref()))
                } else {
                    None
                };

                let function = if let Some(dictionary) = dictionary.get_dictionary("Function") {
                    Function::from_dictionary(dictionary, objects)?
                } else {
                    panic!("No function found");
                };

                Ok(Shading::Radial {
                    color_space,
                    function,
                    coords,
                })
            }
            _ => Err(ShadingError::InvalidShadingType),
        }
    }
}
