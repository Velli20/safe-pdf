//! The note icon a Text annotation without a normal appearance is drawn with, as PDFium
//! generates it: a yellow speech bubble with three lines on a square of fixed size.
use crate::pdf_data::{AppearanceDictionary, AppearanceStreams};
use pdf_graphics::rect::Rect;
use pdf_object_reader::{
    Dictionary, StreamObject, object_variant::ObjectVariant, pdf_array::PdfArray,
    pdf_object::PdfObject, pdf_string::PdfString, string_kind::StringKind,
};

/// Width and height of the generated note icon, in page units.
const NOTE_ICON_SIZE: f32 = 20.0;

/// The icon's `/BBox`: the `NOTE_ICON_SIZE` square at the origin.
const NOTE_ICON_BBOX: [i64; 4] = [0, 0, 20, 20];

/// The icon in its `NOTE_ICON_SIZE` square box: a yellow fill and black 1-unit stroke, the
/// bubble outline with its tip at the bottom, and three lines across it.
const NOTE_ICON_CONTENT: &[u8] = b"1 1 0 rg 0 0 0 RG 1 w\n\
0.5 4.5 m 0.5 19.5 l 19.5 19.5 l 19.5 4.5 l 8.5 4.5 l 6.5 0.5 l 4.5 4.5 l 0.5 4.5 l\n\
2.5 15.75 m 17.5 15.75 l\n\
2.5 12 m 17.5 12 l\n\
2.5 8.25 m 17.5 8.25 l\n\
B*\n";

impl AppearanceDictionary {
    /// The generated note icon as a normal appearance, with `down` kept from the source.
    pub(crate) fn note_icon(down: Option<AppearanceStreams>) -> Self {
        let name = |bytes: &[u8]| PdfString::from(bytes, StringKind::Name);
        let bbox = PdfArray::new(NOTE_ICON_BBOX.map(ObjectVariant::Integer));
        let dictionary = Dictionary::from_entries([
            (b"Type".as_slice(), name(b"XObject")),
            (b"Subtype".as_slice(), name(b"Form")),
            (b"BBox".as_slice(), ObjectVariant::Array(bbox)),
        ]);
        let stream = StreamObject::new(0, 0, dictionary, NOTE_ICON_CONTENT);
        Self {
            normal: Some(AppearanceStreams::Stream(PdfObject::new(
                ObjectVariant::Stream(stream),
            ))),
            down,
        }
    }
}

/// Places the generated note icon at this source rectangle's lower-left corner, whatever
/// the rectangle's size.
pub(crate) trait NoteIconPlacement {
    /// The icon's rectangle, in the source rectangle's edge convention.
    fn note_icon_rect(&self) -> Rect;
}

impl NoteIconPlacement for Rect {
    fn note_icon_rect(&self) -> Rect {
        let rect = self.normalized();
        Rect {
            left: rect.left,
            top: rect.top,
            right: rect.left + NOTE_ICON_SIZE,
            bottom: rect.top + NOTE_ICON_SIZE,
        }
    }
}
