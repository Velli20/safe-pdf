//! PDF Shading object parsing and representation.
//!
//! This module provides types and parsing logic for PDF shading objects,
//! which define smooth color transitions (gradients) across areas.
//!
//! # Supported Shading Types
//!
//! - **Type 1 (FunctionBased)**: Color at every point defined by a mathematical function.
//! - **Type 2 (Axial)**: Linear gradient between two points.
//! - **Type 3 (Radial)**: Circular gradient between two circles.
//!
//! Types 4-7 (mesh-based shadings) are recognized but not yet fully supported.

use pdf_graphics::rect::Rect;
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

use crate::{
    color_space::{ColorSpace, ColorSpaceError},
    color_stops::ColorStops,
    functions::{Function, FunctionImpl, FunctionInterpolationError, FunctionReadError},
};

/// Errors that can occur while parsing or processing a Shading object.
#[derive(Debug, Error)]
pub enum ShadingError {
    #[error("Missing required entry '{entry_name}'")]
    MissingRequiredEntry { entry_name: &'static str },
    #[error("Unsupported /ShadingType value: {0}")]
    UnsupportedShadingType(ShadingType),
    #[error("Unknown /ShadingType value: {0}")]
    InvalidShadingType(i32),
    #[error("Error parsing Function: {0}")]
    FunctionReadError(#[from] FunctionReadError),
    #[error("Error interpolating Function: {0}")]
    FunctionInterpolationError(#[from] FunctionInterpolationError),
    #[error("Error parsing Dictionary: {0}")]
    ObjectError(#[from] ObjectError),
    #[error("ColorSpace error: {0}")]
    ColorSpaceError(#[from] ColorSpaceError),
}

/// Represents the PDF `/ShadingType` entry value.
///
/// PDF supports seven shading types, each defining a different method
/// for computing colors across an area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ShadingType {
    /// Type 1: Function-based shading. Color at each point is computed
    /// by evaluating a function with the point's coordinates as input.
    FunctionBased = 1,
    /// Type 2: Axial shading (linear gradient). Colors blend along a
    /// line between two points.
    Axial = 2,
    /// Type 3: Radial shading (circular gradient). Colors blend between
    /// two circles, creating cone or cylinder effects.
    Radial = 3,
    /// Type 4: Free-form Gouraud-shaded triangle mesh.
    FreeFormTriangleMesh = 4,
    /// Type 5: Lattice-form Gouraud-shaded triangle mesh.
    LatticeFormTriangleMesh = 5,
    /// Type 6: Coons patch mesh.
    CoonsPatchMesh = 6,
    /// Type 7: Tensor-product patch mesh.
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
    type Error = ShadingError;

    /// Attempts to convert an integer to a `ShadingType`.
    ///
    /// # Errors
    ///
    /// Returns [`ShadingError::InvalidShadingType`] if the value is not in the range 1-7.
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::FunctionBased),
            2 => Ok(Self::Axial),
            3 => Ok(Self::Radial),
            4 => Ok(Self::FreeFormTriangleMesh),
            5 => Ok(Self::LatticeFormTriangleMesh),
            6 => Ok(Self::CoonsPatchMesh),
            7 => Ok(Self::TensorProductPatchMesh),
            _ => Err(ShadingError::InvalidShadingType(value)),
        }
    }
}

/// Represents a PDF Shading object, which defines a smooth transition between colors
/// across an area, used for creating gradient fills.
///
/// Shadings are used in PDF to create smooth color transitions (gradients) and are
/// commonly used with patterns or the `sh` operator for filling areas.
#[derive(Debug, Clone)]
pub enum Shading {
    /// Type 1: Function-based shading.
    ///
    /// The color at every point in the shading domain is defined by evaluating
    /// one or more mathematical functions with the point's coordinates as input.
    /// This allows for arbitrary color distributions defined mathematically.
    FunctionBased {
        /// The color space in which the function's output values are interpreted.
        /// If `None`, the color space may be inherited from context.
        color_space: Option<ColorSpace>,

        /// Optional background color as an array of color components.
        /// Used to fill areas outside the shading's bounding box.
        background: Option<Vec<f32>>,

        /// Optional bounding box `[x_min, y_min, x_max, y_max]` in the shading's
        /// target coordinate space. Clips the shading to this rectangle.
        bbox: Option<Rect>,

        /// Whether to apply anti-aliasing to reduce visual artifacts.
        anti_alias: Option<bool>,

        /// The domain rectangle `[x_min, x_max, y_min, y_max]` specifying the
        /// valid input range for the function(s). Defaults to `[0, 1, 0, 1]`.
        domain: Option<Vec<f32>>,

        /// One or more functions that define the color at each point.
        /// - Single 2-in, n-out function: takes (x, y), returns n color components.
        /// - Array of n 2-in, 1-out functions: each returns one color component.
        functions: Vec<Function>,
    },

    /// Type 2: Axial shading (linear gradient).
    ///
    /// Colors blend smoothly along a line between two points. The gradient
    /// extends infinitely perpendicular to the axis line, with constant color
    /// at any given distance along the axis.
    Axial {
        /// The color space in which color values are expressed.
        color_space: ColorSpace,

        /// Axis coordinates `[x0, y0, x1, y1]` defining the gradient line.
        /// - `(x0, y0)`: Starting point (parameter t=0).
        /// - `(x1, y1)`: Ending point (parameter t=1).
        coords: [f32; 4],

        /// Pre-computed color stops.
        /// Sampled from the function at regular intervals.
        color_stops: ColorStops,
    },

