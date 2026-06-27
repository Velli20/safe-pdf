use crate::{
    annotations::{Annotation, Annotations},
    error::PdfPagesError,
    media_box::MediaBox,
    object_reader::ReadCycleTracker,
    resource_cache::ResourceCache,
    resources::Resources,
};
use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};

/// Represents a single page in a PDF document.
///
/// A page object is a dictionary that describes a single page of a document.
/// It contains references to the page's contents (the text, graphics, and images),
/// its resources, and other attributes according to PDF 1.7 specification.
pub struct PdfPage {
    /// The contents of the page, which can be a single stream object or
    /// an array of streams.
    pub contents: Option<ContentStream>,
    /// The annotations attached to the page.
    pub annotations: Vec<Annotation>,
    /// `/MediaBox` attribute which defines the page boundaries.
    pub media_box: Option<MediaBox>,
    /// `/Resources` attribute which defines the resources used by the page.
    pub resources: Option<Resources>,
}

impl PdfPage {
    pub const KEY: &'static str = "Page";

    pub fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
        cache: &mut dyn ResourceCache,
        cycle_tracker: &mut ReadCycleTracker,
        id_allocator: &mut ContentStreamIdAllocator,
    ) -> Result<Self, PdfPagesError> {
        let contents = ContentStream::from_dictionary(dictionary, objects, id_allocator)?;
        let annotations = Annotations::from_dictionary(dictionary, objects)?;
        let media_box = MediaBox::from_dictionary(dictionary, objects)?;
        let resources = Resources::read(dictionary, objects, cache, cycle_tracker, id_allocator)?;

        Ok(Self {
            contents,
            annotations,
            media_box,
            resources,
        })
    }
}
