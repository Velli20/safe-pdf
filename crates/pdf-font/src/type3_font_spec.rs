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
use pdf_object_reader::{
    DictionaryContext, FromPdfObject, ObjectAccess, ObjectContext, ObjectReadError, ReadResult,
};
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
        // Copy keys only so mutable child reads do not require cloning stream objects.
        let names: Vec<_> = context.dictionary().dictionary.keys().cloned().collect();
        let mut handles = BTreeMap::new();
        let mut streams = HashMap::new();
        for name in names {
            // Every procedure shares the font's traversal and document-wide stream IDs.
            let stream: ContentStream = context.required(&name)?;
            let handle = GlyphId(u32::try_from(stream.id).map_err(|_| {
                FontError::InvalidDescendantFonts("Type 3 content stream ID does not fit u32")
            })?);
            handles.insert(GlyphName(Arc::from(name)), handle);
            streams.insert(handle, stream);
        }
        Ok(Self { handles, streams })
    }
}

impl FromPdfObject for Type3FontSpec {
    /// Decodes the concrete font dictionary without whole-font substitution.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Self::try_from(&mut context.dictionary()?)
    }
}

impl<A: ObjectAccess + ?Sized> TryFrom<&mut DictionaryContext<'_, A>> for Type3FontSpec {
    type Error = ObjectReadError;

    /// Reads a Type 3 dictionary, whose absent glyph widths default to zero.
    #[allow(clippy::arc_with_non_send_sync)]
    fn try_from(context: &mut DictionaryContext<'_, A>) -> ReadResult<Self> {
        let [a, b, c, d, e, f] = context.required::<[f32; 6]>(b"FontMatrix")?;
        let [left, top, right, bottom] = context.required::<[f32; 4]>(b"FontBBox")?;
        let encoding = simple_encoding(context, false)?;
        let metrics = PdfMetrics::try_from(&mut *context)?;
        let procedures: CharProcedures = context.required(b"CharProcs")?;
        Ok(Self {
            base_font: base_font(context).unwrap_or_default(),
            metadata: FontMetadata::default(),
            font_matrix: Transform::from_row(a, b, c, d, e, f),
            bounds: Rect {
                left,
                top,
                right,
                bottom,
            },
            encoding,
            metrics,
            char_procedures: Arc::new(procedures.handles),
            type3_procedures: Arc::new(procedures.streams),
            to_unicode: to_unicode(context)?,
        })
    }
}
