//! Stream configuration shared by Type 4, 6, and 7 mesh shadings.

use bytes::Bytes;
use pdf_color_space::color_space::ColorSpace;
use pdf_function::function::Function;
use pdf_graphics::rect::Rect;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};

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
    /// How the stream's records are packed; `None` when the shading is not a stream
    /// or does not describe the packing, so it has no records to decode.
    packing: Option<MeshPacking>,
    /// Functions mapping one parametric value to a color; empty when colors are explicit.
    functions: Vec<Function>,
    /// The packed mesh data.
    pub(crate) data: Bytes,
}

/// Reads the mesh entries of a Type 4, 6, or 7 shading stream.
impl FromPdfObject for MeshConfig {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        // Records are packed in a stream described by its bit widths and decode
        // ranges. Without either there is nothing to read, and the shading paints
        // nothing rather than failing its page.
        let (packing, data) = match context.object() {
            ObjectVariant::Stream(stream) => (
                context.reborrow().decode::<MeshPacking>().ok(),
                stream.data.clone(),
            ),
            _ => (None, Bytes::new()),
        };
        let mut dictionary = context.entries()?;
        Ok(Self {
            color_space: dictionary.required(b"ColorSpace")?,
            bbox: dictionary
                .lenient::<Rect>(b"BBox")
                .map(|bbox| bbox.normalized()),
            anti_alias: dictionary.optional(b"AntiAlias")?,
            packing,
            functions: dictionary.optional_list(b"Function")?,
            data,
        })
    }
}

/// The layout of a mesh stream's packed records.
struct MeshPacking {
    /// Bit widths of the packed stream fields.
    widths: MeshBitWidths,
    /// Ranges the packed coordinates and color components are mapped onto.
    decode: Vec<f32>,
}

/// Reads the bit widths and the required `/Decode` array of a mesh shading stream.
impl FromPdfObject for MeshPacking {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let widths = context.reborrow().decode()?;
        Ok(Self {
            widths,
            decode: context.dictionary()?.required(b"Decode")?,
        })
    }
}

impl MeshConfig {
    /// Creates a decoder over this stream's packed records, or `None` when the
    /// stream's packing is not described and it has no records.
    pub(crate) fn decoder(
        &self,
        mesh_name: &'static str,
    ) -> Result<Option<MeshDecoder<'_>>, PdfShadingError> {
        let Some(packing) = &self.packing else {
            return Ok(None);
        };
        MeshDecoder::new(
            &self.data,
            packing.widths,
            &packing.decode,
            &self.functions,
            &self.color_space,
            mesh_name,
        )
        .map(Some)
    }
}
