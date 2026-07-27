//! Shared PDF shading parsing, modeling, and rasterization utilities.

pub mod color_stops;
pub mod error;
mod free_form_mesh;
pub mod mesh;
mod mesh_decoder;
pub mod model;
pub mod paint;
mod parse;
mod patch_mesh;
