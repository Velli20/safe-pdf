//! Type 3 specifications and glyph procedures decoded in the font's traversal.

use crate::{
    encoding::simple_encoding,
    error::FontError,
    font::{FontMetadata, GlyphId, GlyphName},
    pdf::{PdfMetrics, SimpleEncoding, ToUnicodeMap},
    pdf_font_parser::{base_font, to_unicode},
};
use pdf_content_stream::ContentStream;
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

/// Normalized Type 3 font whose glyph programs remain owned by the PDF layer.
#[derive(Clone)]
pub struct Type3FontSpec {
    /// PDF base font name without a leading slash.
    pub base_font: Arc<[u8]>,
    /// Font matching metadata inferred from the PDF resource.
    pub metadata: FontMetadata,
    /// Matrix mapping Type 3 glyph space to text space.
    pub font_matrix: Transform,
    /// Declared Type 3 font bounds.
    pub bounds: Rect,
    /// One-byte character encoding.
    pub encoding: SimpleEncoding,
    /// PDF width data.
    pub metrics: PdfMetrics,
    /// Opaque character procedure handles indexed by glyph name.
    pub char_procedures: Arc<BTreeMap<GlyphName, GlyphId>>,
    /// Parsed PDF content streams indexed by their opaque glyph handles.
    pub type3_procedures: Arc<HashMap<GlyphId, ContentStream>>,
    /// Optional source-code-to-Unicode map.
    pub to_unicode: Option<Arc<dyn ToUnicodeMap>>,
}

/// Glyph procedures and their normalized handles, decoded in the CharProcs scope.
struct CharProcedures {
    handles: BTreeMap<GlyphName, GlyphId>,
    streams: HashMap<GlyphId, ContentStream>,
}

impl FromPdfObject for CharProcedures {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let mut handles = BTreeMap::new();
        let mut streams = HashMap::new();
        for name in context.dictionary().dictionary.keys() {
            // Every procedure shares the font's traversal and document-wide stream IDs.
            let stream: ContentStream = context.required(name)?;
            let handle = GlyphId(u32::try_from(stream.id).map_err(|_| {
                FontError::InvalidDescendantFonts("Type 3 content stream ID does not fit u32")
            })?);
            handles.insert(GlyphName(Arc::from(name.as_slice())), handle);
            streams.insert(handle, stream);
        }
        Ok(Self { handles, streams })
    }
}

impl FromPdfObject for Type3FontSpec {
    /// Decodes a Type 3 dictionary without whole-font substitution; absent glyph widths
    /// default to zero.
    #[allow(clippy::arc_with_non_send_sync)]
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object().value();
        let mut context = context.dictionary()?;
        let [a, b, c, d, e, f] = context.required::<[f32; 6]>(b"FontMatrix")?;
        let bounds = context
            .dictionary()
            .required_rect(b"FontBBox", context.source())?;
        let encoding = simple_encoding(&mut context, false)?;
        let metrics: PdfMetrics = context.read(value)?;
        let procedures: CharProcedures = context.required(b"CharProcs")?;
        Ok(Self {
            base_font: base_font(&mut context).unwrap_or_default(),
            metadata: FontMetadata::default(),
            font_matrix: Transform::from_row(a, b, c, d, e, f),
            bounds,
            encoding,
            metrics,
            char_procedures: Arc::new(procedures.handles),
            type3_procedures: Arc::new(procedures.streams),
            to_unicode: to_unicode(&mut context)?,
        })
    }
}
