use crate::{error::PdfPagesError, resource::Resource};
use pdf_graphics::Image;
use pdf_image::{PdfImage, PdfImageError};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, pdf_name::PdfName,
};
use std::sync::Arc;

/// An image XObject whose metadata was read with the document but whose samples are
/// decoded only when it is drawn.
///
/// The stream data stays encoded, so an open document holds no image rasters; each
/// [`Self::decode`] produces one that lives as long as its caller keeps it.
#[derive(Debug)]
pub struct ImageXObject {
    image: PdfImage,
    soft_mask: Option<Arc<ImageXObject>>,
}

impl ImageXObject {
    /// Decodes the image and its soft mask into a raster.
    ///
    /// Returns `None` when the stream data cannot be decoded into samples, so the image
    /// cannot be drawn. A soft mask whose data cannot be decoded leaves the image
    /// unmasked.
    ///
    /// # Errors
    ///
    /// Returns [`PdfImageError`] when the image or its soft mask fails for any other
    /// reason.
    pub fn decode(&self) -> Result<Option<Image>, PdfImageError> {
        let soft_mask = match &self.soft_mask {
            Some(soft_mask) => soft_mask.decode()?,
            None => None,
        };
        match self.image.decode(soft_mask.as_ref()) {
            Ok(image) => Ok(Some(image)),
            Err(error) if error.is_unreadable_data() => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// An image XObject, or unavailable when its dimensions or filter chain are malformed.
pub(crate) enum ImageResource {
    Available(Arc<ImageXObject>),
    Unavailable,
}

impl FromPdfObject for ImageResource {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let object = context.object();
        let image = match context.read::<PdfImage>(object) {
            Ok(image) => image,
            Err(error) if PdfImage::is_unreadable(&error) => return Ok(Self::Unavailable),
            Err(error) => return Err(error),
        };
        let SoftMaskImage(soft_mask) = context.read(object)?;
        Ok(Self::Available(Arc::new(ImageXObject { image, soft_mask })))
    }
}

impl From<ImageResource> for Resource {
    fn from(image: ImageResource) -> Self {
        match image {
            ImageResource::Available(image) => Self::Image(image),
            ImageResource::Unavailable => Self::UnavailableImage,
        }
    }
}

/// An image's `/SMask`.
///
/// `/None`, an unavailable mask, or a mask that refers back to its own image leaves the
/// image unmasked.
struct SoftMaskImage(Option<Arc<ImageXObject>>);

impl FromPdfObject for SoftMaskImage {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        if !context.contains(b"SMask")
            || context
                .lenient::<PdfName>(b"SMask")
                .is_some_and(|name| name.as_bytes() == b"None")
        {
            return Ok(Self(None));
        }
        match context.required::<Resource>(b"SMask") {
            Ok(Resource::Image(image)) => Ok(Self(Some(image))),
            Ok(Resource::UnavailableImage) => Ok(Self(None)),
            Err(error) if error.is_cyclic_reference() => Ok(Self(None)),
            Ok(_) => Err(PdfPagesError::from(PdfImageError::InvalidSoftMaskXObject).into()),
            Err(error) => Err(error),
        }
    }
}
