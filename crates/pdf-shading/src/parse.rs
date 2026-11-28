//! Top-level parsing for PDF shading dictionaries and streams.
//!
//! Each shading type has its own decoder: function-based shadings in
//! `function_shading`, axial and radial ones in `gradient_shading`, and mesh streams
//! in `mesh_config` with the free-form and patch-mesh modules. This module only
//! dispatches by shading type.

use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{
    mesh_config::MeshConfig,
    model::{Shading, ShadingType},
    patch_mesh_kind::PatchKind,
};

impl FromPdfObject for Shading {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let shading = match context.entries()?.required::<ShadingType>(b"ShadingType")? {
            ShadingType::FunctionBased => Shading::FunctionBased(context.decode()?),
            ShadingType::Axial => {
                let coords = context.entries()?.required(b"Coords")?;
                Shading::Axial {
                    coords,
                    gradient: context.decode()?,
                }
            }
            ShadingType::Radial => {
                let coords = context.entries()?.required(b"Coords")?;
                Shading::Radial {
                    coords,
                    gradient: context.decode()?,
                }
            }
            ShadingType::FreeFormTriangleMesh => {
                context.decode::<MeshConfig>()?.free_form_triangle_mesh()?
            }
            ShadingType::CoonsPatchMesh => context
                .decode::<MeshConfig>()?
                .patch_mesh(PatchKind::Coons)?,
            ShadingType::TensorProductPatchMesh => context
                .decode::<MeshConfig>()?
                .patch_mesh(PatchKind::Tensor)?,
            unsupported @ ShadingType::LatticeFormTriangleMesh => Shading::Unsupported(unsupported),
        };
        Ok(shading)
    }
}
