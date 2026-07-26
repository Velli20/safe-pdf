//! Public PDF shading model types.

use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{color::Color, point::Point, rect::Rect};
use pdf_object::{object_resolver::ObjectResolver, object_variant::ObjectVariant};

use crate::{color_stops::ColorStops, error::PdfShadingError};

/// The `/ShadingType` discriminator used by PDF shading dictionaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ShadingType {
    /// Type 1 function-based shading.
    FunctionBased = 1,
    /// Type 2 axial shading.
    Axial = 2,
    /// Type 3 radial shading.
    Radial = 3,
    /// Type 4 free-form Gouraud-shaded triangle mesh.
    FreeFormTriangleMesh = 4,
    /// Type 5 lattice-form Gouraud-shaded triangle mesh.
    LatticeFormTriangleMesh = 5,
    /// Type 6 Coons patch mesh shading.
    CoonsPatchMesh = 6,
    /// Type 7 tensor-product patch mesh shading.
    TensorProductPatchMesh = 7,
}

impl std::fmt::Display for ShadingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::FunctionBased => "FunctionBased",
            Self::Axial => "Axial",
            Self::Radial => "Radial",
            Self::FreeFormTriangleMesh => "FreeFormTriangleMesh",
            Self::LatticeFormTriangleMesh => "LatticeFormTriangleMesh",
            Self::CoonsPatchMesh => "CoonsPatchMesh",
            Self::TensorProductPatchMesh => "TensorProductPatchMesh",
        };
        f.write_str(name)
    }
}

impl TryFrom<i32> for ShadingType {
    type Error = PdfShadingError;

    /// Converts a numeric `/ShadingType` value into the typed enum.
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::FunctionBased),
            2 => Ok(Self::Axial),
            3 => Ok(Self::Radial),
            4 => Ok(Self::FreeFormTriangleMesh),
            5 => Ok(Self::LatticeFormTriangleMesh),
            6 => Ok(Self::CoonsPatchMesh),
            7 => Ok(Self::TensorProductPatchMesh),
            _ => Err(PdfShadingError::InvalidShadingType { value }),
        }
    }
}

/// A parsed PDF shading object.
#[derive(Debug, Clone)]
pub enum Shading {
    /// Type 1 function-based shading.
    FunctionBased {
        /// The output color space, if explicitly defined.
        color_space: Option<ColorSpace>,
        /// Optional background color component values.
        background: Option<Vec<f32>>,
        /// Optional bounding box in shading space.
        bbox: Option<Rect>,
        /// Optional anti-aliasing preference.
        anti_alias: Option<bool>,
        /// Optional function domain values.
        domain: Option<Vec<f32>>,
        /// The shading functions.
        functions: Vec<pdf_function::function::Function>,
    },
    /// Type 2 axial shading.
    Axial {
        /// The shading color space.
        color_space: ColorSpace,
        /// Gradient axis coordinates `[x0, y0, x1, y1]`.
        coords: [f32; 4],
        /// Sampled color stops used by render backends.
        color_stops: ColorStops,
    },
    /// Type 3 radial shading.
    Radial {
        /// The shading color space.
        color_space: ColorSpace,
        /// Gradient circle coordinates `[x0, y0, r0, x1, y1, r1]`.
        coords: [f32; 6],
        /// Sampled color stops used by render backends.
        color_stops: ColorStops,
        /// Optional bounding box in shading space.
        bbox: Option<Rect>,
    },
    /// Type 4 free-form Gouraud-shaded triangle mesh.
    FreeFormTriangleMesh {
        /// The shading color space.
        color_space: ColorSpace,
        /// Optional mesh bounding box in shading space.
        bbox: Option<Rect>,
        /// Optional anti-aliasing preference.
        anti_alias: Option<bool>,
        /// Parsed vertex-colored triangles.
        triangles: Vec<MeshTriangle>,
    },
    /// Type 6 or 7 patch mesh shading.
    PatchMesh {
        /// The original mesh shading subtype.
        shading_type: ShadingType,
        /// The shading color space.
        color_space: ColorSpace,
        /// Optional mesh bounding box in shading space.
        bbox: Option<Rect>,
        /// Optional anti-aliasing preference.
        anti_alias: Option<bool>,
        /// Parsed mesh patches.
        patches: Vec<MeshPatch>,
    },
    /// Placeholder for unsupported shading types.
    Unsupported {
        /// The unsupported shading type name.
        name: String,
    },
}

/// A vertex in a PDF triangle mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    /// The vertex position in shading space.
    pub point: Point,
    /// The decoded vertex color.
    pub color: Color,
}

/// A Gouraud-shaded triangle from a PDF triangle mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshTriangle {
    /// The triangle vertices in stream order.
    pub vertices: [MeshVertex; 3],
}

/// A parsed patch from a PDF mesh shading.
#[derive(Debug, Clone)]
pub enum MeshPatch {
    /// A Coons patch with 12 control points and 4 corner colors.
    Coons {
        /// Boundary control points for the patch.
        control_points: [Point; 12],
        /// Corner colors ordered clockwise from the top-left corner.
        corner_colors: [Color; 4],
    },
    /// A tensor-product patch with a 4x4 control net and 4 corner colors.
    Tensor {
        /// Control points for the 4x4 patch net.
        control_points: [Point; 16],
        /// Corner colors ordered clockwise from the top-left corner.
        corner_colors: [Color; 4],
    },
}

impl Shading {
    /// Returns the shading color space when the shading type defines one.
    pub fn color_space(&self) -> Option<&ColorSpace> {
        match self {
            Self::FunctionBased { color_space, .. } => color_space.as_ref(),
            Self::Axial { color_space, .. }
            | Self::Radial { color_space, .. }
            | Self::FreeFormTriangleMesh { color_space, .. }
            | Self::PatchMesh { color_space, .. } => Some(color_space),
            Self::Unsupported { .. } => None,
        }
    }

    /// Returns the optional bounding box associated with this shading.
    pub fn bbox(&self) -> Option<&Rect> {
        match self {
            Self::FunctionBased { bbox, .. }
            | Self::Radial { bbox, .. }
            | Self::FreeFormTriangleMesh { bbox, .. }
            | Self::PatchMesh { bbox, .. } => bbox.as_ref(),
            Self::Axial { .. } | Self::Unsupported { .. } => None,
        }
    }

    /// Parses a shading object from a PDF shading dictionary or stream.
    pub fn from_dictionary(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfShadingError> {
        crate::parse::shading_from_dictionary(object, objects)
    }
}
