//! PDF Shading object parsing and representation.

use pdf_graphics::{color::Color, point::Point, rect::Rect};
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::color_stops::ColorStops;
use crate::error::PdfPagesError;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};

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
    type Error = PdfPagesError;

    /// Attempts to convert an integer to a `ShadingType`.
    ///
    /// # Errors
    ///
    /// Returns [`PdfPagesError::InvalidShadingType`] if the value is not in the range 1-7.
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::FunctionBased),
            2 => Ok(Self::Axial),
            3 => Ok(Self::Radial),
            4 => Ok(Self::FreeFormTriangleMesh),
            5 => Ok(Self::LatticeFormTriangleMesh),
            6 => Ok(Self::CoonsPatchMesh),
            7 => Ok(Self::TensorProductPatchMesh),
            _ => Err(PdfPagesError::InvalidShadingType { value }),
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

    /// Type 6 / 7 patch mesh shading.
    PatchMesh {
        /// The original mesh shading subtype.
        shading_type: ShadingType,
        /// The source color space for the shading.
        color_space: ColorSpace,
        /// Optional mesh clipping rectangle.
        bbox: Option<Rect>,
        /// Optional anti-alias preference.
        anti_alias: Option<bool>,
        /// Parsed mesh patches.
        patches: Vec<MeshPatch>,
    },
    /// Placeholder for unsupported shading types (Types 4-5).
    Unsupported { name: String },
}

/// A parsed patch mesh shading patch.
#[derive(Debug, Clone)]
pub enum MeshPatch {
    /// A Coons patch with 12 boundary control points.
    Coons {
        control_points: [Point; 12],
        corner_colors: [Color; 4],
    },
    /// A tensor-product patch with a 4x4 control net.
    Tensor {
        control_points: [Point; 16],
        corner_colors: [Color; 4],
    },
}

impl Shading {
    /// Returns the color space used by this shading, if available.
    pub fn color_space(&self) -> Option<&ColorSpace> {
        match self {
            Self::FunctionBased { color_space, .. } => color_space.as_ref(),
            Self::Axial { color_space, .. }
            | Self::Radial { color_space, .. }
            | Self::PatchMesh { color_space, .. } => Some(color_space),
            Self::Unsupported { .. } => None,
        }
    }
}

