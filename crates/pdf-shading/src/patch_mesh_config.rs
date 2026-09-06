//! Dictionary configuration for Type 6 and Type 7 patch meshes.

use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::rect::Rect;
use pdf_object_reader::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::{
    error::PdfShadingError,
    mesh_bit_widths::MeshBitWidths,
    mesh_decoder::MeshDecoder,
    model::{MeshPatch, Shading, ShadingType},
    parse::{parse_functions, required_color_space},
};

/// Validated, owned values from a patch-mesh shading dictionary.
///
/// Keeping the dictionary data together lets the stream parser borrow one
/// stable configuration while it decodes every patch.
pub(crate) struct PatchMeshConfig {
    color_space: ColorSpace,
    bbox: Option<Rect>,
    anti_alias: Option<bool>,
    widths: MeshBitWidths,
    decode: Vec<f32>,
    functions: Vec<Function>,
}

impl PatchMeshConfig {
    /// Reads and validates entries shared by Type 6 and Type 7 dictionaries.
    pub(crate) fn parse(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, PdfShadingError> {
        let color_space = required_color_space(dictionary, objects)?;
        let widths = MeshBitWidths::from_dictionary(dictionary, objects)?;
        let decode = dictionary.required_vec_of::<f32>(b"Decode", objects)?;
        let bbox = dictionary.optional_bbox(objects)?;
        let anti_alias = dictionary.optional_boolean(b"AntiAlias", objects)?;
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

    /// Creates a sample decoder borrowing this configuration.
    pub(crate) fn decoder(&self) -> Result<MeshDecoder<'_>, PdfShadingError> {
        MeshDecoder::new(
            self.widths,
            &self.decode,
            &self.functions,
            &self.color_space,
            "Patch mesh",
        )
    }

    /// Returns the encoded width of each patch flag.
    pub(crate) fn flag_width(&self) -> usize {
        self.widths.flag()
    }

    /// Combines this dictionary metadata with the reconstructed patches.
    pub(crate) fn into_shading(
        self,
        shading_type: ShadingType,
        patches: Vec<MeshPatch>,
    ) -> Shading {
        Shading::PatchMesh {
            shading_type,
            color_space: self.color_space,
            bbox: self.bbox,
            anti_alias: self.anti_alias,
            patches,
        }
    }
}

/// Reads an optional `/Function` entry as the parser's uniform function list.
fn optional_functions(
    dictionary: &Dictionary,
    objects: &dyn ObjectResolver,
) -> Result<Vec<Function>, PdfShadingError> {
    if dictionary.get(b"Function").is_some() {
        parse_functions(dictionary, objects)
    } else {
        Ok(Vec::new())
    }
}
