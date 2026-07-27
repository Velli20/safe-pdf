use pdf_color_space::error::ColorSpaceError;
use pdf_decode::DecodeError;
use pdf_function::{
    error::FunctionReadError, function_interpolation_error::FunctionInterpolationError,
};
use pdf_object::error::ObjectError;
use thiserror::Error;

pub use crate::{
    free_form_mesh::FreeFormMeshError, mesh_decoder::MeshDecoderError, patch_mesh::PatchMeshError,
};

/// Errors that can occur while parsing or preparing PDF shadings.
#[derive(Debug, Error)]
pub enum PdfShadingError {
    #[error("{0}")]
    Object(#[from] ObjectError),
    #[error("{0}")]
    ColorSpace(#[from] ColorSpaceError),
    #[error("{0}")]
    FunctionInterpolation(#[from] FunctionInterpolationError),
    #[error("{0}")]
    FunctionRead(#[from] FunctionReadError),
    #[error("{0}")]
    Decode(#[from] DecodeError),
    #[error("missing required dictionary entry '/{entry}'")]
    MissingRequiredEntry { entry: &'static str },
    #[error("invalid /ShadingType value: {value}")]
    InvalidShadingType { value: i32 },
    #[error("invalid shading mesh data: {0}")]
    MeshDecoder(#[from] MeshDecoderError),
    #[error("invalid shading mesh data: {0}")]
    FreeFormMesh(#[from] FreeFormMeshError),
    #[error("invalid shading mesh data: {0}")]
    PatchMesh(#[from] PatchMeshError),
    #[error("unsupported shading feature: {0}")]
    UnsupportedFeature(String),
}
