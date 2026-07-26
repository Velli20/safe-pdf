//! Parsing for Type 4 free-form Gouraud triangle meshes.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::rect::Rect;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
    object_variant::ObjectVariant,
};

use crate::{
    error::PdfShadingError,
    mesh_decoder::{MeshBitWidths, MeshDecoder},
    mesh_sample_reader::MeshSampleReader,
    model::{MeshTriangle, MeshVertex, Shading},
    parse::{optional_bbox, parse_functions, required_color_space},
};

/// Parses a Type 4 shading stream into independent mesh triangles.
pub(crate) fn parse_free_form_triangle_mesh(
    object: &ObjectVariant,
    objects: &dyn ObjectResolver,
) -> Result<Shading, PdfShadingError> {
    let stream = object.try_stream(objects)?;
    let config = FreeFormMeshConfig::parse(stream.dictionary.as_ref(), objects)?;
    let triangles = FreeFormMeshParser::new(stream.raw_data(), &config)?.parse()?;

    Ok(Shading::FreeFormTriangleMesh {
        color_space: config.color_space,
        bbox: config.bbox,
        anti_alias: config.anti_alias,
        triangles,
    })
}

/// Owned dictionary values needed while parsing a Type 4 stream.
struct FreeFormMeshConfig {
    color_space: ColorSpace,
    bbox: Option<Rect>,
    anti_alias: Option<bool>,
    widths: MeshBitWidths,
    decode: Vec<f32>,
    functions: Vec<Function>,
}

impl FreeFormMeshConfig {
    /// Reads and validates the mesh-specific shading dictionary entries.
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

/// Stateful reconstruction of triangles from Type 4 vertex records.
struct FreeFormMeshParser<'a> {
    reader: MeshSampleReader<'a>,
    decoder: MeshDecoder<'a>,
    flag_width: usize,
}

impl<'a> FreeFormMeshParser<'a> {
    /// Creates a parser borrowing the stream and its validated configuration.
    fn new(data: &'a [u8], config: &'a FreeFormMeshConfig) -> Result<Self, PdfShadingError> {
        Ok(Self {
            reader: MeshSampleReader::new(data),
            decoder: MeshDecoder::new(
                config.widths,
                &config.decode,
                &config.functions,
                &config.color_space,
                "Free-form triangle mesh",
            )?,
            flag_width: config.widths.flag(),
        })
    }

    /// Consumes all complete vertex records and reconstructs their triangles.
    fn parse(mut self) -> Result<Vec<MeshTriangle>, PdfShadingError> {
        let mut triangles = Vec::new();

        while let Some((flag, vertex)) = self.read_vertex()? {
            let triangle = match flag {
                0 => self.read_new_triangle(vertex)?,
                1 | 2 => {
                    let previous = triangles.last().ok_or_else(|| {
                        invalid_mesh_data(format!(
                            "Free-form triangle continuation flag {flag} used without a previous triangle"
                        ))
                    })?;
                    continue_triangle(flag, previous, vertex)?
                }
                _ => {
                    return Err(invalid_mesh_data(format!(
                        "Invalid free-form triangle edge flag {flag}"
                    )));
                }
            };
            triangles.push(triangle);
        }

        if triangles.is_empty() {
            return Err(invalid_mesh_data(
                "Free-form triangle mesh stream did not contain any triangles",
            ));
        }

        Ok(triangles)
    }

    /// Reads the two remaining vertices of a newly started triangle.
    fn read_new_triangle(&mut self, first: MeshVertex) -> Result<MeshTriangle, PdfShadingError> {
        let (_, second) = self.read_required_vertex()?;
        let (_, third) = self.read_required_vertex()?;
        Ok(MeshTriangle {
            vertices: [first, second, third],
        })
    }

    /// Reads a vertex that must exist to complete the current triangle.
    fn read_required_vertex(&mut self) -> Result<(u8, MeshVertex), PdfShadingError> {
        self.read_vertex()?.ok_or_else(|| {
            invalid_mesh_data("Free-form triangle mesh stream ended before completing a triangle")
        })
    }

    /// Reads one byte-aligned Type 4 vertex record.
    fn read_vertex(&mut self) -> Result<Option<(u8, MeshVertex)>, PdfShadingError> {
        let Some(flag) = self.reader.read_bits(self.flag_width)? else {
            return Ok(None);
        };
        let flag = u8::try_from(flag & 0b11)
            .map_err(|_| invalid_mesh_data("Free-form triangle flag does not fit into u8"))?;
        let point = self.decoder.read_point(&mut self.reader)?;
        let color = self.decoder.read_color(&mut self.reader)?;
        self.reader.align_to_byte();

        Ok(Some((flag, MeshVertex { point, color })))
    }
}

/// Reuses the appropriate edge of the preceding triangle.
fn continue_triangle(
    flag: u8,
    previous: &MeshTriangle,
    vertex: MeshVertex,
) -> Result<MeshTriangle, PdfShadingError> {
    let [first, second, third] = previous.vertices;
    let vertices = match flag {
        1 => [second, third, vertex],
        2 => [first, third, vertex],
        _ => {
            return Err(invalid_mesh_data(format!(
                "Invalid free-form triangle continuation flag {flag}"
            )));
        }
    };
    Ok(MeshTriangle { vertices })
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

fn invalid_mesh_data(reason: impl Into<String>) -> PdfShadingError {
    PdfShadingError::InvalidShadingMeshData {
        reason: reason.into(),
    }
}
