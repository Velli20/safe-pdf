use crate::{canvas::Canvas, error::PdfCanvasError, text_renderer::TextRenderer};
use num_traits::FromPrimitive;
use pdf_content_stream::pdf_operator_backend::PdfOperatorBackend;
use pdf_font::{
    font::{Font, FontEncoding},
    glyph_widths_map::GlyphWidthsMap,
    simple_font_glyph_map::SimpleFontGlyphWidthsMap,
    type0_font::CidFontSubType,
};
use pdf_graphics::{PathFillType, pdf_path::PdfPath, transform::Transform};
use thiserror::Error;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// Defines errors that can occur during TrueType font rendering.
#[derive(Debug, Error)]
pub enum TrueTypeFontRendererError {
    #[error("The font file object is not a stream, but a {found_type}")]
    FontFileNotStream { found_type: &'static str },
    #[error("Failed to parse the TrueType font file: {0:?}")]
    TtfParseError(ttf_parser::FaceParsingError),
    #[error("No character map found for font '{0}'")]
    NoCharacterMapForFont(String),
    #[error("Incomplete 2-byte character at the end of the string")]
    IncompleteTwoByteCharacter,
    #[error("Missing font file stream for TrueType font")]
    MissingFontFile,
    #[error("Not implemented")]
    NotImplemented,
}

/// A text renderer for TrueType-based fonts.
pub(crate) struct TrueTypeFontRenderer<'a, T: PdfOperatorBackend + Canvas> {
    /// The canvas backend where glyphs are drawn.
    canvas: &'a mut T,
    /// The underlying TrueType font file stream, if available.
    object_stream: Option<&'a pdf_object::stream::StreamObject>,
    /// Optional character map for mapping character codes to Unicode values.
    cmap: Option<&'a pdf_font::character_map::CharacterMap>,
    /// Optional encoding for simple fonts (Type1, TrueType).
    encoding: Option<&'a FontEncoding>,
    /// Optional glyph widths map for CID-keyed fonts.
    widths: Option<&'a GlyphWidthsMap>,
    /// Optional width map for simple fonts (Type1, TrueType).
    w: Option<&'a SimpleFontGlyphWidthsMap>,
    /// The default glyph width for the font, used if specific widths are not provided.
    default_width: f32,
    /// The current text matrix (Tm), which positions the text.
    text_matrix: Transform,
    /// The Current Transformation Matrix (CTM) at the time of rendering.
    current_transform: Transform,
    /// The font size in user space units.
    font_size: f32,
    /// The text rise (Ts), a vertical offset from the baseline.
    rise: f32,
    /// The spacing to add between words, applied to space characters.
    word_spacing: f32,
    /// The spacing to add between individual characters.
    char_spacing: f32,
    /// The horizontal scaling factor for glyphs, as a percentage [0-100].
    horizontal_scaling: f32,
}

impl<'a, T: PdfOperatorBackend + Canvas> TrueTypeFontRenderer<'a, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        canvas: &'a mut T,
        font: &'a Font,
        font_size: f32,
        horizontal_scaling: f32,
        text_matrix: Transform,
        current_transform: Transform,
        rise: f32,
        word_spacing: f32,
        char_spacing: f32,
    ) -> Result<Self, PdfCanvasError> {
        match font {
            Font::TrueType(tt_font) => {
                let object_stream = tt_font.font_file.as_ref();
                let cmap = tt_font.cmap.as_ref();
                let encoding = tt_font.encoding.as_ref();
                let w = Some(&tt_font.widths);

                Ok(Self {
                    canvas,
                    object_stream,
                    cmap,
                    encoding,
                    widths: None,
                    w,
                    default_width: 0.0,
                    text_matrix,
                    current_transform,
                    font_size,
                    rise,
                    word_spacing,
                    char_spacing,
                    horizontal_scaling,
                })
            }
            Font::Type0(type0_font) => {
                // Ensure the CIDFont is a TrueType-based font (CIDFontType2).
                let cid_font = match &type0_font.subtype {
                    CidFontSubType::Type2 => type0_font,
                    _ => {
                        return Err(TrueTypeFontRendererError::NotImplemented.into());
                    }
                };

                let object_stream = cid_font.font_file.as_ref();
                let cmap = cid_font.cmap.as_ref();
                let encoding = cid_font.encoding.as_ref();
                let widths = cid_font.widths.as_ref();
                let default_width = cid_font.default_width;

                Ok(Self {
                    canvas,
                    object_stream,
                    cmap,
                    encoding,
                    widths,
                    w: None,
                    default_width,
                    text_matrix,
                    current_transform,
                    font_size,
                    rise,
                    word_spacing,
                    char_spacing,
                    horizontal_scaling,
                })
            }
            _ => Err(TrueTypeFontRendererError::NotImplemented.into()),
        }
    }
}

