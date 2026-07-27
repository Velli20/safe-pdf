//! Parsing for Type 6 Coons and Type 7 tensor-product patch meshes.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::{color::Color, point::Point, rect::Rect};
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};
use pdf_utils::BitReader;

use crate::{
    error::PdfShadingError,
    mesh_decoder::{MeshBitWidths, MeshDecoder, read_mesh_bits},
    model::{MeshPatch, Shading, ShadingType},
    parse::{optional_bbox, parse_functions, required_color_space},
};

const COONS_CONTROL_POINTS: usize = 12;
const TENSOR_CONTROL_POINTS: usize = 16;
const CORNER_COLORS: usize = 4;

/// Parses a Type 6 or Type 7 patch-mesh shading stream.
pub(crate) fn parse_patch_mesh(
    object: &ObjectVariant,
    objects: &dyn ObjectResolver,
    shading_type: ShadingType,
) -> Result<Shading, PdfShadingError> {
    let kind = PatchKind::from_shading_type(shading_type)?;
    let stream = object.try_stream(objects)?;
    let config = PatchMeshConfig::parse(stream.dictionary.as_ref(), objects)?;
    let patches = PatchMeshParser::new(stream.raw_data(), &config, kind)?.parse()?;

    Ok(Shading::PatchMesh {
        shading_type,
        color_space: config.color_space,
        bbox: config.bbox,
        anti_alias: config.anti_alias,
        patches,
    })
}

/// The geometry layout selected by `/ShadingType`.
#[derive(Debug, Clone, Copy)]
enum PatchKind {
    Coons,
    Tensor,
}

impl PatchKind {
    fn from_shading_type(shading_type: ShadingType) -> Result<Self, PdfShadingError> {
        match shading_type {
            ShadingType::CoonsPatchMesh => Ok(Self::Coons),
            ShadingType::TensorProductPatchMesh => Ok(Self::Tensor),
            _ => Err(invalid_mesh_data(format!(
                "{shading_type} is not a patch-mesh shading type"
            ))),
        }
    }
}

/// Owned dictionary values needed while parsing a patch stream.
struct PatchMeshConfig {
    color_space: ColorSpace,
    bbox: Option<Rect>,
    anti_alias: Option<bool>,
    widths: MeshBitWidths,
    decode: Vec<f32>,
    functions: Vec<Function>,
}

impl PatchMeshConfig {
    /// Reads and validates entries shared by Type 6 and Type 7 dictionaries.
    fn parse(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfShadingError> {
        let color_space = required_color_space(dictionary, objects)?;
        let widths = MeshBitWidths::new(
            dictionary.required_number::<usize>("BitsPerCoordinate", objects)?,
            dictionary.required_number::<usize>("BitsPerComponent", objects)?,
            dictionary.required_number::<usize>("BitsPerFlag", objects)?,
        )?;
        let decode = dictionary.required_vec_of::<f32>("Decode", objects)?;
        let bbox = optional_bbox(dictionary, objects)?;
        let anti_alias = dictionary.optional_boolean("AntiAlias", objects)?;
        let functions = optional_functions(dictionary, objects)?;

        Ok(Self {
            color_space,
            bbox,
            anti_alias,
            widths,
            decode,
            functions,
        })
    }
}

/// Stateful reconstruction of patches and their implicit shared edges.
struct PatchMeshParser<'a> {
    reader: BitReader<'a>,
    decoder: MeshDecoder<'a>,
    flag_width: usize,
    kind: PatchKind,
}

impl<'a> PatchMeshParser<'a> {
    /// Creates a parser borrowing the stream and its validated configuration.
    fn new(
        data: &'a [u8],
        config: &'a PatchMeshConfig,
        kind: PatchKind,
    ) -> Result<Self, PdfShadingError> {
        Ok(Self {
            reader: BitReader::new(data),
            decoder: MeshDecoder::new(
                config.widths,
                &config.decode,
                &config.functions,
                &config.color_space,
                "Patch mesh",
            )?,
            flag_width: config.widths.flag(),
            kind,
        })
    }

    /// Consumes the packed stream and reconstructs all patches.
    fn parse(mut self) -> Result<Vec<MeshPatch>, PdfShadingError> {
        let mut patches = Vec::new();

        while let Some(flag) = read_mesh_bits(&mut self.reader, self.flag_width)? {
            let flag = u8::try_from(flag & 0b11)
                .map_err(|_| invalid_mesh_data("Patch flag does not fit into u8"))?;
            let patch = self.read_patch(flag, patches.last())?;
            patches.push(patch);
        }

        if patches.is_empty() {
            return Err(invalid_mesh_data(
                "Patch mesh stream did not contain any patches",
            ));
        }

        Ok(patches)
    }

    fn read_patch(
        &mut self,
        flag: u8,
        previous: Option<&MeshPatch>,
    ) -> Result<MeshPatch, PdfShadingError> {
        match self.kind {
            PatchKind::Coons => self.read_coons_patch(flag, previous),
            PatchKind::Tensor => self.read_tensor_patch(flag, previous),
        }
    }

