use pdf_annotation_types::{Annotation, AnnotationError, annotation_id::AnnotationId};
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
    /// Next page-scoped annotation identifier.
    #[doc(hidden)]
    pub annotation_id_high_watermark: usize,
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
        let annotation_id_high_watermark = annotations.as_ref().map_or(0, Vec::len);

        Ok(Self {
            contents,
            annotations,
            media_box,
            resources,
            annotation_id_high_watermark,
        })
    }

    /// Returns an annotation by its stable page-scoped identifier.
    pub fn annotation(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations
            .as_deref()?
            .iter()
            .find(|annotation| annotation.id() == id)
    }

    /// Returns a mutable annotation by its stable page-scoped identifier.
    #[doc(hidden)]
    pub fn annotation_mut(&mut self, id: AnnotationId) -> Option<&mut Annotation> {
        self.annotations
            .as_deref_mut()?
            .iter_mut()
            .find(|annotation| annotation.id() == id)
    }

    /// Reserves a new identifier that will not be reused during this page's lifetime.
    #[doc(hidden)]
    pub fn reserve_annotation_id(&mut self) -> Option<AnnotationId> {
        let next = self.annotation_id_high_watermark.checked_add(1)?;
        let id = AnnotationId::from_page_value(self.annotation_id_high_watermark);
        self.annotation_id_high_watermark = next;
        Some(id)
    }

    /// Attaches an already materialized annotation to this page.
    #[doc(hidden)]
    pub fn push_annotation(&mut self, mut annotation: Annotation, id: AnnotationId) {
        annotation.set_id(id);
        self.annotations
            .get_or_insert_with(Vec::new)
            .push(annotation);
    }

    /// Removes an annotation by identifier.
    #[doc(hidden)]
    pub fn take_annotation(&mut self, id: AnnotationId) -> Option<Annotation> {
        let annotations = self.annotations.as_mut()?;
        let index = annotations
            .iter()
            .position(|annotation| annotation.id() == id)?;
        let annotation = annotations.remove(index);
        if annotations.is_empty() {
            self.annotations = None;
        }
        Some(annotation)
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
