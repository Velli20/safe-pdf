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
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut context = context.dictionary()?;
        let shading = match context.required::<ShadingType>(b"ShadingType")? {
            ShadingType::FunctionBased => Shading::FunctionBased(context.read(object)?),
            ShadingType::Axial => Shading::Axial {
                coords: context.required(b"Coords")?,
                gradient: context.read(object)?,
            },
            ShadingType::Radial => Shading::Radial {
                coords: context.required(b"Coords")?,
                gradient: context.read(object)?,
            },
            ShadingType::FreeFormTriangleMesh => context
                .read::<MeshConfig>(object)?
                .free_form_triangle_mesh()?,
            ShadingType::CoonsPatchMesh => context
                .read::<MeshConfig>(object)?
                .patch_mesh(PatchKind::Coons)?,
            ShadingType::TensorProductPatchMesh => context
                .read::<MeshConfig>(object)?
                .patch_mesh(PatchKind::Tensor)?,
            unsupported @ ShadingType::LatticeFormTriangleMesh => Shading::Unsupported {
                name: unsupported.to_string(),
            },
        };
        Ok(shading)
    }
}
