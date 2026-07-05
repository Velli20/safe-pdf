use pdf_annotation_types::{Annotation, AnnotationError};
use pdf_content_stream::{ContentStream, ContentStreamIdAllocator};
use pdf_object::{dictionary::Dictionary, object_resolver::ObjectResolver};
use pdf_resources::{
    error::PdfPagesError, media_box::MediaBox, object_reader::ReadCycleTracker,
    resource_cache::ResourceCache, resources::Resources,
};

/// Represents a single page in a PDF document.
///
/// A page object is a dictionary that describes a single page of a document.
/// It contains references to the page's contents (the text, graphics, and images),
/// its resources, and other attributes according to PDF 1.7 specification.
#[derive(Default)]
pub struct PdfPage {
    /// The contents of the page, which can be a single stream object or
    /// an array of streams.
    pub contents: Option<ContentStream>,
    /// The raw annotation dictionaries attached to the page.
    pub annotations: Option<Vec<Annotation>>,
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
        let media_box = MediaBox::from_dictionary(dictionary, objects)?;
        let resources = Resources::read(dictionary, objects, cache, cycle_tracker, id_allocator)?;

        let annotations = Annotation::from_page_dictionary(
            dictionary,
            objects,
            cache,
            cycle_tracker,
            id_allocator,
        )
        .map_err(annotation_error_into_pages_error)?;

        Ok(Self {
            contents,
            annotations,
            media_box,
            resources,
        })
    }
}

fn annotation_error_into_pages_error(error: AnnotationError) -> PdfPagesError {
    match error {
        AnnotationError::Object(error) => error.into(),
        AnnotationError::Resources(error) => error,
        AnnotationError::InvalidEntry { entry, reason } => {
            PdfPagesError::InvalidAnnotationEntry { entry, reason }
        }
        AnnotationError::MissingEntry { entry } => PdfPagesError::MissingAnnotationEntry { entry },
    }
}
