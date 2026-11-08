use crate::{error::PdfPagesError, resource::Resource};
use pdf_graphics::{Image, PixelFormat};
use pdf_image::{PdfImage, PdfImageError};
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
    pdf_name::PdfName,
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
    mask: Option<ImageMask>,
}

impl ImageXObject {
    /// Decodes the image and its mask into a raster.
    ///
    /// Returns `None` when the stream data cannot be decoded into samples, so the image
    /// cannot be drawn. A soft mask or stencil mask whose data cannot be decoded leaves
    /// the image unmasked.
    ///
    /// # Errors
    ///
    /// Returns [`PdfImageError`] when the image or its soft mask fails for any other
    /// reason.
    pub fn decode(&self) -> Result<Option<Image>, PdfImageError> {
        let soft_mask = match &self.mask {
            Some(mask) => mask.decode()?,
            None => None,
        };
        match self.image.decode(soft_mask.as_ref()) {
            Ok(image) => Ok(Some(image)),
            Err(error) if error.is_unreadable_data() => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Returns whether the image has its own `/SMask`, which replaces the graphics
    /// state's soft mask when the image is painted.
    pub fn has_soft_mask(&self) -> bool {
        matches!(self.mask, Some(ImageMask::Soft(_)))
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
        let mask = match context.read(object)? {
            SoftMaskImage(Some(soft_mask)) => Some(ImageMask::Soft(soft_mask)),
            SoftMaskImage(None) => context
                .dictionary()?
                .optional::<StencilMaskImage>(b"Mask")?
                .and_then(|StencilMaskImage(stencil)| stencil)
                .map(ImageMask::Stencil),
        };
        Ok(Self::Available(Arc::new(ImageXObject { image, mask })))
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

/// What masks an image: its `/SMask` or, without one, its `/Mask` stencil.
#[derive(Debug)]
enum ImageMask {
    Soft(Arc<ImageXObject>),
    Stencil(Arc<ImageXObject>),
}

impl ImageMask {
    /// Decodes the mask into a raster whose coverage is the image's opacity.
    fn decode(&self) -> Result<Option<Image>, PdfImageError> {
        match self {
            Self::Soft(mask) => mask.decode(),
            // A stencil decodes to coverage in alpha, 255 where the image is painted.
            // As gray, that coverage reads as soft-mask opacity.
            Self::Stencil(mask) => Ok(mask.decode()?.map(|mask| match mask.pixel_format {
                PixelFormat::Alpha8 => Image {
                    pixel_format: PixelFormat::Gray8,
                    ..mask
                },
                _ => mask,
            })),
        }
    }
}

/// An image's `/Mask` stencil: an `/ImageMask` image whose painted samples mark where
/// the image shows.
///
/// A color-key array, a stream that is not a stencil mask, an unavailable mask, or a
/// mask that refers back to its own image leaves the image unmasked.
struct StencilMaskImage(Option<Arc<ImageXObject>>);

impl FromPdfObject for StencilMaskImage {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let value = context.object();
        if !matches!(value, ObjectVariant::Stream(_)) {
            return Ok(Self(None));
        }
        match context.read::<ImageResource>(value) {
            Ok(ImageResource::Available(mask)) if mask.image.is_image_mask() => {
                Ok(Self(Some(mask)))
            }
            Ok(_) => Ok(Self(None)),
            Err(error) if error.is_cyclic_reference() => Ok(Self(None)),
            Err(error) => Err(error),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;

    use pdf_graphics::PixelFormat;
    use pdf_object_reader::{
        EmptySource, dictionary::Dictionary, object_variant::ObjectVariant, pdf_string::PdfString,
        stream::StreamObject, string_kind::StringKind,
    };

    use super::ImageResource;

    fn name(value: &[u8]) -> ObjectVariant {
        PdfString::from(value, StringKind::Name)
    }

    fn image_stream(entries: Vec<(&[u8], ObjectVariant)>, data: Vec<u8>) -> ObjectVariant {
        let mut dictionary = BTreeMap::from([
            (Vec::from(b"Subtype"), name(b"Image")),
            (Vec::from(b"Width"), ObjectVariant::Integer(2)),
            (Vec::from(b"Height"), ObjectVariant::Integer(1)),
        ]);
        dictionary.extend(
            entries
                .into_iter()
                .map(|(key, value)| (Vec::from(key), value)),
        );
        ObjectVariant::Stream(StreamObject::new(
            1,
            0,
            Dictionary::from_iter(dictionary),
            data,
        ))
    }

    #[test]
    fn stencil_mask_hides_the_image_where_its_samples_are_one() {
        let stencil = image_stream(
            vec![(b"ImageMask", ObjectVariant::Boolean(true))],
            vec![0b0100_0000],
        );
        let image = image_stream(
            vec![
                (b"ColorSpace", name(b"DeviceGray")),
                (b"BitsPerComponent", ObjectVariant::Integer(8)),
                (b"Mask", stencil),
            ],
            vec![0x10, 0x20],
        );

        let resource = pdf_object_reader::ObjectReader::new(EmptySource)
            .read::<ImageResource>(&image)
            .expect("masked image should read");
        let ImageResource::Available(image) = resource else {
            unreachable!("masked image should be available");
        };
        let raster = image
            .decode()
            .expect("masked image should decode")
            .expect("masked image should have samples");

        assert_eq!(raster.pixel_format, PixelFormat::RGBA8888);
        let alphas: Vec<u8> = raster
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|[_, _, _, alpha]| *alpha)
            .collect();
        assert_eq!(alphas, [255, 0]);
    }
}