    /// Type 3: Radial shading (circular gradient).
    ///
    /// Colors blend between two circles, creating effects like cones, cylinders,
    /// or spherical highlights. The circles need not be concentric.
    Radial {
        /// The color space in which color values are expressed.
        color_space: ColorSpace,

        /// Circle coordinates `[x0, y0, r0, x1, y1, r1]` defining the gradient.
        /// - `(x0, y0, r0)`: Center and radius of the starting circle (t=0).
        /// - `(x1, y1, r1)`: Center and radius of the ending circle (t=1).
        coords: [f32; 6],

        /// Pre-computed color stops.
        /// Sampled from the function at regular intervals.
        color_stops: ColorStops,

        /// Optional bounding box `[x_min, y_min, x_max, y_max]` in the shading's
        /// target coordinate space. Clips the shading to this rectangle.
        bbox: Option<Rect>,
    },
}

impl Shading {
    /// Returns the color space used by this shading, if available.
    pub fn color_space(&self) -> Option<&ColorSpace> {
        match self {
            Self::FunctionBased { color_space, .. } => color_space.as_ref(),
            Self::Axial { color_space, .. } | Self::Radial { color_space, .. } => Some(color_space),
        }
    }
}

impl Shading {
    pub fn from_dictionary(
        object: &ObjectVariant,
        objects: &ObjectCollection,
    ) -> Result<Self, ShadingError> {
        // Extract and validate the required `/ShadingType` entry.
        let dictionary = object.try_dictionary(objects)?;
        let shading_type_value = dictionary.get_or_err("ShadingType")?.as_number::<i32>()?;
        let shading_type = ShadingType::try_from(shading_type_value)?;

        match shading_type {
            ShadingType::FunctionBased => Self::parse_function_based(dictionary, objects),
            ShadingType::Axial => Self::parse_axial(object, objects),
            ShadingType::Radial => Self::parse_radial(object, objects),
            // Mesh-based shadings are recognized but not yet implemented.
            unsupported => Err(ShadingError::UnsupportedShadingType(unsupported)),
        }
    }
}

impl Shading {
    /// Parses a Type 1 (function-based) shading from a dictionary.
    fn parse_function_based(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self, ShadingError> {
        // Read optional `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?;

        // Read optional `/Background` entry (array of color components).
        let background = dictionary
            .get("Background")
            .map(ObjectVariant::as_vec_of::<f32>)
            .transpose()?;

        // Read optional `/BBox` entry (clipping rectangle).
        let bbox = dictionary
            .get("BBox")
            .map(ObjectVariant::as_array_of::<f32, 4>)
            .transpose()?
            .map(Rect::from);

        // Read optional `/Domain` entry (function input range).
        let domain = dictionary
            .get("Domain")
            .map(ObjectVariant::as_vec_of::<f32>)
            .transpose()?;

        // Read required `/Function` entry.
        let functions = Self::parse_functions(dictionary, objects)?;

        Ok(Self::FunctionBased {
            color_space,
            background,
            bbox,
            anti_alias: None,
            domain,
            functions,
        })
    }

    /// Parses a Type 2 (axial) shading from a dictionary.
    fn parse_axial(
        object: &ObjectVariant,
        objects: &ObjectCollection,
    ) -> Result<Self, ShadingError> {
        // Read required `/Coords` entry defining the gradient axis.
        let dictionary = object.try_dictionary(objects)?;
        let coords = dictionary.get_or_err("Coords")?.as_array_of::<f32, 4>()?;

        // Read required `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
            ShadingError::MissingRequiredEntry {
                entry_name: "ColorSpace",
            },
        )?;

        // Read required `/Function` entry.
        let object = dictionary.get_or_err("Function")?;
        let function = Function::parse(object, objects)?;

        // Pre-compute color stops.
        let color_stops = ColorStops::from_function(&function, &color_space)?;

        Ok(Self::Axial {
            color_space,
            coords,
            color_stops,
        })
    }

    /// Parses a Type 3 (radial) shading from a dictionary.
    fn parse_radial(
        object: &ObjectVariant,
        objects: &ObjectCollection,
    ) -> Result<Self, ShadingError> {
        // Read required `/Coords` entry defining the two circles.
        let dictionary = object.try_dictionary(objects)?;
        let coords = dictionary.get_or_err("Coords")?.as_array_of::<f32, 6>()?;

        // Read required `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
            ShadingError::MissingRequiredEntry {
                entry_name: "ColorSpace",
            },
        )?;

        // Read optional `/BBox` entry.
        let bbox = dictionary
            .get("BBox")
            .map(ObjectVariant::as_array_of::<f32, 4>)
            .transpose()?
            .map(Rect::from);

        // Read required `/Function` entry.
        let object = dictionary.get_or_err("Function")?;
        let function = Function::parse(object, objects)?;

        // Pre-compute color stops.
        let color_stops = ColorStops::from_function(&function, &color_space)?;

        Ok(Self::Radial {
            color_space,
            coords,
            color_stops,
            bbox,
        })
    }

    /// Parses one or more functions from the `/Function` entry.
    ///
    /// The `/Function` entry may be either:
    /// - A single function (dictionary or stream)
    /// - An array of functions
    fn parse_functions(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Vec<Function>, ShadingError> {
        let function_obj = objects.resolve_object(dictionary.get_or_err("Function")?)?;

        if let ObjectVariant::Array(array) = function_obj {
            // Parse array of functions.
            array
                .iter()
                .map(|value| Function::parse(value, objects).map_err(ShadingError::from))
                .collect()
        } else {
            // Parse single function.
            let function = Function::parse(function_obj, objects)?;
            Ok(vec![function])
        }
    }
}

impl Shading {
    pub fn bbox(&self) -> Option<&Rect> {
        match self {
            Shading::FunctionBased { bbox, .. } => bbox.as_ref(),
            Shading::Radial { bbox, .. } => bbox.as_ref(),
            _ => None,
        }
    }
}
