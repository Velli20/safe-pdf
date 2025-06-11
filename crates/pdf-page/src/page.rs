use crate::{
    content_stream::ContentStream,
    error::PageError,
    media_box::{self, MediaBox},
    resources::Resources,
};
use pdf_object::{
    dictionary::Dictionary, indirect_object::IndirectObject, object_collection::ObjectCollection,
    traits::FromDictionary,
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
    pub contents: Option<ContentStream>,
    /// `/MediaBox` attribute which defines the page boundaries.
    pub media_box: Option<MediaBox>,
    pub resources: Option<Resources>,
}

impl FromDictionary for PdfPage {
    const KEY: &'static str = "Page";

    type ResultType = Self;
    type ErrorType = PageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, PageError> {
        // Get the optional `/Contents` entry from the page dictionary.
        let contents = ContentStream::from_dictionary(dictionary, objects).ok();

        let media_box = {
            let media_box = MediaBox::from_dictionary(dictionary, objects);
            if let Err(PageError::MissingMediaBox) = media_box {
                None
            } else if let Err(e) = media_box {
                return Err(e);
            } else {
                let media_box = media_box.unwrap();
                Some(media_box)
            }
        };

        let resources = Resources::from_dictionary(dictionary, objects).ok();

        Ok(Self {
            parent: None,
            contents,
            media_box,
            resources,
        })
    }
}
