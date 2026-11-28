//! Parsing for Type 6 Coons and Type 7 tensor-product patch meshes.
//!
//! This module is the entry point for patch-mesh parsing. Dictionary parsing,
//! packed-stream decoding, and patch reconstruction live in dedicated sibling
//! modules so each stage can be understood independently.

use thiserror::Error;

use crate::{
    error::PdfShadingError, mesh_config::MeshConfig, model::Shading, patch_mesh_kind::PatchKind,
    patch_mesh_parser::PatchMeshParser,
};

/// Errors produced while reconstructing Type 6 and Type 7 patch meshes.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PatchMeshError {
    /// A continuation record appeared without a compatible preceding patch.
    #[error("{kind} continuation flag {flag} used without a previous patch")]
    ContinuationWithoutPreviousPatch { kind: &'static str, flag: u8 },
    /// Patch reconstruction did not produce the required fixed-size data.
    #[error("patch did not contain {expected}")]
    IncompletePatch { expected: &'static str },
    /// The stream did not contain any complete patches.
    #[error("stream did not contain any patches")]
    EmptyMesh,
}

impl MeshConfig {
    /// Decodes this Type 6 or Type 7 stream into a patch-mesh shading.
    pub(crate) fn patch_mesh(self, kind: PatchKind) -> Result<Shading, PdfShadingError> {
        let patches = match self.decoder("Patch mesh")? {
            Some(decoder) => PatchMeshParser::new(decoder, kind).parse()?,
            None => Vec::new(),
        };

        Ok(Shading::PatchMesh {
            shading_type: kind.shading_type(),
            color_space: self.color_space,
            bbox: self.bbox,
            anti_alias: self.anti_alias,
            patches,
        })
    }
}
