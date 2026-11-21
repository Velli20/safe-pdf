//! Parsing for Type 4 free-form Gouraud triangle meshes.

use thiserror::Error;

use crate::{
    error::PdfShadingError,
    mesh_config::MeshConfig,
    mesh_decoder::MeshDecoder,
    model::{MeshTriangle, MeshVertex, Shading},
};

/// Errors produced while reconstructing a Type 4 free-form triangle mesh.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FreeFormMeshError {
    /// A continuation record appeared before any complete triangle.
    #[error("continuation flag {flag} used without a previous triangle")]
    ContinuationWithoutPreviousTriangle { flag: u8 },
    /// A vertex record used an edge flag that is not defined for a Type 4 mesh.
    #[error("invalid edge flag {flag}")]
    InvalidEdgeFlag { flag: u8 },
    /// The stream ended before all three vertices of a triangle were decoded.
    #[error("stream ended before completing a triangle")]
    IncompleteTriangle,
    /// The stream did not contain any complete triangles.
    #[error("stream did not contain any triangles")]
    EmptyMesh,
}

impl MeshConfig {
    /// Decodes this Type 4 stream into independent mesh triangles.
    pub(crate) fn free_form_triangle_mesh(self) -> Result<Shading, PdfShadingError> {
        let triangles = self.decoder("Free-form triangle mesh")?.triangles()?;

        Ok(Shading::FreeFormTriangleMesh {
            color_space: self.color_space,
            bbox: self.bbox,
            anti_alias: self.anti_alias,
            triangles,
        })
    }
}

/// Reconstruction of triangles from Type 4 vertex records.
impl MeshDecoder<'_> {
    /// Consumes all complete vertex records and reconstructs their triangles.
    ///
    /// Flag 0 starts a new triangle from the next three vertices; flags 1 and 2
    /// form a triangle from one vertex and an edge of the preceding triangle.
    fn triangles(mut self) -> Result<Vec<MeshTriangle>, PdfShadingError> {
        let mut triangles = Vec::<MeshTriangle>::new();

        while let Some((flag, vertex)) = self.read_vertex()? {
            let vertices = match (flag, triangles.last()) {
                (0, _) => [
                    vertex,
                    self.read_required_vertex()?,
                    self.read_required_vertex()?,
                ],
                (1, Some(previous)) => {
                    let [_, second, third] = previous.vertices;
                    [second, third, vertex]
                }
                (2, Some(previous)) => {
                    let [first, _, third] = previous.vertices;
                    [first, third, vertex]
                }
                (1 | 2, None) => {
                    return Err(
                        FreeFormMeshError::ContinuationWithoutPreviousTriangle { flag }.into(),
                    );
                }
                _ => return Err(FreeFormMeshError::InvalidEdgeFlag { flag }.into()),
            };
            triangles.push(MeshTriangle { vertices });
        }

        if triangles.is_empty() {
            return Err(FreeFormMeshError::EmptyMesh.into());
        }
        Ok(triangles)
    }

    /// Reads a vertex that must exist to complete the current triangle; its flag is ignored.
    fn read_required_vertex(&mut self) -> Result<MeshVertex, PdfShadingError> {
        let (_, vertex) = self
            .read_vertex()?
            .ok_or(FreeFormMeshError::IncompleteTriangle)?;
        Ok(vertex)
    }

    /// Reads one byte-aligned Type 4 vertex record.
    fn read_vertex(&mut self) -> Result<Option<(u8, MeshVertex)>, PdfShadingError> {
        let Some(flag) = self.read_flag()? else {
            return Ok(None);
        };
        let point = self.read_point()?;
        let color = self.read_color()?;
        self.align_to_byte();
        Ok(Some((flag, MeshVertex { point, color })))
    }
}
