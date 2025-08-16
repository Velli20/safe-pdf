use crate::{
    content_stream::{ContentStream, ContentStreamReadError},
    media_box::{MediaBox, MediaBoxError},
    resources::{Resources, ResourcesError},
};
use pdf_object::{
    dictionary::Dictionary, indirect_object::IndirectObject, object_collection::ObjectCollection,
    traits::FromDictionary,
};
use thiserror::Error;

/// Errors that can occur during parsing of a PDF Page object.
#[derive(Error, Debug)]
pub enum PdfPageError {
    #[error("Failed to parse content stream for page: {0}")]
    ContentStreamParse(#[from] ContentStreamReadError),

    #[error("Failed to parse media box for page: {0}")]
    MediaBoxParse(#[from] MediaBoxError),
    #[error("Failed to parse resources for page: {0}")]
    ResourcesParse(#[from] ResourcesError),
}

/// Represents a single page in a PDF document.
///
/// A page object is a dictionary that describes a single page of a document.
/// It contains references to the page's contents (the text, graphics, and images),
/// its resources, and other attributes according to PDF 1.7 specification.
pub struct PdfPage {
    /// The page object dictionary containing all page-specific information.
    /// Reference to the parent page tree node.
    _parent: Option<IndirectObject>,
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
    type ErrorType = PdfPageError;

    fn from_dictionary(
        dictionary: &Dictionary,
        objects: &ObjectCollection,
    ) -> Result<Self::ResultType, Self::ErrorType> {
        let contents = ContentStream::from_dictionary(dictionary, objects)?;
        let media_box = MediaBox::from_dictionary(dictionary, objects)?;
        let resources = Resources::from_dictionary(dictionary, objects)?;

        Ok(Self {
            _parent: None,
            contents,
            media_box,
            resources,
        })
    }
}
