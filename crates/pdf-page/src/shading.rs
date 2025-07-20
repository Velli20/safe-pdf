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
        bbox: Option<[f32; 4]>,
        anti_alias: Option<bool>,
        domain: Option<Vec<f32>>,
        functions: Option<Vec<Function>>,
    },
    Axial {
        color_space: Option<ColorSpace>,
        coords: [f32; 4],
        function: Function,
    },
    Radial {
        color_space: Option<ColorSpace>,
        coords: [f32; 6],
        function: Function,
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
        println!("ShadingType: {shading_type_int:?}");
        match ShadingType::from_i32(shading_type_int) {
            Some(ShadingType::FunctionBased) => {
                // Parse /ColorSpace (optional, as String for now)
                let color_space = dictionary
                    .get("ColorSpace")
                    .and_then(|obj| obj.as_str().map(|s| s.to_string()));

                // Parse /Background (optional, array of numbers)
                let background = dictionary
                    .get("Background")
                    .and_then(|obj| obj.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_number::<f32>().ok())
                            .collect::<Vec<_>>()
                    });

                // Parse /BBox (optional, array of 4 numbers)
                let bbox = if let Some(obj) = dictionary.get("BBox") {
                    Some(obj.as_array_of::<f32, 4>()?)
                } else {
                    None
                };

                // Parse /AntiAlias (optional, bool)
                // let anti_alias = dictionary
                //     .get("AntiAlias")
                //     .and_then(|obj| obj.as_bool().ok());

                // Parse /Domain (optional, array of numbers)
                let domain = dictionary
                    .get("Domain")
                    .and_then(|obj| obj.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_number::<f32>().ok())
                            .collect::<Vec<_>>()
                    });

                if let Some(fun) = dictionary.get("Function") {
                    println!("Function: {fun:?}");
                }

                // Parse /Function (required, can be a reference or array of references)
                let functions = match dictionary.get("Function") {
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
                                    _ => {
                                        return Err(ShadingError::InvalidValue(
                                            "Function".to_string(),
                                        ));
                                    }
                                }
                            }
                        }
                        Some(functions)
                    }
                    Some(obj) => {
                        let value = objects.resolve_dictionary(obj.as_ref())?;
                        let function = match objects.resolve_object(obj)? {
                            ObjectVariant::Dictionary(dictionary) => {
                                Function::from_dictionary(dictionary, objects, None)?
                            }
                            ObjectVariant::Stream(stream) => Function::from_dictionary(
                                &stream.dictionary,
                                objects,
                                Some(&stream.data),
                            )?,
                            _ => {
                                return Err(ShadingError::InvalidValue("Function".to_string()));
                            }
                        };

                        Some(vec![function])
                    }
                    None => None,
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
                    Function::from_dictionary(f, objects, None)?
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
                    Function::from_dictionary(dictionary, objects, None)?
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
