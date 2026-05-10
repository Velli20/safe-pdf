//! Parsing helpers for PDF shading dictionaries and streams.

use num_traits::ToPrimitive;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::{Function, FunctionImpl};
use pdf_graphics::{color::Color, point::Point, rect::Rect};
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    color_stops::ColorStops,
    error::PdfShadingError,
    model::{MeshPatch, Shading, ShadingType},
};

/// Parses a PDF shading object from a dictionary or stream object.
pub fn shading_from_dictionary(
    object: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let dictionary = object.try_dictionary(objects)?;
    let shading_type_value = dictionary
        .get_or_err("ShadingType")?
        .try_number::<i32>(objects)?;
    let shading_type = ShadingType::try_from(shading_type_value)?;

    match shading_type {
        ShadingType::FunctionBased => parse_function_based(dictionary, objects),
        ShadingType::Axial => parse_axial(object, objects),
        ShadingType::Radial => parse_radial(object, objects),
        ShadingType::CoonsPatchMesh | ShadingType::TensorProductPatchMesh => {
            parse_patch_mesh(object, objects, shading_type)
        }
        unsupported => Ok(Shading::Unsupported {
            name: unsupported.to_string(),
        }),
    }
}

fn parse_function_based(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let color_space = ColorSpace::from_dictionary(dictionary, objects)?;
    let background = dictionary
        .get("Background")
        .map(|object| object.try_vec_of::<f32>(objects))
        .transpose()?;
    let bbox = dictionary
        .get("BBox")
        .map(|object| object.try_array_of::<f32, 4>(objects))
        .transpose()?
        .map(Rect::from);
    let domain = dictionary
        .get("Domain")
        .map(|object| object.try_vec_of::<f32>(objects))
        .transpose()?;
    let anti_alias = dictionary
        .get("AntiAlias")
        .map(|object| object.try_boolean(objects))
        .transpose()?;
    let functions = parse_functions(dictionary, objects)?;

    Ok(Shading::FunctionBased {
        color_space,
        background,
        bbox,
        anti_alias,
        domain,
        functions,
    })
}

fn parse_axial(
    object: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let dictionary = object.try_dictionary(objects)?;
    let coords = dictionary
        .get_or_err("Coords")?
        .try_array_of::<f32, 4>(objects)?;
    let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
        PdfShadingError::MissingRequiredEntry {
            entry: "ColorSpace",
        },
    )?;
    let function_object = dictionary.get_or_err("Function")?;
    let function = Function::parse(function_object, objects)?;
    let color_stops = ColorStops::from_function(&function, &color_space)?;

    Ok(Shading::Axial {
        color_space,
        coords,
        color_stops,
    })
}

fn parse_radial(
    object: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let dictionary = object.try_dictionary(objects)?;
    let coords = dictionary
        .get_or_err("Coords")?
        .try_array_of::<f32, 6>(objects)?;
    let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
        PdfShadingError::MissingRequiredEntry {
            entry: "ColorSpace",
        },
    )?;
    let bbox = dictionary
        .get("BBox")
        .map(|object| object.try_array_of::<f32, 4>(objects))
        .transpose()?
        .map(Rect::from);
    let function_object = dictionary.get_or_err("Function")?;
    let function = Function::parse(function_object, objects)?;
    let color_stops = ColorStops::from_function(&function, &color_space)?;

    Ok(Shading::Radial {
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
) -> Result<Shading, PdfShadingError> {
    let stream = object.try_stream(objects)?;
    let dictionary = stream.dictionary.as_ref();
    let color_space = ColorSpace::from_dictionary(dictionary, objects)?.ok_or(
        PdfShadingError::MissingRequiredEntry {
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
        return Err(PdfShadingError::InvalidShadingMeshData {
            reason: "Decode array must contain coordinate and color ranges".to_string(),
        });
    }

    let bbox = dictionary
        .get("BBox")
        .map(|object| object.try_array_of::<f32, 4>(objects))
        .transpose()?
        .map(Rect::from);
    let anti_alias = dictionary
        .get("AntiAlias")
        .map(|object| object.try_boolean(objects))
        .transpose()?;
    let functions = if dictionary.get("Function").is_some() {
        parse_functions(dictionary, objects)?
    } else {
        Vec::new()
    };

    let component_inputs = decode_values
        .len()
        .checked_div(2)
        .and_then(|pairs| pairs.checked_sub(2))
        .ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
            reason: "Decode array does not include component ranges".to_string(),
        })?;
    let color_input_count = if functions.is_empty() {
        color_space.num_color_components()
    } else {
        component_inputs
    };

    if color_input_count == 0 {
        return Err(PdfShadingError::InvalidShadingMeshData {
            reason: "Patch mesh color space requires at least one component".to_string(),
        });
    }

    let params = MeshDecodeParams {
        bits_per_coordinate,
        bits_per_component,
        decode_values: &decode_values,
        color_input_count,
        functions: &functions,
        color_space: &color_space,
    };

    let mut reader = MeshSampleReader::new(stream.raw_data());
    let mut patches = Vec::new();

    while let Some(flag_bits) = reader.read_bits(bits_per_flag)? {
        let flag =
            u8::try_from(flag_bits).map_err(|_| PdfShadingError::InvalidShadingMeshData {
                reason: "Patch flag does not fit into u8".to_string(),
            })?;

        let patch = match shading_type {
            ShadingType::CoonsPatchMesh => {
                let previous = match patches.last() {
                    Some(MeshPatch::Coons {
                        control_points,
                        corner_colors,
                    }) => Some(PreviousCoonsPatch {
                        control_points,
                        corner_colors,
                    }),
                    _ => None,
                };
                read_coons_patch(&mut reader, &params, flag, previous)?
            }
            ShadingType::TensorProductPatchMesh => {
                let previous = match patches.last() {
                    Some(MeshPatch::Tensor {
                        control_points,
                        corner_colors,
                    }) => Some(PreviousTensorPatch {
                        control_points,
                        corner_colors,
                    }),
                    _ => None,
                };
                read_tensor_patch(&mut reader, &params, flag, previous)?
            }
            _ => break,
        };
        patches.push(patch);
    }

    if patches.is_empty() {
        return Err(PdfShadingError::InvalidShadingMeshData {
            reason: "Patch mesh stream did not contain any patches".to_string(),
        });
    }

    Ok(Shading::PatchMesh {
        shading_type,
        color_space,
        bbox,
        anti_alias,
        patches,
    })
}