impl<T: PdfOperatorBackend + Canvas> TextRenderer for TrueTypeFontRenderer<'_, T> {
    fn render_text(&mut self, text: &[u8]) -> Result<(), crate::error::PdfCanvasError> {
        let Some(object_stream) = self.object_stream else {
            // TODO: Use BaseName from FontDescriptor to load a system font?
            return Ok(());
        };

        let face = Face::parse(object_stream.data.as_slice(), 0)
            .map_err(TrueTypeFontRendererError::TtfParseError)?;

        // Extract font and text state parameters.
        let units_per_em = face.units_per_em();
        let char_spacing = self.char_spacing;
        let word_spacing = self.word_spacing;
        let text_rise = self.rise;

        // Compute the inverse of units per em for scaling.
        let upe_inv = if units_per_em != 0 {
            1.0 / f32::from_u16(units_per_em)
                .ok_or(PdfCanvasError::NumericConversionError("units_per_em"))?
        } else {
            0.0
        };

        // Th_factor: Horizontal scaling factor (Th / 100).
        let th_factor = self.horizontal_scaling / 100.0;

        // Build the text rendering transform.
        let m_params = Transform::from_row(
            self.font_size * upe_inv * th_factor, // sx
            0.0,                                  // ky (skew)
            0.0,                                  // kx (skew)
            self.font_size * upe_inv,             // sy
            0.0,                                  // tx
            text_rise,                            // ty
        );

        // Determine if the font uses a 2-byte encoding (e.g., /Identity-H for CID-keyed fonts).
        let is_two_byte_encoding = self.encoding.is_some();
        let mut iter = text.iter().copied();

        // Iterate over each character in the input text.
        while let Some(first_byte) = iter.next() {
            let char_code = if is_two_byte_encoding {
                // For 2-byte encodings, read the second byte.
                if let Some(second_byte) = iter.next() {
                    // Combine the two bytes into a single u16 character code.
                    // PDF uses big-endian for 2-byte character codes.
                    u16::from_be_bytes([first_byte, second_byte])
                } else {
                    // Incomplete 2-byte character at the end of the string. Return an error.
                    return Err(TrueTypeFontRendererError::IncompleteTwoByteCharacter.into());
                }
            } else {
                // For 1-byte encodings, the character code is simply the byte itself.
                u16::from_u8(first_byte)
                    .ok_or(PdfCanvasError::NumericConversionError("first_byte"))?
            };

            let mut glyph_id = GlyphId(char_code);

            // Compose the final transformation matrix for this glyph:
            // m_params -> text matrix -> current transformation matrix
            let mut glyph_matrix_for_char = m_params;
            glyph_matrix_for_char.concat(&self.text_matrix);
            glyph_matrix_for_char.concat(&self.current_transform);

            // Build the glyph outline using the composed transform.
            let mut builder = PdfGlyphOutline::new(glyph_matrix_for_char);

            // Map character code to glyph ID using the font's cmap if available.
            if let Some(cmap) = self.cmap
                && let Some(a) = cmap.get_mapping(u32::from(char_code))
                && let Some(x) = face.glyph_index(a)
            {
                glyph_id = x;
            }

            face.outline_glyph(glyph_id, &mut builder);

            // Fill it on the canvas
            self.canvas
                .fill_path(&builder.path, PathFillType::Winding)?;

            // Determine the glyph's advance width in font units.
            // Determine width source: CID descendant map or simple font widths (in glyph space 1000 units)
            let w0_glyph_units = if let Some(widths) = self.widths {
                widths.get_width(char_code).unwrap_or(self.default_width)
            } else if let Some(widths) = self.w {
                widths.get_width(char_code).unwrap_or(self.default_width)
            } else {
                self.default_width
            };

            // Convert width from font units to ems.
            let w0_ems = w0_glyph_units / 1000.0;

            // Scale the glyph width by the font size.
            let glyph_width_tfs_scaled = w0_ems * self.font_size;

            // Apply word spacing only to space characters.
            let word_spacing_for_char = if char_code == 32 { word_spacing } else { 0.0 };

            // Compute the horizontal advance for this glyph.
            let advance_x =
                (glyph_width_tfs_scaled + char_spacing + word_spacing_for_char) * th_factor;

            // Advance the text matrix for the next glyph.
            self.text_matrix.translate(advance_x, 0.0);
        }

        Ok(())
    }
}

/// An implementation of `ttf_parser::OutlineBuilder` that converts glyph outlines
/// into a `PdfPath`.
#[derive(Default)]
pub struct PdfGlyphOutline {
    /// The `PdfPath` being constructed from the glyph outline commands.
    path: PdfPath,
    /// The transformation matrix to apply to each point of the glyph outline.
    transform: Transform,
}

impl PdfGlyphOutline {
    pub fn new(transform: Transform) -> Self {
        Self {
            path: PdfPath::default(),
            transform,
        }
    }
}

impl OutlineBuilder for PdfGlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.transform_point(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.transform_point(x1, y1);
        let (x2, y2) = self.transform.transform_point(x2, y2);
        let (x, y) = self.transform.transform_point(x, y);
        self.path.curve_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}
