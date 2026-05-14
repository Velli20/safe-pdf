use std::collections::HashMap;

use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_graphics::{rect::Rect, transform::Transform};
use pdf_object::{
    dictionary::Dictionary, object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

use crate::{
    encoding::{Encoding, FontEncoding},
    error::FontError,
    to_unicode_cmap::ToUnicodeCMap,
};

/// Represents a Type 3 font in a PDF document.
///
/// Type 3 fonts are defined by a program that describes the shape of each character.
/// Unlike other font types that rely on predefined glyph descriptions, Type 3 fonts
/// offer more flexibility in defining character shapes, allowing for complex
/// graphical elements within glyphs.  However, they are less efficient and do not
/// support advanced typographic features like hinting.
pub struct Type3Font {
    /// A matrix that maps user space coordinates to glyph space coordinates.
    /// It is used to transform glyph outlines during rendering.
    pub font_matrix: Transform,
    /// The bounding box of the font.
    pub bounds: Rect,
    /// A procedure defining any special actions to be taken before a character from this font is rendered.
    pub char_procs: HashMap<String, ContentStream>,
    /// The font's encoding, specifying the mapping from character codes to glyph names.
    pub encoding: Option<Encoding>,
    /// Parsed ToUnicode CMap for char-code → Unicode mapping.
    pub to_unicode: Option<ToUnicodeCMap>,
}

impl Type3Font {
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, FontError> {
        let [a, b, c, d, e, f] = dictionary
            .get_or_err("FontMatrix")?
            .try_array_of::<f32, 6>(objects)?;
        let font_matrix = Transform::from_row(a, b, c, d, e, f);

        let [left, top, right, bottom] = dictionary
            .get_or_err("FontBBox")?
            .try_array_of::<f32, 4>(objects)?;
        let bounds = Rect {
            left,
            top,
            right,
            bottom,
        };

        // Read optional `/Encoding` entry. This is either a name or a dictionary.
        let encoding = dictionary
            .get("Encoding")
            .map(|enc_obj| match objects.resolve_object(enc_obj)? {
                ObjectVariant::Dictionary(enc_dictionary) => {
                    Encoding::from_dictionary(enc_dictionary, objects)
                }
                other => Encoding::from_base_encoding(FontEncoding::from(other.try_str(objects)?)),
            })
            .transpose()?;

        let char_proc_dictionary = dictionary
            .get_or_err("CharProcs")?
            .try_dictionary(objects)?;

        let mut char_procs = HashMap::new();
        for (name, value) in char_proc_dictionary.dictionary.iter() {
            let content_stream = ContentStream::new(value, objects, id_allocator)?;
            char_procs.insert(name.to_owned(), content_stream);
        }

        // Parse optional ToUnicode CMap stream.
        let to_unicode = dictionary
            .get("ToUnicode")
            .and_then(|e| e.try_stream(objects).ok())
            .and_then(|s| s.data().ok())
            .map(|data| ToUnicodeCMap::from_bytes(&data));

        Ok(Type3Font {
            font_matrix,
            bounds,
            char_procs,
            encoding,
            to_unicode,
        })
    }
}
