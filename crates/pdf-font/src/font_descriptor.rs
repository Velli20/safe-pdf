use bitflags::bitflags;
use pdf_object::{
    ObjectVariant, dictionary::Dictionary, error::ObjectError, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

bitflags! {
    /// Defines various characteristics of a font, such as whether it is serif, italic, etc.
    /// These flags correspond to the values specified in Table 9.8 "Font Descriptor Flags"
    /// in the PDF 1.7 specification (ISO 32000-1:2008).
    /// Each flag represents a specific attribute of the font's appearance or behavior.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct FontDescriptorFlags: u32 {
        /// Set if the font is monospaced.
        const FIXED_PITCH  = 0x0001;
        ///	Set if the font uses serifs.
        const SERIF        = 0x0002;
        /// Set if the font contains characters outside the Adobe standard Latin set.
        const SYMBOLIC     = 0x0004;
        /// Set if the font is script-style (glyphs resemble cursive handwriting).
        const SCRIPT       = 0x0008;
        /// Set if the font uses the Adobe standard Latin set.
        const NONSYMBOLIC  = 0x0010;
        /// Set if the font is italic.
        const ITALIC       = 0x0020;
        /// Set if the font is all caps.
        const ALL_CAP      = 0x0040;
        /// Set if the font is small caps.
        const SMALL_CAP    = 0x0080;
        /// Set if the font is forcibly bold.
        const FORCE_BOLD   = 0x0100;
    }
}

/// Defines errors that can occur while reading or processing font-related PDF objects.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum FontDescriptorError {
    #[error("Invalid data for /FontBBox entry")]
    InvalidFontBoundingBox,
    #[error("Missing /FontBBox entry")]
    MissingFontBoundingBox,
    #[error("Missing /FontName entry")]
    MissingFontName,
    /// Error converting a PDF value to a number.
    #[error("Failed to convert PDF value to number for '{entry_description}': {err}")]
    FontBoundingBoxNumericConversionError {
        entry_description: &'static str,
        #[source]
        err: ObjectError,
    },
}

/// Represents a font descriptor, a dictionary that provides detailed information
/// about a font, such as its metrics, style, and font file data.
#[derive(Debug)]
pub struct FontDescriptor {
    /// The maximum height above the baseline reached by glyphs in this font.
    /// This value is positive for ascenders.
    ascent: i64,
    /// The maximum depth below the baseline reached by glyphs in this font.
    /// This value is negative for descenders.
    descent: i64,
    /// The y-coordinate of the top of flat capital letters, measured from the baseline.
    cap_height: i64,
    /// A collection of flags specifying various characteristics of the font.
    flags: FontDescriptorFlags,
    /// A rectangle, expressed in the glyph coordinate system,
    /// that specifies the font bounding box. This is the smallest rectangle enclosing
    /// the shape that would result if all of the glyphs of the font were
    /// placed with their origins coincident and then filled.
    font_bounding_box: [i32; 4],
    /// A string specifying the preferred font family name.
    font_family: Option<String>,
    /// A stream containing the font program.
    /// This can be FontFile, FontFile2, or FontFile3 depending on the font type.
    pub font_file: Option<ObjectVariant>,
    /// The PostScript name of the font.
    font_name: String,
    /// The weight (thickness) of the font's strokes.
    font_weight: Option<i64>,
    /// The angle, in degrees counterclockwise from the vertical, of the dominant vertical strokes of the font.
    italic_angle: i64,
    /// The width to use for glyphs not found in the font's encoding.
    pub missing_width: i64,
    /// The maximum width of a glyph in the font.
    max_width: Option<i64>,
    /// The thickness, measured horizontally, of the dominant vertical stems of glyphs in the font.
    stem_v: i64,
}

impl FromDictionary for FontDescriptor {
    const KEY: &'static str = "FontDescriptor";

    type ResultType = Self;
    type ErrorType = FontDescriptorError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let ascent = dictionary.get_number("Ascent").unwrap_or(0);
        let descent = dictionary.get_number("Descent").unwrap_or(0);
        let cap_height = dictionary.get_number("CapHeight").unwrap_or(0);
        let flags = dictionary.get_number("Flags").unwrap_or(0);
        let flags = FontDescriptorFlags::from_bits_truncate(flags as u32);
        let font_bounding_box = dictionary
            .get_array("FontBBox")
            .ok_or(FontDescriptorError::MissingFontBoundingBox)?;

        // Helper closure to convert a PDF Value to i32 for FontBBox entries
        // and map errors appropriately.
        let convert_bbox_entry = |value: &pdf_object::ObjectVariant, description: &'static str| {
            value.as_number::<i32>().map_err(|err| {
                FontDescriptorError::FontBoundingBoxNumericConversionError {
                    entry_description: description,
                    err,
                }
            })
        };

        let font_bounding_box = match font_bounding_box.as_slice() {
            // Pattern match for exactly 4 elements in the slice.
            [l, t, r, b] => {
                [
                    convert_bbox_entry(l, "left")?,   // Corresponds to llx
                    convert_bbox_entry(t, "top")?,    // Corresponds to lly
                    convert_bbox_entry(r, "right")?,  // Corresponds to urx
                    convert_bbox_entry(b, "bottom")?, // Corresponds to ury
                ]
            }
            _ => {
                return Err(FontDescriptorError::InvalidFontBoundingBox);
            }
        };
        let font_family = dictionary.get_string("FontFamily").cloned();

        let font_file = if let Some(s) = dictionary.get("FontFile2") {
            objects.get2(s).cloned()
        } else {
            None
        };

        let font_name = dictionary
            .get_string("FontName")
            .unwrap_or(&String::new())
            .clone();
        let font_weight = dictionary.get_number("FontWeight");
        let italic_angle = dictionary.get_number("ItalicAngle").unwrap_or(0);
        let missing_width = dictionary.get_number("MissingWidth").unwrap_or(0);
        let max_width = dictionary.get_number("MaxWidth");
        let stem_v = dictionary.get_number("StemV").unwrap_or(0);

        Ok(Self {
            ascent,
            descent,
            cap_height,
            flags,
            font_bounding_box,
            font_family,
            font_file,
            font_name,
            font_weight,
            italic_angle,
            missing_width,
            max_width,
            stem_v,
        })
    }
}
