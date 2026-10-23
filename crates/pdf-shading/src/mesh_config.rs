//! Stream configuration shared by Type 4, 6, and 7 mesh shadings.

use bytes::Bytes;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::rect::Rect;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{error::PdfShadingError, mesh_bit_widths::MeshBitWidths, mesh_decoder::MeshDecoder};

/// Validated, owned values from a mesh shading stream.
///
/// Keeping the dictionary data together lets the stream parsers borrow one
/// stable configuration while they decode every vertex or patch.
pub(crate) struct MeshConfig {
    /// The shading color space.
    pub(crate) color_space: ColorSpace,
    /// Optional mesh bounding box in shading space.
    pub(crate) bbox: Option<Rect>,
    /// Optional anti-aliasing preference.
    pub(crate) anti_alias: Option<bool>,
    /// Bit widths of the packed stream fields.
    widths: MeshBitWidths,
    /// Ranges the packed coordinates and color components are mapped onto.
    decode: Vec<f32>,
    /// Functions mapping one parametric value to a color; empty when colors are explicit.
    functions: Vec<Function>,
    /// The packed mesh data.
    pub(crate) data: Bytes,
}

/// Reads the mesh entries of a Type 4, 6, or 7 shading stream.
impl FromPdfObject for MeshConfig {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let object = context.object();
        let mut context = context.stream()?;
        let data = context.stream().data.clone();
        let mut dictionary = context.dictionary();
        Ok(Self {
            color_space: dictionary.required(b"ColorSpace")?,
            bbox: dictionary
                .lenient::<Rect>(b"BBox")
                .map(|bbox| bbox.normalized()),
            anti_alias: dictionary.optional(b"AntiAlias")?,
            widths: dictionary.read(object)?,
            decode: dictionary.required(b"Decode")?,
            functions: dictionary.optional_list(b"Function")?,
            data,
        })
    }
}

impl MeshConfig {
    /// Creates a sample decoder borrowing this configuration.
    pub(crate) fn decoder(
        &self,
        mesh_name: &'static str,
    ) -> Result<MeshDecoder<'_>, PdfShadingError> {
        MeshDecoder::new(
            self.widths,
            &self.decode,
            &self.functions,
            &self.color_space,
            mesh_name,
        )
    }

    /// Returns the encoded width of each vertex or patch flag.
    pub(crate) fn flag_width(&self) -> usize {
        self.widths.flag()
    }
}
