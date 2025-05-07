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
    // dictionary: Dictionary,
    /// Reference to the parent page tree node.
    parent: Option<IndirectObjectOrReference>,

    contents: Value,
}

impl PdfPage {
    /// Key for the Type entry in the page dictionary.
    /// Must always have the value "Page" for page objects.
    const KEY_TYPE: &'static str = "Type";

    /// Key for the required Parent entry, which must be an indirect reference
    /// to the page tree node that contains this page object.
    const KEY_PARENT: &'static str = "Parent";

    /// Key for the MediaBox entry, which defines the boundaries of the physical
    /// medium on which the page is intended to be displayed/printed.
    const KEY_MEDIABOX: &'static str = "MediaBox";

    /// Key for the Contents entry, which can be a content stream or array of
    /// content streams containing the PDF instructions for rendering the page.
    const KEY_CONTENTS: &'static str = "Contents";

    /// Key for the Resources entry, which is a dictionary specifying named
    /// resources (such as fonts, images) required by the content streams.
    const KEY_RESOURCES: &'static str = "Resources";

    /// Key for the CropBox entry, which defines the visible region of the page.
    /// Value is an array of four numbers [llx lly urx ury] specifying a rectangle.
    /// If not specified, defaults to the MediaBox value.
    const KEY_CROPBOX: &'static str = "CropBox";

    /// Key for the BleedBox entry, which defines the region to which the page's
    /// contents are clipped when output in a production environment.
    /// Used in professional printing. Defaults to CropBox if not specified.
    const KEY_BLEEDBOX: &'static str = "BleedBox";

    /// Key for the TrimBox entry, which defines the intended dimensions of the
    /// finished page after trimming.
    /// Used in professional printing. Defaults to CropBox if not specified.
    const KEY_TRIMBOX: &'static str = "TrimBox";

    /// Key for the ArtBox entry, which defines the extent of the page's meaningful
    /// content as intended by the page's creator.
    /// Used in professional printing. Defaults to CropBox if not specified.
    const KEY_ARTBOX: &'static str = "ArtBox";

    /// Key for the Rotate entry, which specifies the number of degrees by which the page
    /// should be rotated clockwise when displayed or printed.
    /// The value must be a multiple of 90. Default value: 0.
    const KEY_ROTATE: &'static str = "Rotate";

    /// Key for the Annots entry, which is an array of annotation dictionaries.
    /// Each dictionary describes a single annotation associated with the page.
    const KEY_ANNOTS: &'static str = "Annots";

    /// Key for the Thumb entry, which is a stream object containing the page's
    /// thumbnail image in JPEG, JPEG2000, or PNG format.
    const KEY_THUMB: &'static str = "Thumb";

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

        let contents;
        if contents_ref.object.is_none() {
            contents = objects.get(contents_ref.object_number).unwrap().clone();
        } else {
            contents = contents_ref.object.as_ref().unwrap().clone();
        }
        println!("contents {:?}", contents);
        Self {
            parent: None,
            contents,
        }
    }
}
