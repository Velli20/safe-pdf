mod eval;
mod raster;

pub use eval::{
    MeshPatchRef, MeshVertex, evaluate_coons_patch_vertex, evaluate_tensor_patch_vertex,
    patch_mesh_bounds, patch_subdivision, tessellate_patch,
};
pub use raster::{RasterizedPatchMesh, rasterize_patch_mesh, rasterize_triangle};

#[cfg(test)]
mod tests;
