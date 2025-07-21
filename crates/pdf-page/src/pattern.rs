use pdf_object::{
    dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    bbox::{BBox, BBoxReadError},
    external_graphics_state::{ExternalGraphicsState, ExternalGraphicsStateError},
    matrix::{Matrix, MatrixReadError},
    resources::{Resources, ResourcesError},
    shading::{Shading, ShadingError},
};

/// Defines errors that can occur while parsing a Pattern.
#[derive(Debug, Error)]
pub enum PatternError {
    #[error("Missing /PatternType key")]
    MissingPatternType,
    #[error("Missing required entry in Pattern: /{0}")]
    MissingRequiredEntry(&'static str),
    #[error("Failed to convert PDF value to number for '{entry_description}': {source}")]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: pdf_object::error::ObjectError,
    },
    #[error("Invalid integer value for /PatternType value: {0}")]
    InvalidPatternType(i32),
    #[error("Invalid value for key '{key}': {value}")]
    InvalidValue { key: &'static str, value: String },
    #[error("Error parsing /Matrix: {0}")]
    InvalidMatrix(#[from] MatrixReadError),
    #[error("Error parsing /BBox: {0}")]
    InvalidBBox(#[from] BBoxReadError),
    #[error("Failed to parse resources for page: {err}")]
    ResourcesParse { err: Box<ResourcesError> },
    #[error("External Graphics State parsing error: {0}")]
    ExternalGraphicsStateError(#[from] ExternalGraphicsStateError),
    #[error("Shading parsing error: {0}")]
    ShadingError(#[from] ShadingError),
    #[error("{0}")]
    ObjectError(#[from] ObjectError),
}

/// PaintType for tiling patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintType {
    /// Colored tiling pattern.
    Colored = 1,
    /// Uncolored tiling pattern.
    Uncolored = 2,
}

impl PaintType {
    /// Attempts to create a `PaintType` from an integer value, returning `None` if the
    /// value is not a valid paint type.
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(PaintType::Colored),
            2 => Some(PaintType::Uncolored),
            _ => None,
        }
    }
}

/// Represents the type of a PDF Pattern object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternType {
    /// Tiling pattern.
    Tiling = 1,
    /// Shading pattern.
    Shading = 2,
}

impl PatternType {
    /// Attempts to create a `PatternType` from an integer value, returning `None` if the
    /// value is not a valid pattern type.
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(PatternType::Tiling),
            2 => Some(PatternType::Shading),
            _ => None,
        }
    }
}

/// Represents the `/TilingType` entry, which controls the spacing of tiles
/// in a tiling pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingType {
    /// Constant spacing.
    ConstantSpacing = 1,
    /// No distortion.
    NoDistortion = 2,
    /// Constant spacing and faster tiling.
    ConstantSpacingFast = 3,
}

impl TilingType {
    /// Attempts to create a `TilingType` from an integer value, returning `None` if the
    /// value is not a valid tiling type.
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(TilingType::ConstantSpacing),
            2 => Some(TilingType::NoDistortion),
            3 => Some(TilingType::ConstantSpacingFast),
            _ => None,
        }
    }
}

/// Represents a PDF Pattern object, which can be either a tiling pattern or a shading pattern.
///
/// Patterns are used as "colors" for filling or stroking paths, allowing for repeating
/// graphical figures or smooth color transitions (gradients) to be used.
pub enum Pattern {
    /// A tiling pattern, which consists of a small graphical figure (a "pattern cell")
    /// that is replicated at fixed intervals to fill an area.
    Tiling {
        /// Specifies how the pattern's color is determined.
        paint_type: PaintType,
        /// Controls how the spacing of tiles is adjusted.
        tiling_type: TilingType,
        /// The bounding box of the pattern cell, defining its size.
        bbox: BBox,
        /// The horizontal spacing between adjacent tiles.
        x_step: f32,
        /// The vertical spacing between adjacent tiles.
        y_step: f32,
        /// An optional transformation matrix to be applied to the pattern.
        matrix: Option<Matrix>,
        /// A dictionary of resources required by the pattern's content stream.
        resources: Resources,
    },
    /// A shading pattern, which defines a smooth transition between colors across an area.
    Shading {
        /// The shading object that defines the gradient fill.
        shading: Shading,
        /// An optional transformation matrix to be applied to the pattern.
        matrix: Option<Matrix>,
        /// An optional external graphics state to apply when painting the pattern.
        ext_g_state: Option<ExternalGraphicsState>,
    },
}