    /// Reads the explicit values of a Coons patch and prepends any shared edge.
    fn read_coons_patch(
        &mut self,
        flag: u8,
        previous: Option<&MeshPatch>,
    ) -> Result<MeshPatch, PdfShadingError> {
        let mut control_points = initial_coons_points(flag, previous)?;
        self.read_points(&mut control_points, COONS_CONTROL_POINTS)?;

        let mut corner_colors = initial_coons_colors(flag, previous)?;
        self.read_colors(&mut corner_colors, CORNER_COLORS)?;

        Ok(MeshPatch::Coons {
            control_points: try_array(control_points, "12 Coons control points")?,
            corner_colors: try_array(corner_colors, "4 Coons corner colors")?,
        })
    }

    /// Reads the explicit values of a tensor patch and prepends any shared edge.
    fn read_tensor_patch(
        &mut self,
        flag: u8,
        previous: Option<&MeshPatch>,
    ) -> Result<MeshPatch, PdfShadingError> {
        let mut control_points = initial_tensor_points(flag, previous)?;
        self.read_points(&mut control_points, TENSOR_CONTROL_POINTS)?;

        let mut corner_colors = initial_tensor_colors(flag, previous)?;
        self.read_colors(&mut corner_colors, CORNER_COLORS)?;

        Ok(MeshPatch::Tensor {
            control_points: try_array(control_points, "16 tensor control points")?,
            corner_colors: try_array(corner_colors, "4 tensor corner colors")?,
        })
    }

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

    fn read_colors(
        &mut self,
        colors: &mut Vec<Color>,
        required: usize,
    ) -> Result<(), PdfShadingError> {
        while colors.len() < required {
            colors.push(self.decoder.read_color(&mut self.reader)?);
        }
        Ok(())
    }
}

fn initial_coons_points(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<Vec<Point>, PdfShadingError> {
    if flag == 0 {
        return Ok(Vec::with_capacity(COONS_CONTROL_POINTS));
    }
    let (points, _) = previous_coons_patch(flag, previous)?;
    let &[p0, _p1, _p2, p3, p4, p5, p6, p7, p8, p9, p10, p11] = points;
    let shared = match flag {
        1 => [p3, p4, p5, p6],
        2 => [p6, p7, p8, p9],
        3 => [p9, p10, p11, p0],
        _ => return Err(invalid_continuation("Coons", flag)),
    };
    Ok(Vec::from(shared))
}

fn initial_coons_colors(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<Vec<Color>, PdfShadingError> {
    if flag == 0 {
        return Ok(Vec::with_capacity(CORNER_COLORS));
    }
    let (_, colors) = previous_coons_patch(flag, previous)?;
    Ok(Vec::from(shared_colors(flag, *colors, "Coons")?))
}

fn initial_tensor_points(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<Vec<Point>, PdfShadingError> {
    if flag == 0 {
        return Ok(Vec::with_capacity(TENSOR_CONTROL_POINTS));
    }
    let (points, _) = previous_tensor_patch(flag, previous)?;
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
    ] = points;
    let shared = match flag {
        1 => [p3, p7, p11, p15],
        2 => [p15, p14, p13, p12],
        3 => [p12, p8, p4, p0],
        _ => return Err(invalid_continuation("Tensor", flag)),
    };
    Ok(Vec::from(shared))
}

fn initial_tensor_colors(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<Vec<Color>, PdfShadingError> {
    if flag == 0 {
        return Ok(Vec::with_capacity(CORNER_COLORS));
    }
    let (_, colors) = previous_tensor_patch(flag, previous)?;
    Ok(Vec::from(shared_colors(flag, *colors, "tensor")?))
}

fn shared_colors(
    flag: u8,
    [c0, c1, c2, c3]: [Color; 4],
    kind: &str,
) -> Result<[Color; 2], PdfShadingError> {
    match flag {
        1 => Ok([c1, c2]),
        2 => Ok([c2, c3]),
        3 => Ok([c3, c0]),
        _ => Err(invalid_continuation(kind, flag)),
    }
}

fn previous_coons_patch(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<(&[Point; 12], &[Color; 4]), PdfShadingError> {
    match previous {
        Some(MeshPatch::Coons {
            control_points,
            corner_colors,
        }) => Ok((control_points, corner_colors)),
        _ => Err(missing_previous_patch("Coons", flag)),
    }
}

fn previous_tensor_patch(
    flag: u8,
    previous: Option<&MeshPatch>,
) -> Result<(&[Point; 16], &[Color; 4]), PdfShadingError> {
    match previous {
        Some(MeshPatch::Tensor {
            control_points,
            corner_colors,
        }) => Ok((control_points, corner_colors)),
        _ => Err(missing_previous_patch("Tensor", flag)),
    }
}

fn try_array<T, const N: usize>(values: Vec<T>, expected: &str) -> Result<[T; N], PdfShadingError> {
    values
        .try_into()
        .map_err(|_| invalid_mesh_data(format!("Patch did not contain {expected}")))
}

fn optional_functions(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Vec<Function>, PdfShadingError> {
    if dictionary.get("Function").is_some() {
        parse_functions(dictionary, objects)
    } else {
        Ok(Vec::new())
    }
}

fn missing_previous_patch(kind: &str, flag: u8) -> PdfShadingError {
    invalid_mesh_data(format!(
        "{kind} continuation flag {flag} used without a previous patch"
    ))
}

fn invalid_continuation(kind: &str, flag: u8) -> PdfShadingError {
    invalid_mesh_data(format!("Unsupported {kind} continuation flag {flag}"))
}

fn invalid_mesh_data(reason: impl Into<String>) -> PdfShadingError {
    PdfShadingError::InvalidShadingMeshData {
        reason: reason.into(),
    }
}
