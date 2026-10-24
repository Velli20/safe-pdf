//! Type 3 specifications and glyph procedures decoded in the font's traversal.

use crate::{
    error::FontError,
    font::{FontMetadata, GlyphId, GlyphName},
    pdf::{PdfMetrics, SimpleEncoding, ToUnicodeMap},
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
    /// Character procedures and their opaque glyph handles.
    pub char_procedures: CharProcedures,
    /// Optional source-code-to-Unicode map.
    pub to_unicode: Option<Arc<ToUnicodeMap>>,
}

/// Type 3 glyph procedures and their opaque handles, decoded in the CharProcs scope.
#[derive(Clone)]
pub struct CharProcedures {
    /// Opaque procedure handles indexed by glyph name.
    pub handles: Arc<BTreeMap<GlyphName, GlyphId>>,
    /// Parsed PDF content streams indexed by their opaque handles.
    streams: Arc<HashMap<GlyphId, ContentStream>>,
}

impl CharProcedures {
    /// Resolves a glyph handle to its PDF content stream.
    #[must_use]
    pub fn get(&self, glyph: GlyphId) -> Option<&ContentStream> {
        self.streams.get(&glyph)
    }
}

impl FromPdfObject for CharProcedures {
    #[allow(clippy::arc_with_non_send_sync)]
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let mut handles = BTreeMap::new();
        let mut streams = HashMap::new();
        for name in context.keys() {
            // Every procedure shares the font's traversal and document-wide stream IDs.
            let stream: ContentStream = context.required(name)?;
            let handle = GlyphId::try_from(stream.id).map_err(|_| {
                FontError::InvalidDescendantFonts("Type 3 content stream ID does not fit u32")
            })?;
            handles.insert(GlyphName(Arc::from(name)), handle);
            streams.insert(handle, stream);
        }
        Ok(Self {
            handles: Arc::new(handles),
            streams: Arc::new(streams),
        })
    }
}

impl FromPdfObject for Type3FontSpec {
    /// Decodes a Type 3 dictionary without whole-font substitution; absent glyph widths
    /// default to zero.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let value = context.object();
        let mut context = context.dictionary()?;
        let font_matrix = context.required(b"FontMatrix")?;
        let bounds = context.required(b"FontBBox")?;
        let encoding = SimpleEncoding::new(context.optional(b"Encoding")?, false);
        let metrics: PdfMetrics = context.read(value)?;
        Ok(Self {
            base_font: context.lenient(b"BaseFont").unwrap_or_default(),
            metadata: FontMetadata::default(),
            font_matrix,
            bounds,
            encoding,
            metrics,
            char_procedures: context.required(b"CharProcs")?,
            to_unicode: context.optional(b"ToUnicode")?.map(Arc::new),
        })
    }
}