impl Shading {
    pub fn from_dictionary(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfPagesError> {
        // Extract and validate the required `/ShadingType` entry.
        let dictionary = object.try_dictionary(objects)?;
        let shading_type_value = dictionary
            .get_or_err("ShadingType")?
            .try_number::<i32>(objects)?;
        let shading_type = ShadingType::try_from(shading_type_value)?;

        match shading_type {
            ShadingType::FunctionBased => Self::parse_function_based(dictionary, objects),
            ShadingType::Axial => Self::parse_axial(object, objects),
            ShadingType::Radial => Self::parse_radial(object, objects),
            ShadingType::CoonsPatchMesh | ShadingType::TensorProductPatchMesh => {
                Self::parse_patch_mesh(object, objects, shading_type)
            }
            unsupported => Ok(Shading::Unsupported {
                name: unsupported.to_string(),
            }),
        }
    }
}

impl Shading {
    /// Parses a Type 1 (function-based) shading from a dictionary.
    fn parse_function_based(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfPagesError> {
        // Read optional `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?;

        // Read optional `/Background` entry (array of color components).
        let background = dictionary
            .get("Background")
            .map(|o| o.try_vec_of::<f32>(objects))
            .transpose()?;

        // Read optional `/BBox` entry (clipping rectangle).
        let bbox = dictionary
            .get("BBox")
            .map(|o| o.try_array_of::<f32, 4>(objects))
            .transpose()?
            .map(Rect::from);

        // Read optional `/Domain` entry (function input range).
        let domain = dictionary
            .get("Domain")
            .map(|o| o.try_vec_of::<f32>(objects))
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
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfPagesError> {
        // Read required `/Coords` entry defining the gradient axis.
        let dictionary = object.try_dictionary(objects)?;
        let coords = dictionary
            .get_or_err("Coords")?
            .try_array_of::<f32, 4>(objects)?;

        // Read required `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
            PdfPagesError::MissingRequiredEntry {
                entry: "ColorSpace",
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
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfPagesError> {
        // Read required `/Coords` entry defining the two circles.
        let dictionary = object.try_dictionary(objects)?;
        let coords = dictionary
            .get_or_err("Coords")?
            .try_array_of::<f32, 6>(objects)?;

        // Read required `/ColorSpace` entry.
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
            PdfPagesError::MissingRequiredEntry {
                entry: "ColorSpace",
            },
        )?;

        // Read optional `/BBox` entry.
        let bbox = dictionary
            .get("BBox")
            .map(|o| o.try_array_of::<f32, 4>(objects))
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

    fn parse_patch_mesh(
        object: &ObjectVariant,
        objects: &dyn ObjectResolver,
        shading_type: ShadingType,
    ) -> Result<Self, PdfPagesError> {
        let stream = object.try_stream(objects)?;
        let dictionary = stream.dictionary.as_ref();
        let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
            PdfPagesError::MissingRequiredEntry {
                entry: "ColorSpace",
            },
        )?;
        let bits_per_coordinate = dictionary
            .get_or_err("BitsPerCoordinate")?
            .try_number::<usize>(objects)?;
        let bits_per_component = dictionary
            .get_or_err("BitsPerComponent")?
            .try_number::<usize>(objects)?;
        let bits_per_flag = dictionary
            .get_or_err("BitsPerFlag")?
            .try_number::<usize>(objects)?;
        let decode_values = dictionary
            .get_or_err("Decode")?
            .try_vec_of::<f32>(objects)?;

        if decode_values.len() < 6 || decode_values.len() % 2 != 0 {
            return Err(PdfPagesError::InvalidShadingMeshData {
                reason: "Decode array must contain coordinate and color ranges".to_string(),
            });
        }

        let bbox = dictionary
            .get("BBox")
            .map(|o| o.try_array_of::<f32, 4>(objects))
            .transpose()?
            .map(Rect::from);
        let anti_alias = dictionary
            .get("AntiAlias")
            .map(|o| o.try_boolean(objects))
            .transpose()?;
        let functions = if dictionary.get("Function").is_some() {
            Self::parse_functions(dictionary, objects)?
        } else {
            Vec::new()
        };

        let component_inputs = decode_values
            .len()
            .checked_div(2)
            .and_then(|pairs| pairs.checked_sub(2))
            .ok_or(PdfPagesError::InvalidShadingMeshData {
                reason: "Decode array does not include component ranges".to_string(),
            })?;
        let color_input_count = if functions.is_empty() {
            color_space.num_color_components()
        } else {
            component_inputs
        };

        if color_input_count == 0 {
            return Err(PdfPagesError::InvalidShadingMeshData {
                reason: "Patch mesh color space requires at least one component".to_string(),
            });
        }

        let mut reader = MeshSampleReader::new(stream.raw_data());
        let mut patches = Vec::new();

        while let Some(flag_bits) = reader.read_bits(bits_per_flag)? {
            let flag =
                u8::try_from(flag_bits).map_err(|_| PdfPagesError::InvalidShadingMeshData {
                    reason: "Patch flag does not fit into u8".to_string(),
                })?;

            let patch = match shading_type {
                ShadingType::CoonsPatchMesh => {
                    let previous = match patches.last() {
                        Some(MeshPatch::Coons {
                            control_points,
                            corner_colors,
                        }) => Some((control_points, corner_colors)),
                        _ => None,
                    };
                    Self::read_coons_patch(
                        &mut reader,
                        bits_per_coordinate,
                        bits_per_component,
                        &decode_values,
                        color_input_count,
                        &functions,
                        &color_space,
                        flag,
                        previous,
                    )?
                }
                ShadingType::TensorProductPatchMesh => {
                    let previous = match patches.last() {
                        Some(MeshPatch::Tensor {
                            control_points,
                            corner_colors,
                        }) => Some((control_points, corner_colors)),
                        _ => None,
                    };
                    Self::read_tensor_patch(
                        &mut reader,
                        bits_per_coordinate,
                        bits_per_component,
                        &decode_values,
                        color_input_count,
                        &functions,
                        &color_space,
                        flag,
                        previous,
                    )?
                }
                _ => break,
            };
            patches.push(patch);
        }

        if patches.is_empty() {
            return Err(PdfPagesError::InvalidShadingMeshData {
                reason: "Patch mesh stream did not contain any patches".to_string(),
            });
        }

        Ok(Self::PatchMesh {
            shading_type,
            color_space,
            bbox,
            anti_alias,
            patches,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn read_coons_patch(
        reader: &mut MeshSampleReader<'_>,
        bits_per_coordinate: usize,
        bits_per_component: usize,
        decode_values: &[f32],
        color_input_count: usize,
        functions: &[Function],
        color_space: &ColorSpace,
        flag: u8,
        previous: Option<(&[Point; 12], &[Color; 4])>,
    ) -> Result<MeshPatch, PdfPagesError> {
        let mut control_points = if flag == 0 {
            Vec::with_capacity(12)
        } else {
            Vec::from(Self::coons_shared_edge_points(flag, previous)?)
        };

        while control_points.len() < 12 {
            control_points.push(Self::read_mesh_point(
                reader,
                bits_per_coordinate,
                decode_values,
            )?);
        }

        let mut corner_colors = if flag == 0 {
            Vec::with_capacity(4)
        } else {
            Vec::from(Self::coons_shared_edge_colors(flag, previous)?)
        };

        while corner_colors.len() < 4 {
            corner_colors.push(Self::read_mesh_color(
                reader,
                bits_per_component,
                decode_values,
                color_input_count,
                functions,
                color_space,
            )?);
        }

        let control_points: [Point; 12] =
            control_points
                .try_into()
                .map_err(|_| PdfPagesError::InvalidShadingMeshData {
                    reason: "Coons patch did not contain 12 control points".to_string(),
                })?;
        let corner_colors: [Color; 4] =
            corner_colors
                .try_into()
                .map_err(|_| PdfPagesError::InvalidShadingMeshData {
                    reason: "Coons patch did not contain 4 corner colors".to_string(),
                })?;

        Ok(MeshPatch::Coons {
            control_points,
            corner_colors,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn read_tensor_patch(
        reader: &mut MeshSampleReader<'_>,
        bits_per_coordinate: usize,
        bits_per_component: usize,
        decode_values: &[f32],
        color_input_count: usize,
        functions: &[Function],
        color_space: &ColorSpace,
        flag: u8,
        previous: Option<(&[Point; 16], &[Color; 4])>,
    ) -> Result<MeshPatch, PdfPagesError> {
        let mut control_points = if flag == 0 {
            Vec::with_capacity(16)
        } else {
            Vec::from(Self::tensor_shared_edge_points(flag, previous)?)
        };

        while control_points.len() < 16 {
            control_points.push(Self::read_mesh_point(
                reader,
                bits_per_coordinate,
                decode_values,
            )?);
        }

        let mut corner_colors = if flag == 0 {
            Vec::with_capacity(4)
        } else {
            Vec::from(Self::tensor_shared_edge_colors(flag, previous)?)
        };

        while corner_colors.len() < 4 {
            corner_colors.push(Self::read_mesh_color(
                reader,
                bits_per_component,
                decode_values,
                color_input_count,
                functions,
                color_space,
            )?);
        }

        let control_points: [Point; 16] =
            control_points
                .try_into()
                .map_err(|_| PdfPagesError::InvalidShadingMeshData {
                    reason: "Tensor patch did not contain 16 control points".to_string(),
                })?;
        let corner_colors: [Color; 4] =
            corner_colors
                .try_into()
                .map_err(|_| PdfPagesError::InvalidShadingMeshData {
                    reason: "Tensor patch did not contain 4 corner colors".to_string(),
                })?;

        Ok(MeshPatch::Tensor {
            control_points,
            corner_colors,
        })
    }

    fn read_mesh_point(
        reader: &mut MeshSampleReader<'_>,
        bits_per_coordinate: usize,
        decode_values: &[f32],
    ) -> Result<Point, PdfPagesError> {
        let x = Self::decode_mesh_sample(
            reader.read_required_bits(bits_per_coordinate)?.into(),
            bits_per_coordinate,
            *decode_values
                .first()
                .ok_or(PdfPagesError::InvalidShadingMeshData {
                    reason: "Decode array is missing X min".to_string(),
                })?,
            *decode_values
                .get(1)
                .ok_or(PdfPagesError::InvalidShadingMeshData {
                    reason: "Decode array is missing X max".to_string(),
                })?,
        )?;
        let y = Self::decode_mesh_sample(
            reader.read_required_bits(bits_per_coordinate)?.into(),
            bits_per_coordinate,
            *decode_values
                .get(2)
                .ok_or(PdfPagesError::InvalidShadingMeshData {
                    reason: "Decode array is missing Y min".to_string(),
                })?,
            *decode_values
                .get(3)
                .ok_or(PdfPagesError::InvalidShadingMeshData {
                    reason: "Decode array is missing Y max".to_string(),
                })?,
        )?;
        Ok(Point::new(x, y))
    }

    fn read_mesh_color(
        reader: &mut MeshSampleReader<'_>,
        bits_per_component: usize,
        decode_values: &[f32],
        color_input_count: usize,
        functions: &[Function],
        color_space: &ColorSpace,
    ) -> Result<Color, PdfPagesError> {
        let mut inputs = Vec::with_capacity(color_input_count);
        for component_index in 0..color_input_count {
            let pair_index = component_index.saturating_add(2).saturating_mul(2);
            let min =
                *decode_values
                    .get(pair_index)
                    .ok_or(PdfPagesError::InvalidShadingMeshData {
                        reason: "Decode array is missing component minimum".to_string(),
                    })?;
            let max = *decode_values.get(pair_index.saturating_add(1)).ok_or(
                PdfPagesError::InvalidShadingMeshData {
                    reason: "Decode array is missing component maximum".to_string(),
                },
            )?;
            let value = Self::decode_mesh_sample(
                reader.read_required_bits(bits_per_component)?.into(),
                bits_per_component,
                min,
                max,
            )?;
            inputs.push(value);
        }

        let components = if functions.is_empty() {
            inputs
        } else if functions.len() == 1 {
            functions
                .first()
                .ok_or(PdfPagesError::InvalidShadingMeshData {
                    reason: "Mesh shading function vector was unexpectedly empty".to_string(),
                })?
                .apply(&inputs)?
        } else {
            let mut outputs = Vec::with_capacity(functions.len());
            for function in functions {
                let mut values = function.apply(&inputs)?;
                let value =
                    values
                        .drain(..1)
                        .next()
                        .ok_or(PdfPagesError::InvalidShadingMeshData {
                            reason: "Mesh shading function did not return any values".to_string(),
                        })?;
                outputs.push(value);
            }
            outputs
        };

        Ok(color_space.apply(&components)?)
    }

    fn decode_mesh_sample(
        code: u64,
        bits_per_sample: usize,
        min: f32,
        max: f32,
    ) -> Result<f32, PdfPagesError> {
        if bits_per_sample == 0 || bits_per_sample > 32 {
            return Err(PdfPagesError::InvalidShadingMeshData {
                reason: "BitsPerCoordinate/BitsPerComponent must be in 1..=32".to_string(),
            });
        }
        let shift =
            u32::try_from(bits_per_sample).map_err(|_| PdfPagesError::InvalidShadingMeshData {
                reason: "Bits per sample value is too large".to_string(),
            })?;
        let max_value = 1u64
            .checked_shl(shift)
            .ok_or(PdfPagesError::InvalidShadingMeshData {
                reason: "Bits per sample shift overflowed".to_string(),
            })?
            .saturating_sub(1);
        if max_value == 0 {
            return Ok(min);
        }
        let normalized = code as f32 / max_value as f32;
        Ok(min + normalized * (max - min))
    }

    fn coons_shared_edge_points(
        flag: u8,
        previous: Option<(&[Point; 12], &[Color; 4])>,
    ) -> Result<[Point; 4], PdfPagesError> {
        let (points, _) = previous.ok_or(PdfPagesError::InvalidShadingMeshData {
            reason: format!("Coons continuation flag {flag} used without a previous patch"),
        })?;
        match flag {
            1 => Ok([points[3], points[4], points[5], points[6]]),
            2 => Ok([points[6], points[7], points[8], points[9]]),
            3 => Ok([points[9], points[10], points[11], points[0]]),
            _ => Err(PdfPagesError::InvalidShadingMeshData {
                reason: format!("Unsupported Coons continuation flag {flag}"),
            }),
        }
    }

    fn coons_shared_edge_colors(
        flag: u8,
        previous: Option<(&[Point; 12], &[Color; 4])>,
    ) -> Result<[Color; 2], PdfPagesError> {
        let (_, colors) = previous.ok_or(PdfPagesError::InvalidShadingMeshData {
            reason: format!("Coons continuation flag {flag} used without a previous patch"),
        })?;
        match flag {
            1 => Ok([colors[1], colors[2]]),
            2 => Ok([colors[2], colors[3]]),
            3 => Ok([colors[3], colors[0]]),
            _ => Err(PdfPagesError::InvalidShadingMeshData {
                reason: format!("Unsupported Coons continuation flag {flag}"),
            }),
        }
    }

    fn tensor_shared_edge_points(
        flag: u8,
        previous: Option<(&[Point; 16], &[Color; 4])>,
    ) -> Result<[Point; 4], PdfPagesError> {
        let (points, _) = previous.ok_or(PdfPagesError::InvalidShadingMeshData {
            reason: format!("Tensor continuation flag {flag} used without a previous patch"),
        })?;
        match flag {
            1 => Ok([points[3], points[7], points[11], points[15]]),
            2 => Ok([points[15], points[14], points[13], points[12]]),
            3 => Ok([points[12], points[8], points[4], points[0]]),
            _ => Err(PdfPagesError::InvalidShadingMeshData {
                reason: format!("Unsupported tensor continuation flag {flag}"),
            }),
        }
    }

    fn tensor_shared_edge_colors(
        flag: u8,
        previous: Option<(&[Point; 16], &[Color; 4])>,
    ) -> Result<[Color; 2], PdfPagesError> {
        let (_, colors) = previous.ok_or(PdfPagesError::InvalidShadingMeshData {
            reason: format!("Tensor continuation flag {flag} used without a previous patch"),
        })?;
        match flag {
            1 => Ok([colors[1], colors[2]]),
            2 => Ok([colors[2], colors[3]]),
            3 => Ok([colors[3], colors[0]]),
            _ => Err(PdfPagesError::InvalidShadingMeshData {
                reason: format!("Unsupported tensor continuation flag {flag}"),
            }),
        }
    }

    /// Parses one or more functions from the `/Function` entry.
    ///
    /// The `/Function` entry may be either:
    /// - A single function (dictionary or stream)
    /// - An array of functions
    fn parse_functions(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Vec<Function>, PdfPagesError> {
        let function_obj = objects.resolve_object(dictionary.get_or_err("Function")?)?;

        if let ObjectVariant::Array(array) = function_obj {
            // Parse array of functions.
            array
                .iter()
                .map(|value| Function::parse(value, objects).map_err(PdfPagesError::from))
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
            Shading::PatchMesh { bbox, .. } => bbox.as_ref(),
            _ => None,
        }
    }
}

struct MeshSampleReader<'a> {
    data: &'a [u8],
    bit_offset: usize,
}

impl<'a> MeshSampleReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_offset: 0,
        }
    }

    fn read_required_bits(&mut self, bits: usize) -> Result<u32, PdfPagesError> {
        self.read_bits(bits)?
            .ok_or(PdfPagesError::InvalidShadingMeshData {
                reason: "Patch mesh stream ended unexpectedly".to_string(),
            })
    }

    fn read_bits(&mut self, bits: usize) -> Result<Option<u32>, PdfPagesError> {
        if bits == 0 || bits > 32 {
            return Err(PdfPagesError::InvalidShadingMeshData {
                reason: "Mesh bit widths must be in 1..=32".to_string(),
            });
        }

        let required_bits = self.bit_offset.saturating_add(bits);
        let required_bytes = required_bits.div_ceil(8);
        if self.bit_offset >= self.data.len().saturating_mul(8) {
            return Ok(None);
        }
        if required_bytes > self.data.len() {
            return Err(PdfPagesError::InvalidShadingMeshData {
                reason: "Patch mesh stream ended in the middle of a sample".to_string(),
            });
        }

        let mut value = 0u32;
        for bit_index in 0..bits {
            let absolute_bit = self.bit_offset.saturating_add(bit_index);
            let byte_index = absolute_bit / 8;
            let bit_in_byte = absolute_bit % 8;
            let byte = self.data.get(byte_index).copied().ok_or(
                PdfPagesError::InvalidShadingMeshData {
                    reason: "Patch mesh stream byte access overflowed".to_string(),
                },
            )?;
            let bit = (byte >> (7usize.saturating_sub(bit_in_byte))) & 1;
            value <<= 1;
            value |= u32::from(bit);
        }
        self.bit_offset = self.bit_offset.saturating_add(bits);
        Ok(Some(value))
    }
}
