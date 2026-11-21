//! Stateful decoding of packed patch-mesh streams.

use crate::{
    error::PdfShadingError,
    mesh_config::MeshConfig,
    mesh_decoder::MeshDecoder,
    model::MeshPatch,
    patch_mesh::PatchMeshError,
    patch_mesh_kind::{PatchKind, SharedEdge},
};

/// Reconstructs patches and their implicit shared edges from a packed stream.
pub(crate) struct PatchMeshParser<'a> {
    decoder: MeshDecoder<'a>,
    kind: PatchKind,
}

impl<'a> PatchMeshParser<'a> {
    /// Creates a parser over the stream of `config`.
    pub(crate) fn new(config: &'a MeshConfig, kind: PatchKind) -> Result<Self, PdfShadingError> {
        Ok(Self {
            decoder: config.decoder("Patch mesh")?,
            kind,
        })
    }

    /// Consumes the packed stream and reconstructs all complete patches.
    pub(crate) fn parse(mut self) -> Result<Vec<MeshPatch>, PdfShadingError> {
        let mut patches = Vec::new();

        while let Some(flag) = self.decoder.read_flag()? {
            let patch = self.read_patch(SharedEdge::from_flag(flag), patches.last())?;
            patches.push(patch);
        }

        if patches.is_empty() {
            return Err(PatchMeshError::EmptyMesh.into());
        }
        Ok(patches)
    }

    /// Reads the explicit values and prepends values shared with the prior patch.
    fn read_patch(
        &mut self,
        edge: Option<SharedEdge>,
        previous: Option<&MeshPatch>,
    ) -> Result<MeshPatch, PdfShadingError> {
        let mut control_points = self.kind.initial_control_points(edge, previous)?;
        while control_points.len() < self.kind.control_point_count() {
            control_points.push(self.decoder.read_point()?);
        }

        let mut corner_colors = self.kind.initial_corner_colors(edge, previous)?;
        while corner_colors.len() < PatchKind::CORNER_COLOR_COUNT {
            corner_colors.push(self.decoder.read_color()?);
        }

        self.kind.build_patch(control_points, corner_colors)
    }
}