fn read_coons_patch(
    reader: &mut MeshSampleReader<'_>,
    params: &MeshDecodeParams<'_>,
    flag: u8,
    previous: Option<PreviousCoonsPatch<'_>>,
) -> Result<MeshPatch, PdfShadingError> {
    let mut control_points = if flag == 0 {
        Vec::with_capacity(12)
    } else {
        Vec::from(coons_shared_edge_points(flag, previous)?)
    };

    while control_points.len() < 12 {
        control_points.push(read_mesh_point(
            reader,
            params.bits_per_coordinate,
            params.decode_values,
        )?);
    }

    let mut corner_colors = if flag == 0 {
        Vec::with_capacity(4)
    } else {
        Vec::from(coons_shared_edge_colors(flag, previous)?)
    };

    while corner_colors.len() < 4 {
        corner_colors.push(read_mesh_color(reader, params)?);
    }

    let control_points: [Point; 12] =
        control_points
            .try_into()
            .map_err(|_| PdfShadingError::InvalidShadingMeshData {
                reason: "Coons patch did not contain 12 control points".to_string(),
            })?;
    let corner_colors: [Color; 4] =
        corner_colors
            .try_into()
            .map_err(|_| PdfShadingError::InvalidShadingMeshData {
                reason: "Coons patch did not contain 4 corner colors".to_string(),
            })?;

    Ok(MeshPatch::Coons {
        control_points,
        corner_colors,
    })
}

