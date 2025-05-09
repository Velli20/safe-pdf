use std::rc::Rc;

use pdf_object::{
    Value, dictionary::Dictionary, indirect_object::IndirectObjectOrReference,
    object_collection::ObjectCollection,
};

/// Represents a single page in a PDF document.
///
/// A page object is a dictionary that describes a single page of a document.
/// It contains references to the page's contents (the text, graphics, and images),
/// its resources, and other attributes according to PDF 1.7 specification.
pub struct PdfPage {
    /// The page object dictionary containing all page-specific information.
    /// Reference to the parent page tree node.
    parent: Option<IndirectObjectOrReference>,
    /// The contents of the page, which can be a single stream object or
    /// an array of streams.
    contents: Value,
}

impl PdfPage {
    /// Key for the Type entry in the page dictionary.
    /// For a page object, the value of this entry must be `Page`.
    const KEY_TYPE: &'static str = "Type";

    /// (Required) Key for the Parent entry, an indirect reference to the page tree
    /// node that serves as the parent of this page object.
    const KEY_PARENT: &'static str = "Parent";

    /// (Required; inheritable) Key for the MediaBox entry, an array of four numbers
    /// `[llx lly urx ury]` defining the boundaries of the physical medium on which
    /// the page is intended to be displayed or printed.
    const KEY_MEDIABOX: &'static str = "MediaBox";

    /// (Optional) Key for the Contents entry, which is a content stream or an array
    /// of content streams. These streams contain the PDF marking operators that
    /// define the page's appearance.
    const KEY_CONTENTS: &'static str = "Contents";

    /// (Required; inheritable) Key for the Resources entry, a dictionary specifying
    /// the resources (such as fonts, images, and external objects) required by
    /// the page's content streams.
    const KEY_RESOURCES: &'static str = "Resources";

    /// (Optional; inheritable) Key for the CropBox entry, an array of four numbers
    /// `[llx lly urx ury]` defining the rectangular region to which the contents
    /// of the page are to be clipped (cropped) when displayed or printed.
    /// Defaults to the value of MediaBox if not specified.
    const KEY_CROPBOX: &'static str = "CropBox";

    /// (Optional) Key for the BleedBox entry, an array of four numbers `[llx lly urx ury]`
    /// defining the region to which the contents of the page should be clipped when
    /// output in a production environment (e.g., for professional printing).
    /// Defaults to the value of CropBox if not specified.
    const KEY_BLEEDBOX: &'static str = "BleedBox";

    /// (Optional) Key for the TrimBox entry, an array of four numbers `[llx lly urx ury]`
    /// defining the intended dimensions of the finished page after trimming.
    /// Defaults to the value of CropBox if not specified.
    const KEY_TRIMBOX: &'static str = "TrimBox";

    /// (Optional) Key for the ArtBox entry, an array of four numbers `[llx lly urx ury]`
    /// defining the extent of the page's meaningful content (e.g., for imposition or
    /// trapping). Defaults to the value of CropBox if not specified.
    const KEY_ARTBOX: &'static str = "ArtBox";

    /// (Optional; inheritable) Key for the Rotate entry, an integer specifying the
    /// number of degrees by which the page should be rotated clockwise when displayed
    /// or printed. The value must be a multiple of 90. Default: 0.
    const KEY_ROTATE: &'static str = "Rotate";

    /// (Optional) Key for the Annots entry, an array of indirect references to
    /// annotation dictionaries that are associated with this page.
    const KEY_ANNOTS: &'static str = "Annots";

    /// (Optional) Key for the Thumb entry, an indirect reference to a stream object
    /// representing a thumbnail image for the page.
    const KEY_THUMB: &'static str = "Thumb";

    /// Creates a `PdfPage` instance from a page object's dictionary.
    ///
    /// This function parses the provided `dictionary` (which should be a
    /// PDF page object dictionary) and extracts necessary information, such as
    /// the page's contents. Indirect references within the page dictionary,
    /// like the `Contents` stream or array, are resolved using the `objects`
    /// collection.
    pub fn from_dictionary(dictionary: &Dictionary, objects: &ObjectCollection) -> Self {
        // From the PDF specification:
        // "A content stream describing the contents of the page.
        // This value shall be either a stream or an array of streams,
        // either direct or indirect."
        let contents_ref = dictionary.get_object(Self::KEY_CONTENTS);
        if contents_ref.is_none() {
            panic!()
        }
        let contents_ref = contents_ref.unwrap();

        // The /Contents entry in a page dictionary is optional, and if present, it must be:
        // A stream object (a single content stream)
        // or
        // An array of stream objects (multiple content streams to be processed in order).

        let id = contents_ref.object_number;
        let mut contents;
        if contents_ref.object.is_none() {
            contents = objects.get(contents_ref.object_number).unwrap().clone();
        } else {
            contents = contents_ref.object.as_ref().unwrap().clone();
        }
        println!("contents {:?}", contents);

        if let Value::IndirectObject(ss) = &contents {
            if ss.object.is_none() {
                contents = objects.get(ss.object_number).unwrap().clone();
            } else {
                contents = ss.object.as_ref().unwrap().clone();
            }
        }

        if let Value::Array(array) = &mut contents {
            for obj in array.0.iter_mut() {
                if let Value::IndirectObject(ss) = obj {
                    let ii = objects.get(ss.object_number);
                    if ii.is_none() {
                        panic!()
                    }
                    let ii = ii.unwrap();
                    *obj = ii;
                }
            }
        }
        println!("id {} contents {:?}", id, contents);
        Self {
            parent: None,
            contents,
        }
    }
}
