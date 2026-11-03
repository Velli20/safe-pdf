use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ObjectHandle, ReadResult};
use std::sync::Arc;

use pdf_annotation_core::{PageAnnotations, SourceAnnotation};
use pdf_graphics::{
    rect::Rect,
    size::Size,
    viewport::{PageViewport, ViewportError},
};
use pdf_resources::resources::Resources;

use crate::page_contents::PageContents;

/// Represents a single page in a PDF document.
///
/// A page object is a dictionary that describes a single page of a document.
/// It contains references to the page's contents (the text, graphics, and images),
/// its resources, and other attributes according to PDF 1.7 specification.
#[derive(Default)]
pub struct PdfPage {
    /// The unparsed contents of the page, which can be a single stream object or
    /// an array of streams. Read the operators with [`crate::document::PdfDocument::content_stream`].
    pub contents: Option<PageContents>,
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

    /// US Letter bounds in points, used when a page has no usable box.
    const LETTER: Rect = Rect::new(612.0, 792.0);

    /// Returns the page's displayed width and height in PDF points.
    ///
    /// Uses the bounds resolved by [`Self::page_bounds`] and swaps the axes for a
    /// sideways `/Rotate` so hosts size their containers with the same geometry
    /// rendering uses.
    pub fn page_size(&self) -> Option<Size> {
        let (bounds, rotation) = self.page_bounds(None)?;
        Some(bounds.size().quarter_turned(rotation))
    }

    /// Fits `bounds_override`, or the page box, into `device_size` with the page `/Rotate`.
    ///
    /// Rejects invalid device dimensions before resolving bounds, then invalid bounds.
    pub fn viewport(
        &self,
        bounds_override: Option<&Rect>,
        device_size: Size,
    ) -> Result<PageViewport, ViewportError> {
        if !device_size.validate() {
            return Err(ViewportError::Dimensions);
        }
        let (bounds, rotation) = self
            .page_bounds(bounds_override)
            .ok_or(ViewportError::Bounds)?;
        PageViewport::new(bounds, rotation, device_size)
    }

    /// Resolves the page box and normalized `/Rotate` shared by rendering and layout.
    ///
    /// The box is chosen in priority order: `bounds_override`, then the page box
    /// resolved by [`Self::page_box`], and finally US Letter.
    ///
    /// # Parameters
    ///
    /// - `bounds_override`: Bounds in PDF page coordinates that take precedence over
    ///   the page's own boxes, such as a caller-selected clip region.
    ///
    /// # Returns
    ///
    /// The selected bounds and the page rotation in degrees, normalized into
    /// `0..360` (an absent `/Rotate` is `0`). Returns `None` if `bounds_override`
    /// does not have finite edges and positive dimensions.
    pub fn page_bounds(&self, bounds_override: Option<&Rect>) -> Option<(Rect, i32)> {
        let bounds = bounds_override
            .copied()
            .or_else(|| self.page_box())
            .unwrap_or(Self::LETTER);
        if !bounds.is_valid() {
            return None;
        }
        Some((bounds, self.rotation.unwrap_or_default().rem_euclid(360)))
    }

    /// Resolves the page's own visible box with normalized edges.
    ///
    /// An empty MediaBox is ignored, a missing or empty one bounds the CropBox as US
    /// Letter would, and a CropBox that does not overlap it yields the MediaBox.
    /// Returns `None` when neither box gives positive, finite bounds.
    fn page_box(&self) -> Option<Rect> {
        let media_box = self
            .media_box
            .map(|media_box| media_box.normalized())
            .filter(|media_box| media_box.is_valid());
        self.crop_box
            .and_then(|crop_box| crop_box.intersection(&media_box.unwrap_or(Self::LETTER)))
            .or(media_box)
    }
}

impl FromPdfObject for PdfPage {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let contents = context.optional::<PageContents>(b"Contents")?;
        let media_box = context.lenient(b"MediaBox");
        let crop_box = context.lenient(b"CropBox");
        let rotation = context.optional::<i32>(b"Rotate")?;
        let resources = context
            .optional::<ObjectHandle<Resources>>(b"Resources")?
            .map(|handle| handle.get())
            .transpose()?;
        let annotations = context
            .optional::<PageAnnotations>(b"Annots")?
            .and_then(|PageAnnotations(annotations)| annotations);
        let annotation_id_high_watermark = annotations.as_ref().map_or(0, Vec::len);
        Ok(Self {
            contents,
            media_box,
            crop_box,
            rotation,
            resources,
            annotations,
            annotation_id_high_watermark,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_resolves_malformed_boxes() {
        let page = |media_box: Option<[f32; 4]>, crop_box: Option<[f32; 4]>| PdfPage {
            media_box: media_box.map(Rect::from),
            crop_box: crop_box.map(Rect::from),
            ..PdfPage::default()
        };
        let letter = Some(Size::new(612.0, 792.0));

        let inverted = page(Some([0.0, 792.0, 612.0, 0.0]), None);
        assert_eq!(inverted.page_size(), letter);
        assert_eq!(
            inverted.page_bounds(None),
            Some((Rect::new(612.0, 792.0), 0))
        );

        let empty = page(Some([0.0; 4]), Some([0.0; 4]));
        assert_eq!(empty.page_size(), letter);

        let disjoint = page(
            Some([0.0, 0.0, 800.0, 600.0]),
            Some([600.0, 800.0, 1000.0, 1000.0]),
        );
        assert_eq!(disjoint.page_size(), Some(Size::new(800.0, 600.0)));

        assert_eq!(page(None, None).page_size(), letter);
    }
}