fn read_tensor_patch(
    reader: &mut MeshSampleReader<'_>,
    params: &MeshDecodeParams<'_>,
    flag: u8,
    previous: Option<PreviousTensorPatch<'_>>,
) -> Result<MeshPatch, PdfShadingError> {
    let mut control_points = if flag == 0 {
        Vec::with_capacity(16)
    } else {
        Vec::from(tensor_shared_edge_points(flag, previous)?)
    };

    while control_points.len() < 16 {
        control_points.push(read_mesh_point(
            reader,
            params.bits_per_coordinate,
            params.decode_values,
        )?);
    }

    let mut corner_colors = if flag == 0 {
        Vec::with_capacity(4)
    } else {
        Vec::from(tensor_shared_edge_colors(flag, previous)?)
    };

    while corner_colors.len() < 4 {
        corner_colors.push(read_mesh_color(reader, params)?);
    }

    let control_points: [Point; 16] =
        control_points
            .try_into()
            .map_err(|_| PdfShadingError::InvalidShadingMeshData {
                reason: "Tensor patch did not contain 16 control points".to_string(),
            })?;
    let corner_colors: [Color; 4] =
        corner_colors
            .try_into()
            .map_err(|_| PdfShadingError::InvalidShadingMeshData {
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
) -> Result<Point, PdfShadingError> {
    let (x_min, x_max) = decode_pair(decode_values, 0, "X")?;
    let (y_min, y_max) = decode_pair(decode_values, 1, "Y")?;
    let x = decode_mesh_sample(
        reader.read_required_bits(bits_per_coordinate)?.into(),
        bits_per_coordinate,
        x_min,
        x_max,
    )?;
    let y = decode_mesh_sample(
        reader.read_required_bits(bits_per_coordinate)?.into(),
        bits_per_coordinate,
        y_min,
        y_max,
    )?;
    Ok(Point::new(x, y))
}

fn read_mesh_color(
    reader: &mut MeshSampleReader<'_>,
    params: &MeshDecodeParams<'_>,
) -> Result<Color, PdfShadingError> {
    let mut inputs = Vec::with_capacity(params.color_input_count);
    for component_index in 0..params.color_input_count {
        let (min, max) = decode_pair(
            params.decode_values,
            component_index.saturating_add(2),
            "component",
        )?;
        let value = decode_mesh_sample(
            reader.read_required_bits(params.bits_per_component)?.into(),
            params.bits_per_component,
            min,
            max,
        )?;
        inputs.push(value);
    }

    let components =
        if params.functions.is_empty() {
            inputs
        } else if params.functions.len() == 1 {
            let function = params.functions.first().ok_or_else(|| {
                PdfShadingError::InvalidShadingMeshData {
                    reason: "Mesh shading function vector was unexpectedly empty".to_string(),
                }
            })?;
            function.apply(&inputs)?
        } else {
            let mut outputs = Vec::with_capacity(params.functions.len());
            for function in params.functions {
                let mut values = function.apply(&inputs)?;
                let value = values.drain(..1).next().ok_or_else(|| {
                    PdfShadingError::InvalidShadingMeshData {
                        reason: "Mesh shading function did not return any values".to_string(),
                    }
                })?;
                outputs.push(value);
            }
            outputs
        };

    Ok(params.color_space.apply(&components)?)
}

fn decode_pair(
    decode_values: &[f32],
    pair_index: usize,
    label: &str,
) -> Result<(f32, f32), PdfShadingError> {
    let first_index = pair_index.saturating_mul(2);
    let second_index = first_index.saturating_add(1);
    let min =
        *decode_values
            .get(first_index)
            .ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
                reason: format!("Decode array is missing {label} minimum"),
            })?;
    let max = *decode_values.get(second_index).ok_or_else(|| {
        PdfShadingError::InvalidShadingMeshData {
            reason: format!("Decode array is missing {label} maximum"),
        }
    })?;
    Ok((min, max))
}

fn decode_mesh_sample(
    code: u64,
    bits_per_sample: usize,
    min: f32,
    max: f32,
) -> Result<f32, PdfShadingError> {
    if bits_per_sample == 0 || bits_per_sample > 32 {
        return Err(PdfShadingError::InvalidShadingMeshData {
            reason: "BitsPerCoordinate/BitsPerComponent must be in 1..=32".to_string(),
        });
    }
    let shift =
        u32::try_from(bits_per_sample).map_err(|_| PdfShadingError::InvalidShadingMeshData {
            reason: "Bits per sample value is too large".to_string(),
        })?;
    let max_value = 1_u64
        .checked_shl(shift)
        .ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
            reason: "Bits per sample shift overflowed".to_string(),
        })?
        .saturating_sub(1);
    if max_value == 0 {
        return Ok(min);
    }
    let code_f32 = code
        .to_f32()
        .ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
            reason: "Mesh sample code could not be represented as f32".to_string(),
        })?;
    let max_value_f32 =
        max_value
            .to_f32()
            .ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
                reason: "Mesh sample range could not be represented as f32".to_string(),
            })?;
    let normalized = code_f32 / max_value_f32;
    Ok(min + normalized * (max - min))
}

fn coons_shared_edge_points(
    flag: u8,
    previous: Option<PreviousCoonsPatch<'_>>,
) -> Result<[Point; 4], PdfShadingError> {
    let previous = previous.ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
        reason: format!("Coons continuation flag {flag} used without a previous patch"),
    })?;
    let &[p0, _p1, _p2, p3, p4, p5, p6, p7, p8, p9, p10, p11] = previous.control_points;

    match flag {
        1 => Ok([p3, p4, p5, p6]),
        2 => Ok([p6, p7, p8, p9]),
        3 => Ok([p9, p10, p11, p0]),
        _ => Err(PdfShadingError::InvalidShadingMeshData {
            reason: format!("Unsupported Coons continuation flag {flag}"),
        }),
    }
}

