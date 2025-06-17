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

    #[error("Missing required entry in FontDescriptor: /{0}")]
    MissingRequiredEntry(&'static str),

    #[error(
        "Invalid type for FontDescriptor entry /{entry_name}: expected {expected_type}, found {found_type}"
    )]
    InvalidEntryType {
        entry_name: &'static str,
        expected_type: &'static str,
        found_type: &'static str, // Assumes ObjectVariant::name() -> &'static str
    },

    #[error("Invalid FontBBox entry: expected an array of 4 numbers, found array of length {len}")]
    InvalidFontBBoxArrayLength { len: usize },

    #[error(
        "Failed to convert PDF value to number for FontDescriptor entry /{entry_description}: {source}"
    )]
    NumericConversionError {
        entry_description: &'static str,
        #[source]
        source: ObjectError,
    },

    #[error("FontName is required but was an empty string")]
    EmptyFontName,
}

/// Represents a font descriptor, a dictionary that provides detailed information
/// about a font, such as its metrics, style, and font file data.
#[derive(Debug)]
pub struct FontDescriptor {
    /// The maximum height above the baseline reached by glyphs in this font.
    /// This value is positive for ascenders.
    ascent: f32,
    /// The maximum depth below the baseline reached by glyphs in this font.
    /// This value is negative for descenders.
    descent: f32,
    /// The y-coordinate of the top of flat capital letters, measured from the baseline.
    cap_height: f32,
    /// A collection of flags specifying various characteristics of the font.
    flags: FontDescriptorFlags,
    /// A rectangle, expressed in the glyph coordinate system,
    /// that specifies the font bounding box. This is the smallest rectangle enclosing
    /// the shape that would result if all of the glyphs of the font were
    /// placed with their origins coincident and then filled.
    font_bounding_box: [f32; 4],
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
    italic_angle: f32,
    /// The width to use for glyphs not found in the font's encoding.
    pub missing_width: f32,
    /// The maximum width of a glyph in the font.
    max_width: Option<f32>,
    /// The thickness, measured horizontally, of the dominant vertical stems of glyphs in the font.
    stem_v: f32,
}

impl FromDictionary for FontDescriptor {
    const KEY: &'static str = "FontDescriptor";

    type ResultType = Self;
    type ErrorType = FontDescriptorError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let get_required_number_field = |key: &'static str| -> Result<f32, FontDescriptorError> {
            dictionary
                .get(key)
                .ok_or(FontDescriptorError::MissingRequiredEntry(key))?
                .as_number::<f32>()
                .map_err(|source| FontDescriptorError::NumericConversionError {
                    entry_description: key,
                    source,
                })
        };

        let get_optional_number_field =
            |key: &'static str| -> Result<Option<f32>, FontDescriptorError> {
                match dictionary.get(key) {
                    Some(val) => val.as_number::<f32>().map(Some).map_err(|source| {
                        FontDescriptorError::NumericConversionError {
                            entry_description: key,
                            source,
                        }
                    }),
                    None => Ok(None),
                }
            };

        let ascent = get_required_number_field("Ascent")?;
        let descent = get_required_number_field("Descent")?;
        let cap_height = get_required_number_field("CapHeight")?;

        let flags_val = dictionary
            .get("Flags")
            .ok_or(FontDescriptorError::MissingRequiredEntry("Flags"))?
            .as_number::<u32>()
            .map_err(|source| FontDescriptorError::NumericConversionError {
                entry_description: "Flags",
                source,
            })?;
        let flags = FontDescriptorFlags::from_bits_truncate(flags_val);

        let font_bounding_box = dictionary
            .get_array("FontBBox")
            .ok_or(FontDescriptorError::MissingRequiredEntry("FontBBox"))?;

        // Helper closure to convert a PDF Value to i32 for FontBBox entries
        // and map errors appropriately.
        let convert_bbox_entry =
            |value: &ObjectVariant, coord_name: &'static str| -> Result<f32, FontDescriptorError> {
                value.as_number::<f32>().map_err(|source| {
                    FontDescriptorError::NumericConversionError {
                        entry_description: coord_name, // e.g., "FontBBox[0] (llx)"
                        source,
                    }
                })
            };
        let font_bounding_box = match font_bounding_box.as_slice() {
            // Pattern match for exactly 4 elements in the slice.
            [l, t, r, b] => [
                convert_bbox_entry(l, "FontBBox llx")?,
                convert_bbox_entry(t, "FontBBox lly")?,
                convert_bbox_entry(r, "FontBBox urx")?,
                convert_bbox_entry(b, "FontBBox ury")?,
            ],
            arr => {
                return Err(FontDescriptorError::InvalidFontBBoxArrayLength { len: arr.len() });
            }
        };
        let font_family = dictionary.get_string("FontFamily").cloned();

        let resolve_font_file_stream = |key: &str| -> Option<ObjectVariant> {
            dictionary
                .get(key)
                .and_then(|obj_box| match obj_box.as_ref() {
                    ObjectVariant::Reference(id) => objects.get(*id),
                    ObjectVariant::Stream(s) => Some(ObjectVariant::Stream(s.clone())),
                    _ => None,
                })
        };

        let font_file = resolve_font_file_stream("FontFile2")
            .or_else(|| resolve_font_file_stream("FontFile3"))
            .or_else(|| resolve_font_file_stream("FontFile"));

        let font_name = dictionary
            .get_string("FontName")
            .ok_or(FontDescriptorError::MissingRequiredEntry("FontName"))?
            .clone();

        if font_name.is_empty() {
            return Err(FontDescriptorError::EmptyFontName);
        }

        let font_weight = dictionary.get_number("FontWeight");
        let italic_angle = get_required_number_field("ItalicAngle")?;
        let missing_width = get_optional_number_field("MissingWidth")?.unwrap_or(0.0);
        let max_width = get_optional_number_field("MaxWidth")?;
        let stem_v = get_required_number_field("StemV")?;

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
