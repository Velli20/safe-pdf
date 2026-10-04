use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::object_lookup::ObjectLookupExt;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectReader, ReadResult};
use std::sync::Arc;

use pdf_annotation_core::SourceAnnotation;
use pdf_content_stream::ContentStream;
use pdf_graphics::{
    rect::Rect,
    size::Size,
    viewport::{PageViewport, ViewportError},
};
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

    /// Returns the page's displayed width and height in PDF points.
    ///
    /// Uses the CropBox, then the MediaBox, and swaps the axes for a sideways `/Rotate`
    /// so hosts size their containers with the same geometry rendering uses.
    /// Returns `None` for pages without either box or with invalid bounds.
    pub fn page_size(&self) -> Option<Size> {
        if self.crop_box.is_none() && self.media_box.is_none() {
            return None;
        }
        let (bounds, rotation) = self.page_bounds(None, || Rect::new(1.0, 1.0))?;
        let (width, height) = (bounds.width(), bounds.height());
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        Some(Size::new(width, height).quarter_turned(rotation))
    }

    /// Fits `bounds_override`, or the page box, into `device_size` with the page `/Rotate`.
    ///
    /// Pages without a box fit the device size itself. Rejects invalid device
    /// dimensions before resolving bounds, then invalid bounds.
    pub fn viewport(
        &self,
        bounds_override: Option<&Rect>,
        device_size: Size,
    ) -> Result<PageViewport, ViewportError> {
        if !device_size.validate() {
            return Err(ViewportError::Dimensions);
        }
        let (bounds, rotation) = self
            .page_bounds(bounds_override, || {
                Rect::new(device_size.width, device_size.height)
            })
            .ok_or(ViewportError::Bounds)?;
        PageViewport::new(bounds, rotation, device_size)
    }

    /// Resolves the page box and normalized `/Rotate` shared by rendering and layout.
    ///
    /// The box is chosen in priority order: `bounds_override`, then the CropBox, then
    /// the MediaBox, and finally the box produced by `fallback`.
    ///
    /// # Parameters
    ///
    /// - `bounds_override`: Bounds in PDF page coordinates that take precedence over
    ///   the page's own boxes, such as a caller-selected clip region.
    /// - `fallback`: Produces the bounds used when neither an override nor a page box
    ///   is available. It is only called in that case.
    ///
    /// # Returns
    ///
    /// The selected bounds and the page rotation in degrees, normalized into
    /// `0..360` (an absent `/Rotate` is `0`). Returns `None` if the selected bounds
    /// are not valid or have a nonfinite width or height.
    pub fn page_bounds(
        &self,
        bounds_override: Option<&Rect>,
        fallback: impl FnOnce() -> Rect,
    ) -> Option<(Rect, i32)> {
        let bounds = bounds_override
            .or(self.crop_box.as_ref())
            .or(self.media_box.as_ref())
            .copied()
            .unwrap_or_else(fallback);
        if !bounds.is_valid() || !bounds.width().is_finite() || !bounds.height().is_finite() {
            return None;
        }
        Some((bounds, self.rotation.unwrap_or_default().rem_euclid(360)))
    }
}

impl FromPdfObject for PdfPage {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let dictionary = context.dictionary();
        // A dangling `/Contents` reference is null (ISO 32000 §7.3.10), leaving the page empty.
        let contents_absent = match dictionary.get(b"Contents") {
            Some(object) => context.is_absent(object)?,
            None => true,
        };
        let contents = if contents_absent {
            None
        } else {
            context.optional::<ContentStream>(b"Contents")?
        };
        let media_box = dictionary.optional_media_box(context.source())?;
        let crop_box = dictionary
            .optional_array_of::<f32, 4>(b"CropBox", context.source())?
            .map(Rect::from);
        let rotation = dictionary.optional_number::<i32>(b"Rotate", context.source())?;
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