fn coons_shared_edge_colors(
    flag: u8,
    previous: Option<PreviousCoonsPatch<'_>>,
) -> Result<[Color; 2], PdfShadingError> {
    let previous = previous.ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
        reason: format!("Coons continuation flag {flag} used without a previous patch"),
    })?;
    let [c0, c1, c2, c3] = *previous.corner_colors;

    match flag {
        1 => Ok([c1, c2]),
        2 => Ok([c2, c3]),
        3 => Ok([c3, c0]),
        _ => Err(PdfShadingError::InvalidShadingMeshData {
            reason: format!("Unsupported Coons continuation flag {flag}"),
        }),
    }
}

fn tensor_shared_edge_points(
    flag: u8,
    previous: Option<PreviousTensorPatch<'_>>,
) -> Result<[Point; 4], PdfShadingError> {
    let previous = previous.ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
        reason: format!("Tensor continuation flag {flag} used without a previous patch"),
    })?;
    let &[
        p0,
        _p1,
        _p2,
        p3,
        p4,
        _p5,
        _p6,
        p7,
        p8,
        _p9,
        _p10,
        p11,
        p12,
        p13,
        p14,
        p15,
    ] = previous.control_points;

    match flag {
        1 => Ok([p3, p7, p11, p15]),
        2 => Ok([p15, p14, p13, p12]),
        3 => Ok([p12, p8, p4, p0]),
        _ => Err(PdfShadingError::InvalidShadingMeshData {
            reason: format!("Unsupported tensor continuation flag {flag}"),
        }),
    }
}

fn tensor_shared_edge_colors(
    flag: u8,
    previous: Option<PreviousTensorPatch<'_>>,
) -> Result<[Color; 2], PdfShadingError> {
    let previous = previous.ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
        reason: format!("Tensor continuation flag {flag} used without a previous patch"),
    })?;
    let [c0, c1, c2, c3] = *previous.corner_colors;

    match flag {
        1 => Ok([c1, c2]),
        2 => Ok([c2, c3]),
        3 => Ok([c3, c0]),
        _ => Err(PdfShadingError::InvalidShadingMeshData {
            reason: format!("Unsupported tensor continuation flag {flag}"),
        }),
    }
}

fn parse_functions(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Vec<Function>, PdfShadingError> {
    let function_obj = objects.resolve_object(dictionary.get_or_err("Function")?)?;

    if let ObjectVariant::Array(array) = function_obj {
        array
            .iter()
            .map(|value| Function::parse(value, objects).map_err(PdfShadingError::from))
            .collect()
    } else {
        let function = Function::parse(function_obj, objects)?;
        Ok(vec![function])
    }
}

struct MeshDecodeParams<'a> {
    bits_per_coordinate: usize,
    bits_per_component: usize,
    decode_values: &'a [f32],
    color_input_count: usize,
    functions: &'a [Function],
    color_space: &'a ColorSpace,
}

#[derive(Clone, Copy)]
struct PreviousCoonsPatch<'a> {
    control_points: &'a [Point; 12],
    corner_colors: &'a [Color; 4],
}

#[derive(Clone, Copy)]
struct PreviousTensorPatch<'a> {
    control_points: &'a [Point; 16],
    corner_colors: &'a [Color; 4],
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

    fn read_required_bits(&mut self, bits: usize) -> Result<u32, PdfShadingError> {
        self.read_bits(bits)?
            .ok_or_else(|| PdfShadingError::InvalidShadingMeshData {
                reason: "Patch mesh stream ended unexpectedly".to_string(),
            })
    }

    fn read_bits(&mut self, bits: usize) -> Result<Option<u32>, PdfShadingError> {
        if bits == 0 || bits > 32 {
            return Err(PdfShadingError::InvalidShadingMeshData {
                reason: "Mesh bit widths must be in 1..=32".to_string(),
            });
        }

        let required_bits = self.bit_offset.saturating_add(bits);
        let required_bytes = required_bits.div_ceil(8);
        if self.bit_offset >= self.data.len().saturating_mul(8) {
            return Ok(None);
        }
        if required_bytes > self.data.len() {
            return Err(PdfShadingError::InvalidShadingMeshData {
                reason: "Patch mesh stream ended in the middle of a sample".to_string(),
            });
        }

        let mut value = 0_u32;
        for bit_index in 0..bits {
            let absolute_bit = self.bit_offset.saturating_add(bit_index);
            let byte_index = absolute_bit / 8;
            let bit_in_byte = absolute_bit % 8;
            let byte = self.data.get(byte_index).copied().ok_or_else(|| {
                PdfShadingError::InvalidShadingMeshData {
                    reason: "Patch mesh stream byte access overflowed".to_string(),
                }
            })?;
            let bit = (byte >> (7_usize.saturating_sub(bit_in_byte))) & 1;
            value <<= 1;
            value |= u32::from(bit);
        }
        self.bit_offset = self.bit_offset.saturating_add(bits);
        Ok(Some(value))
    }
}