impl FromDictionary for Pattern {
    const KEY: &'static str = "Pattern";
    type ResultType = Pattern;
    type ErrorType = PatternError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, PatternError> {
        let pattern_type = dictionary
            .get("PatternType")
            .ok_or(PatternError::MissingPatternType)?
            .as_number::<i32>()?;

        // Read the transformation matrix for the pattern. Defaults to identity.
        let matrix = Matrix::from_dictionary(dictionary, objects)?;

        match PatternType::from_i32(pattern_type) {
            Some(PatternType::Tiling) => {
                // Read the `/PaintType` entry.
                let paint_type_int = dictionary
                    .get("PaintType")
                    .ok_or(PatternError::MissingRequiredEntry("PaintType"))?
                    .as_number::<i32>()
                    .map_err(|e| PatternError::NumericConversionError {
                        entry_description: "PaintType",
                        source: e,
                    })?;

                let paint_type = PaintType::from_i32(paint_type_int).ok_or_else(|| {
                    PatternError::InvalidValue {
                        key: "PaintType",
                        value: paint_type_int.to_string(),
                    }
                })?;

                // Read the `/TilingType` entry.
                let tiling_type_int = dictionary
                    .get("TilingType")
                    .ok_or(PatternError::MissingRequiredEntry("TilingType"))?
                    .as_number::<i32>()
                    .map_err(|e| PatternError::NumericConversionError {
                        entry_description: "TilingType",
                        source: e,
                    })?;
                let tiling_type = TilingType::from_i32(tiling_type_int).ok_or_else(|| {
                    PatternError::InvalidValue {
                        key: "TilingType",
                        value: tiling_type_int.to_string(),
                    }
                })?;

                // Read the `/BBox` entry.
                let bbox = BBox::from_dictionary(dictionary, objects)?
                    .ok_or(PatternError::MissingRequiredEntry("BBox"))?;

                // Read the `/XStep` entry.
                let x_step = dictionary
                    .get("XStep")
                    .ok_or(PatternError::MissingRequiredEntry("XStep"))?
                    .as_number::<f32>()
                    .map_err(|e| PatternError::NumericConversionError {
                        entry_description: "XStep",
                        source: e,
                    })?;

                // Read the `/YStep` entry.
                let y_step = dictionary
                    .get("YStep")
                    .ok_or(PatternError::MissingRequiredEntry("YStep"))?
                    .as_number::<f32>()
                    .map_err(|e| PatternError::NumericConversionError {
                        entry_description: "YStep",
                        source: e,
                    })?;

                // Read the `/Resources` entry. Needed by the pattern's content stream.
                let resources = Resources::from_dictionary(dictionary, objects)
                    .map_err(|err| PatternError::ResourcesParse { err: Box::new(err) })?
                    .ok_or(PatternError::MissingRequiredEntry("Resources"))?;

                Ok(Pattern::Tiling {
                    paint_type,
                    tiling_type,
                    bbox,
                    x_step,
                    y_step,
                    matrix,
                    resources,
                })
            }
            Some(PatternType::Shading) => {
                // Read the shading object that defines the gradient fill.
                let shading = dictionary
                    .get_dictionary("Shading")
                    .ok_or(PatternError::MissingRequiredEntry("Shading"))
                    .and_then(|shading_dict| {
                        Shading::from_dictionary(shading_dict, objects).map_err(Into::into)
                    })?;

                // Read an external graphics state dictionary to apply when painting the pattern.
                let ext_g_state = if let Some(ext) = dictionary.get_dictionary("ExtGState") {
                    Some(ExternalGraphicsState::from_dictionary(ext, objects)?)
                } else {
                    None
                };

                Ok(Pattern::Shading {
                    shading,
                    matrix,
                    ext_g_state,
                })
            }
            _ => Err(PatternError::InvalidPatternType(pattern_type)),
        }
    }
}
