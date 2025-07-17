use pdf_object::{
    ObjectVariant, dictionary::Dictionary, object_collection::ObjectCollection,
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
    #[error("Invalid /PatternType value")]
    InvalidPatternType,
    #[error("Invalid value for key: {0}")]
    InvalidValue(String),
    #[error("Invalid type for entry '{entry_name}': expected {expected_type}, found {found_type}")]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str,
    },
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
}

/// PatternType as defined in PDF 1.7 Table 87
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternType {
    /// Tiling pattern (PatternType = 1)
    Tiling = 1,
    /// Shading pattern (PatternType = 2)
    Shading = 2,
}

impl PatternType {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(PatternType::Tiling),
            2 => Some(PatternType::Shading),
            _ => None,
        }
    }
}

/// PaintType for tiling patterns (PDF 1.7 Table 88)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintType {
    /// Colored tiling pattern (PaintType = 1)
    Colored = 1,
    /// Uncolored tiling pattern (PaintType = 2)
    Uncolored = 2,
}

impl PaintType {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(PaintType::Colored),
            2 => Some(PaintType::Uncolored),
            _ => None,
        }
    }
}

/// TilingType for tiling patterns (PDF 1.7 Table 88)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingType {
    /// Constant spacing (TilingType = 1)
    ConstantSpacing = 1,
    /// No distortion (TilingType = 2)
    NoDistortion = 2,
    /// Constant spacing and faster tiling (TilingType = 3)
    ConstantSpacingFast = 3,
}

impl TilingType {
    pub fn from_i32(val: i32) -> Option<Self> {
        match val {
            1 => Some(TilingType::ConstantSpacing),
            2 => Some(TilingType::NoDistortion),
            3 => Some(TilingType::ConstantSpacingFast),
            _ => None,
        }
    }
}

pub enum Pattern {
    Tiling {
        paint_type: Option<i32>,
        tiling_type: Option<i32>,
        bbox: Option<BBox>,
        x_step: Option<f32>,
        y_step: Option<f32>,
        matrix: Option<Matrix>,
        resources: Option<Resources>,
    },
    Shading {
        shading: Option<Shading>,
        matrix: Option<Matrix>,
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
            .ok_or(PatternError::MissingPatternType)?;
        match pattern_type.as_ref() {
            ObjectVariant::Integer(1) => {
                // Tiling pattern
                let paint_type = dictionary
                    .get("PaintType")
                    .and_then(|v| v.as_number::<i32>().ok());
                let tiling_type = dictionary
                    .get("TilingType")
                    .and_then(|v| v.as_number::<i32>().ok());
                let bbox = BBox::from_dictionary(dictionary, objects)?;
                let x_step = dictionary
                    .get("XStep")
                    .and_then(|v| v.as_number::<f32>().ok());
                let y_step = dictionary
                    .get("YStep")
                    .and_then(|v| v.as_number::<f32>().ok());
                let matrix = Matrix::from_dictionary(dictionary, objects)?;
                let resources = Resources::from_dictionary(dictionary, objects)
                    .map_err(|err| PatternError::ResourcesParse { err: Box::new(err) })?;

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
            ObjectVariant::Integer(2) => {
                // Shading pattern
                let shading = if let Some(ext) = dictionary.get_dictionary("Shading") {
                    Some(Shading::from_dictionary(ext, objects)?)
                } else {
                    None
                };

                let matrix = Matrix::from_dictionary(dictionary, objects)?;

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
            _ => Err(PatternError::InvalidPatternType),
        }
    }
}
