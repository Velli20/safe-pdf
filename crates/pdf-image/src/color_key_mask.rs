use pdf_decode::{SampleLayout, decode_sample_codes};
use pdf_graphics::{Image, PixelFormat};
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

use crate::{error::PdfImageError, image_metadata::ImageMetadata};

/// An image's color-key `/Mask` array (ISO 32000 §8.9.6.4).
///
/// It gives one `[min max]` range of sample codes per color component. A pixel whose
/// every component lies in its range is not painted. The codes are compared as stored,
/// before `/Decode` maps them.
#[derive(Debug, Clone)]
pub(crate) struct ColorKeyMask {
    ranges: Vec<[f32; 2]>,
}

impl FromPdfObject for ColorKeyMask {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let values: Vec<f32> = context.decode()?;
        Ok(Self {
            ranges: values.as_chunks::<2>().0.to_vec(),
        })
    }
}

impl ColorKeyMask {
    /// Returns the mask for an image with `components` color components, or `None` when
    /// the array gives too few ranges. Ranges beyond the component count are ignored.
    pub(crate) fn for_components(mut self, components: usize) -> Option<Self> {
        if self.ranges.len() < components {
            return None;
        }
        self.ranges.truncate(components);
        Some(self)
    }

    /// Builds the coverage of an image's filtered `samples`: a gray raster that is 0 where
    /// a pixel's color matches the key and 255 elsewhere.
    ///
    /// # Errors
    ///
    /// Returns [`PdfImageError`] when the samples are shorter than the image's rows.
    pub(crate) fn coverage(
        &self,
        samples: &[u8],
        metadata: &ImageMetadata,
    ) -> Result<Image, PdfImageError> {
        let (width, height) = (metadata.size.width(), metadata.size.height());
        let components = self.ranges.len();
        let codes = decode_sample_codes(
            samples,
            metadata.bits_per_component,
            SampleLayout::RowAligned {
                width,
                height,
                samples_per_pixel: components,
            },
        )?;
        let data: Vec<u8> = codes
            .chunks_exact(components.max(1))
            .map(|pixel| self.matches(pixel))
            .map(|keyed| if keyed { 0 } else { u8::MAX })
            .collect();
        Ok(Image {
            data: data.into(),
            width,
            height,
            pixel_format: PixelFormat::Gray8,
            interpolate: false,
        })
    }

    /// Returns whether every component code of `pixel` lies in its range.
    fn matches(&self, pixel: &[u32]) -> bool {
        // Sample codes are at most 16 bits wide.
        pixel.iter().zip(&self.ranges).all(|(&code, [min, max])| {
            u16::try_from(code).is_ok_and(|code| (*min..=*max).contains(&f32::from(code)))
        })
    }
}
