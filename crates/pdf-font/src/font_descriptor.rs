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
    #[error("Object error: {0}")]
    ObjectError(#[from] ObjectError),
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
        // Required numeric fields
        let ascent = dictionary
            .get_or_err("Ascent")?
            .as_number_entry::<f32>("Ascent")?;
        let descent = dictionary
            .get_or_err("Descent")?
            .as_number_entry::<f32>("Descent")?;
        let cap_height = dictionary
            .get_or_err("CapHeight")?
            .as_number_entry::<f32>("CapHeight")?;

        let flags_val = dictionary
            .get_or_err("Flags")?
            .as_number_entry::<u32>("Flags")?;

        let flags = FontDescriptorFlags::from_bits_truncate(flags_val);

        let font_bounding_box = dictionary.get_or_err("FontBBox")?.as_array_of::<f32, 4>()?;

        let font_family = dictionary
            .get("FontFamily")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());


        let resolve_font_file_stream = |key: &str| -> Option<ObjectVariant> {
            dictionary.get(key).and_then(|obj| match obj {
                ObjectVariant::Reference(id) => objects.get(*id).cloned(),
                ObjectVariant::Stream(s) => Some(ObjectVariant::Stream(std::rc::Rc::clone(s))),
                _ => None,
            })
        };

        let font_file = resolve_font_file_stream("FontFile2")
            .or_else(|| resolve_font_file_stream("FontFile3"))
            .or_else(|| resolve_font_file_stream("FontFile"));

        let font_name = dictionary.get_or_err("FontName")?.try_str()?.to_string();
        if font_name.is_empty() {
            return Err(FontDescriptorError::EmptyFontName);
        }

        let italic_angle = dictionary
            .get_or_err("ItalicAngle")?
            .as_number_entry::<f32>("ItalicAngle")?;

        // Optional numeric fields
        let missing_width = dictionary
            .get("MissingWidth")
            .map(ObjectVariant::as_number::<f32>)
            .transpose()?
            .unwrap_or(0.0);

        let max_width = dictionary
            .get("MaxWidth")
            .map(ObjectVariant::as_number::<f32>)
            .transpose()?;

        let stem_v = dictionary
            .get_or_err("StemV")?
            .as_number_entry::<f32>("StemV")?;

        Ok(Self {
            ascent,
            descent,
            cap_height,
            flags,
            font_bounding_box,
            font_family,
            font_file,
            font_name,
            italic_angle,
            missing_width,
            max_width,
            stem_v,
        })
    }
}
