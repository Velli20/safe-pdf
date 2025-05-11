use crate::{error::PageError, media_box::MediaBox};
use pdf_object::{
    ObjectVariant, Value, dictionary::Dictionary, indirect_object::IndirectObject,
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
    parent: Option<IndirectObject>,
    /// The contents of the page, which can be a single stream object or
    /// an array of streams.
    contents: Option<ObjectVariant>,
    /// `/MediaBox` attribute which defines the page boundaries.
    media_box: MediaBox,
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
    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self, PageError> {
        // Get the optional `/Contents` entry from the page dictionary.
        let contents = if let Some(contents) = dictionary.get_object(Self::KEY_CONTENTS) {
            // The `/Contents` entry can be either:
            // 1. A direct stream object.
            // 2. An array of direct stream objects.
            // 3. An indirect reference to a stream object.
            // 4. An indirect reference to an array of stream objects.
            if let ObjectVariant::Reference(object_number) = contents {
                // The object is an indirect reference; resolve it from the `objects` collection.
                Some(
                    objects
                        .get(*object_number)
                        .ok_or(PageError::MissingContent)?,
                )
            } else {
                // The object is directly available (not an indirect reference that needs resolving here).
                Some(contents.clone())
            }
        } else {
            None
        };

        if let Some(ObjectVariant::IndirectObject(s)) = &contents {
            if let Some(Value::Array(array)) = &s.object {
                println!("Has arry {:?}", array);
                for obj in array.0.iter() {
                    if let Value::IndirectObject(s) = obj {
                        // println!("Has arry inner {:?}", s);
                        if let Some(ss) = objects.get(s.object_number()) {
                            println!("inner inner cont: {:?}", ss);
                        }
                    }
                }
            }
        }

        println!("contents {:?}", contents);

        // TODO: If the mediabox is missing, try to inherit one from the parent page.
        let media_box = if let Some(array) = dictionary.get_array(Self::KEY_MEDIABOX) {
            match array.0.as_slice() {
                // Pattern match for exactly 4 elements in the slice.
                [l, t, r, b] => {
                    // Safely extract and cast the values
                    let left = l.as_number::<u32>()?;
                    let top = t.as_number::<u32>()?;
                    let right = r.as_number::<u32>()?;
                    let bottom = b.as_number::<u32>()?;

                    MediaBox::new(left, top, right, bottom)
                }
                _ => {
                    return Err(PageError::InvalidMediaBox(
                        "MediaBox array must contain exactly 4 numbers",
                    ));
                }
            }
        } else {
            return Err(PageError::MissingMediaBox);
        };

        Ok(Self {
            parent: None,
            contents,
            media_box,
        })
    }
}
