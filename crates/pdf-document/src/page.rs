use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectReader, ReadResult};
use std::sync::Arc;

use pdf_annotation_core::SourceAnnotation;
use pdf_content_stream::ContentStream;
use pdf_graphics::rect::Rect;
use pdf_resources::resources::Resources;

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
    pub annotations: Option<Vec<SourceAnnotation>>,
    /// `/MediaBox` attribute which defines the page boundaries.
    pub media_box: Option<Rect>,
    /// Inherited visible page bounds.
    pub crop_box: Option<Rect>,
    /// Inherited clockwise page rotation in degrees.
    pub rotation: Option<i32>,
    /// `/Resources` attribute which defines the resources used by the page.
    pub resources: Option<Arc<Resources>>,
    /// Next page-scoped annotation identifier.
    #[doc(hidden)]
    pub annotation_id_high_watermark: usize,
    /// Retains the source and typed resources when a page is detached from its document.
    #[doc(hidden)]
    pub read_state: Option<Arc<ObjectReader<ObjectCollection>>>,
}

impl PdfPage {
    pub const KEY: &'static [u8] = b"Page";

    /// Returns an annotation by its stable page-scoped identifier.
    pub fn annotation(&self, id: u64) -> Option<&SourceAnnotation> {
        self.annotations
            .as_deref()?
            .iter()
            .find(|annotation| annotation.id == id)
    }

    /// Returns a mutable annotation by its stable page-scoped identifier.
    #[doc(hidden)]
    pub fn annotation_mut(&mut self, id: u64) -> Option<&mut SourceAnnotation> {
        self.annotations
            .as_deref_mut()?
            .iter_mut()
            .find(|annotation| annotation.id == id)
    }

    /// Reserves a new identifier that will not be reused during this page's lifetime.
    #[doc(hidden)]
    pub fn reserve_annotation_id(&mut self) -> Option<u64> {
        let next = self.annotation_id_high_watermark.checked_add(1)?;
        let id = u64::try_from(self.annotation_id_high_watermark).ok()?;
        self.annotation_id_high_watermark = next;
        Some(id)
    }

    /// Attaches an already materialized annotation to this page.
    #[doc(hidden)]
    pub fn push_annotation(&mut self, mut annotation: SourceAnnotation, id: u64) {
        annotation.id = id;
        self.annotations
            .get_or_insert_with(Vec::new)
            .push(annotation);
    }

    /// Removes an annotation by identifier.
    #[doc(hidden)]
    pub fn take_annotation(&mut self, id: u64) -> Option<SourceAnnotation> {
        let annotations = self.annotations.as_mut()?;
        let index = annotations
            .iter()
            .position(|annotation| annotation.id == id)?;
        let annotation = annotations.remove(index);
        if annotations.is_empty() {
            self.annotations = None;
        }
        Some(annotation)
    }
}

impl FromPdfObject for PdfPage {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let dictionary = context.dictionary().clone();
        let contents = context.optional::<ContentStream>(b"Contents")?;
        let media_box = dictionary.optional_media_box(context.source())?;
        let crop_box = dictionary
            .optional_array_of::<f32, 4>(b"CropBox", context.source())?
            .map(Rect::from);
        let rotation = context
            .dictionary()
            .optional_number::<i32>(b"Rotate", context.source())?;
        let resources = context
            .optional_shared::<Resources>(b"Resources")?
            .map(|handle| handle.get())
            .transpose()?;
        let annotations = SourceAnnotation::from_page_dictionary(&mut context)?;
        let annotation_id_high_watermark = annotations.as_ref().map_or(0, Vec::len);
        Ok(Self {
            contents,
            media_box,
            crop_box,
            rotation,
            resources,
            annotations,
            annotation_id_high_watermark,
            read_state: None,
        })
    }
}
