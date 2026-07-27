//! Stateful decoding of packed patch-mesh streams.

use pdf_graphics::{color::Color, point::Point};
use pdf_utils::BitReader;

use crate::{
    error::PdfShadingError,
    mesh_decoder::{MeshDecoder, read_mesh_bits},
    model::MeshPatch,
    patch_mesh::PatchMeshError,
    patch_mesh_config::PatchMeshConfig,
    patch_mesh_kind::PatchKind,
};

/// Reconstructs patches and their implicit shared edges from a packed stream.
pub(crate) struct PatchMeshParser<'a> {
    reader: BitReader<'a>,
    decoder: MeshDecoder<'a>,
    flag_width: usize,
    kind: PatchKind,
}

impl<'a> PatchMeshParser<'a> {
    /// Creates a parser borrowing the stream and its validated configuration.
    pub(crate) fn new(
        data: &'a [u8],
        config: &'a PatchMeshConfig,
        kind: PatchKind,
    ) -> Result<Self, PdfShadingError> {
        Ok(Self {
            reader: BitReader::new(data),
            decoder: config.decoder()?,
            flag_width: config.flag_width(),
            kind,
        })
    }

    /// Consumes the packed stream and reconstructs all complete patches.
    pub(crate) fn parse(mut self) -> Result<Vec<MeshPatch>, PdfShadingError> {
        let mut patches = Vec::new();

        while let Some(flag) = self.read_flag()? {
            let patch = self.read_patch(flag, patches.last())?;
            patches.push(patch);
        }

        if patches.is_empty() {
            return Err(PatchMeshError::EmptyMesh.into());
        }

        Ok(patches)
    }

    /// Reads a flag and keeps only the two low bits defined by the PDF format.
    fn read_flag(&mut self) -> Result<Option<u8>, PdfShadingError> {
        read_mesh_bits(&mut self.reader, self.flag_width)?
            .map(|flag| {
                let value = flag & 0b11;
                u8::try_from(value)
                    .map_err(|_| PdfShadingError::from(PatchMeshError::FlagOutOfRange { value }))
            })
            .transpose()
    }

    /// Reads the explicit values and prepends values shared with the prior patch.
    fn read_patch(
        &mut self,
        flag: u8,
        previous: Option<&MeshPatch>,
    ) -> Result<MeshPatch, PdfShadingError> {
        let mut control_points = self.kind.initial_control_points(flag, previous)?;
        self.read_points(&mut control_points, self.kind.control_point_count())?;

        let mut corner_colors = self.kind.initial_corner_colors(flag, previous)?;
        self.read_colors(&mut corner_colors)?;

        self.kind.build_patch(control_points, corner_colors)
    }

    /// Reads points until the patch has the required number of control points.
    fn read_points(
        &mut self,
        points: &mut Vec<Point>,
        required: usize,
    ) -> Result<(), PdfShadingError> {
        while points.len() < required {
            points.push(self.decoder.read_point(&mut self.reader)?);
        }
        Ok(())
    }

    /// Reads colors until all four patch corners have a color.
    fn read_colors(&mut self, colors: &mut Vec<Color>) -> Result<(), PdfShadingError> {
        while colors.len() < PatchKind::CORNER_COLOR_COUNT {
            colors.push(self.decoder.read_color(&mut self.reader)?);
        }
        Ok(())
    }
}
