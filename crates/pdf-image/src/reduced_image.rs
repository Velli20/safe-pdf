//! Decoding of images whose sample grid is larger than an image is held at for drawing.
//!
//! Such an image is reduced by a whole factor while it is decoded: each output pixel
//! averages the block of source pixels it covers. The source is decoded a band of rows
//! at a time, so no full-resolution component or RGBA buffer is ever built. Gray and RGB
//! samples are averaged before their conversion to render-ready pixels, which then only
//! runs on the reduced image.

use bytes::Bytes;
use pdf_color_space::color_space::ColorSpace;
use pdf_graphics::{Image, PixelFormat, rect::Rect};

use crate::decoded_samples::DecodedSamples;
use crate::error::{ImageRasterError, PdfImageError};
use crate::image_metadata::ImageMetadata;

/// Limits on the pixels an image is held at once decoded.
pub(crate) struct PixelBudget {
    /// Most pixels the image may have.
    pub(crate) pixels: usize,
    /// Longest side, in pixels, the image may have.
    pub(crate) side: usize,
}

impl PixelBudget {
    /// Returns the smallest whole factor that, dividing both sides of a `width` × `height`
    /// image and rounding up, brings it within this budget.
    pub(crate) fn reduction_factor(&self, width: usize, height: usize) -> usize {
        let longest = width.max(height).max(1);
        (1..=longest)
            .find(|&factor| {
                let (width, height) = (width.div_ceil(factor), height.div_ceil(factor));
                width <= self.side
                    && height <= self.side
                    && width.saturating_mul(height) <= self.pixels
            })
            .unwrap_or(longest)
    }
}

/// How the bytes of one pixel are averaged.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PixelLayout {
    /// One gray sample.
    Gray,
    /// Red, green and blue samples, averaged independently.
    Rgb,
    /// Straight RGBA, whose colour is weighted by alpha.
    Rgba,
}

impl PixelLayout {
    /// Returns the layout of decoded samples, or `None` for components that must be
    /// converted to RGBA before they can be averaged.
    fn of_samples(samples: &DecodedSamples) -> Option<Self> {
        match (samples.is_rgba, samples.num_color_components) {
            (true, _) => Some(Self::Rgba),
            (false, 1) => Some(Self::Gray),
            (false, 3) => Some(Self::Rgb),
            _ => None,
        }
    }

    /// Returns the bytes in one pixel.
    fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }
}

impl From<PixelFormat> for PixelLayout {
    fn from(format: PixelFormat) -> Self {
        match format {
            PixelFormat::Gray8 => Self::Gray,
            PixelFormat::RGBA8888 => Self::Rgba,
        }
    }
}

/// Averages blocks of `factor` × `factor` source pixels into single pixels.
struct BoxReduction {
    /// Side of the averaged blocks, in source pixels.
    factor: usize,
    /// Width of the source rows, in pixels.
    source_width: usize,
    /// Layout of both source and output pixels.
    layout: PixelLayout,
}

impl BoxReduction {
    /// Returns the width of the reduced rows.
    fn output_width(&self) -> usize {
        self.source_width.div_ceil(self.factor)
    }

    /// Averages `band`, up to `factor` packed source rows, into one output row appended to
    /// `output`. A trailing partial row is ignored.
    fn reduce_band(&self, band: &[u8], output: &mut Vec<u8>) {
        let bytes_per_pixel = self.layout.bytes_per_pixel();
        let stride = self.source_width.saturating_mul(bytes_per_pixel).max(1);
        let block_len = self.factor.saturating_mul(bytes_per_pixel).max(1);
        let mut sums = vec![BlockSum::default(); self.output_width()];
        for row in band.chunks_exact(stride) {
            for (sum, block) in sums.iter_mut().zip(row.chunks(block_len)) {
                match self.layout {
                    PixelLayout::Gray => sum.add_gray(block),
                    PixelLayout::Rgb => sum.add_rgb(block.as_chunks::<3>().0),
                    PixelLayout::Rgba => sum.add_rgba(block.as_chunks::<4>().0),
                }
            }
        }
        for sum in &sums {
            sum.write(self.layout, output);
        }
    }

    /// Allocates the buffer for `height` reduced rows.
    fn buffer(&self, height: usize) -> Result<Vec<u8>, PdfImageError> {
        let byte_len = self
            .output_width()
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(self.layout.bytes_per_pixel()))
            .ok_or(PdfImageError::Raster(ImageRasterError::ResourceLimit))?;
        let mut data = Vec::new();
        data.try_reserve_exact(byte_len)
            .map_err(|_| PdfImageError::Raster(ImageRasterError::ResourceLimit))?;
        Ok(data)
    }
}

/// Reduces a whole render-ready image.
pub(crate) trait ReduceImage {
    /// Returns the image reduced by `factor`.
    ///
    /// # Errors
    ///
    /// Returns a resource-limit error if the reduced pixels cannot be allocated.
    fn reduced(&self, factor: usize) -> Result<Image, PdfImageError>;
}

impl ReduceImage for Image {
    fn reduced(&self, factor: usize) -> Result<Image, PdfImageError> {
        let reduction = BoxReduction {
            factor,
            source_width: self.width,
            layout: self.pixel_format.into(),
        };
        let height = self.height.div_ceil(factor);
        let mut data = reduction.buffer(height)?;
        let band_len = self
            .width
            .saturating_mul(reduction.layout.bytes_per_pixel())
            .saturating_mul(factor);
        for band in self.data.chunks(band_len.max(1)).take(height) {
            reduction.reduce_band(band, &mut data);
        }
        Ok(Image {
            data: data.into(),
            width: reduction.output_width(),
            height,
            pixel_format: self.pixel_format,
            interpolate: self.interpolate,
        })
    }
}

/// Running totals of the pixels in one block.
#[derive(Clone, Default)]
struct BlockSum {
    /// Number of pixels added.
    count: u64,
    /// Sum of each channel; RGBA colour is weighted by alpha.
    channels: [u64; 3],
    /// Sum of alpha.
    alpha: u64,
}

impl BlockSum {
    /// Adds gray pixels.
    fn add_gray(&mut self, pixels: &[u8]) {
        let [gray, _, _] = &mut self.channels;
        for sample in pixels {
            *gray = gray.wrapping_add(u64::from(*sample));
        }
        self.add_count(pixels.len());
    }

    /// Adds RGB pixels, averaging each channel independently.
    fn add_rgb(&mut self, pixels: &[[u8; 3]]) {
        let [red_sum, green_sum, blue_sum] = &mut self.channels;
        for [red, green, blue] in pixels {
            *red_sum = red_sum.wrapping_add(u64::from(*red));
            *green_sum = green_sum.wrapping_add(u64::from(*green));
            *blue_sum = blue_sum.wrapping_add(u64::from(*blue));
        }
        self.add_count(pixels.len());
    }

    /// Adds straight RGBA pixels, weighting their colour by alpha.
    fn add_rgba(&mut self, pixels: &[[u8; 4]]) {
        let [red_sum, green_sum, blue_sum] = &mut self.channels;
        for [red, green, blue, alpha] in pixels {
            let alpha = u64::from(*alpha);
            *red_sum = red_sum.wrapping_add(u64::from(*red).wrapping_mul(alpha));
            *green_sum = green_sum.wrapping_add(u64::from(*green).wrapping_mul(alpha));
            *blue_sum = blue_sum.wrapping_add(u64::from(*blue).wrapping_mul(alpha));
            self.alpha = self.alpha.wrapping_add(alpha);
        }
        self.add_count(pixels.len());
    }

    /// Counts `pixels` more pixels.
    ///
    /// Sums cannot overflow: a block holds at most `usize::MAX` pixels of at most
    /// 255 × 255 each, far below `u64::MAX` for any image held in memory.
    fn add_count(&mut self, pixels: usize) {
        self.count = self
            .count
            .wrapping_add(u64::try_from(pixels).unwrap_or(u64::MAX));
    }

    /// Appends the block's average as one pixel of `layout`.
    fn write(&self, layout: PixelLayout, output: &mut Vec<u8>) {
        let [red, green, blue] = self.channels;
        match layout {
            PixelLayout::Gray => output.push(rounded_quotient(red, self.count)),
            PixelLayout::Rgb => output.extend_from_slice(&[
                rounded_quotient(red, self.count),
                rounded_quotient(green, self.count),
                rounded_quotient(blue, self.count),
            ]),
            PixelLayout::Rgba => output.extend_from_slice(&[
                rounded_quotient(red, self.alpha),
                rounded_quotient(green, self.alpha),
                rounded_quotient(blue, self.alpha),
                rounded_quotient(self.alpha, self.count),
            ]),
        }
    }
}

/// Divides `sum` by `count` to the nearest byte value, giving zero for an empty count.
fn rounded_quotient(sum: u64, count: u64) -> u8 {
    let quotient = sum
        .saturating_add(count / 2)
        .checked_div(count)
        .unwrap_or(0);
    u8::try_from(quotient).unwrap_or(u8::MAX)
}

/// Decodes an image's samples a band of `factor` rows at a time, reducing each band to one
/// output row as it goes.
pub(crate) struct BandDecode<'a> {
    /// The filtered sample bytes of the whole image.
    samples: &'a Bytes,
    /// The image's metadata.
    metadata: &'a ImageMetadata,
    /// Side of the averaged blocks, in source pixels.
    factor: usize,
    /// Bytes in one packed source row.
    row_len: usize,
}

impl<'a> BandDecode<'a> {
    /// Prepares band decoding of `samples`, or returns `None` when they are shorter than
    /// the declared rows, which only decoding the whole image interprets.
    pub(crate) fn new(
        samples: &'a Bytes,
        metadata: &'a ImageMetadata,
        factor: usize,
    ) -> Option<Self> {
        let row_len = metadata.row_len(samples)?;
        let image_len = row_len.checked_mul(metadata.size.height())?;
        (row_len > 0 && samples.len() >= image_len).then_some(Self {
            samples,
            metadata,
            factor,
            row_len,
        })
    }

    /// Returns the metadata of the reduced image.
    pub(crate) fn reduced_metadata(&self) -> ImageMetadata {
        let mut metadata = self.metadata.clone();
        metadata.size = Rect::from_size(
            self.metadata.size.width().div_ceil(self.factor),
            self.metadata.size.height().div_ceil(self.factor),
        );
        metadata
    }

    /// Decodes each band with `decode` and reduces it into samples of the reduced image.
    ///
    /// `decode` turns a band's filtered samples into decoded samples, given metadata whose
    /// height is the band's. It must convert components other than gray and RGB to RGBA,
    /// since only those average independently of their colour space.
    ///
    /// # Errors
    ///
    /// Returns errors from `decode`, an error for components it left unconverted, or a
    /// resource-limit error if the reduced samples cannot be allocated.
    pub(crate) fn decode(
        &self,
        mut decode: impl FnMut(Bytes, &ImageMetadata) -> Result<DecodedSamples, PdfImageError>,
    ) -> Result<DecodedSamples, PdfImageError> {
        let (width, height) = (self.metadata.size.width(), self.metadata.size.height());
        let output_height = height.div_ceil(self.factor);
        let mut band_metadata = self.metadata.clone();
        let mut output: Option<(BoxReduction, Vec<u8>)> = None;
        for first_row in (0..height).step_by(self.factor) {
            let rows = self.factor.min(height.saturating_sub(first_row));
            band_metadata.size = Rect::from_size(width, rows);
            let start = first_row.saturating_mul(self.row_len);
            let band = self
                .samples
                .slice(start..start.saturating_add(rows.saturating_mul(self.row_len)));
            let band = decode(band, &band_metadata)?;
            let (reduction, data) = match &mut output {
                Some(output) => output,
                None => {
                    let reduction = BoxReduction {
                        factor: self.factor,
                        source_width: width,
                        layout: PixelLayout::of_samples(&band)
                            .ok_or(PdfImageError::InvalidColorComponentCount)?,
                    };
                    let data = reduction.buffer(output_height)?;
                    output.insert((reduction, data))
                }
            };
            reduction.reduce_band(&band.image_data, data);
        }
        let (reduction, data) = output.ok_or(PdfImageError::Raster(
            ImageRasterError::InvalidInput("image dimensions"),
        ))?;
        Ok(DecodedSamples {
            num_color_components: reduction.layout.bytes_per_pixel(),
            image_data: data.into(),
            is_rgba: reduction.layout == PixelLayout::Rgba,
        })
    }
}

impl ImageMetadata {
    /// Returns the bytes in one packed row of the filtered `samples`.
    fn row_len(&self, samples: &[u8]) -> Option<usize> {
        Some(
            self.size
                .width()
                .checked_mul(self.samples_per_pixel(samples))?
                .checked_mul(self.bits_per_component)?
                .div_ceil(8),
        )
    }

    /// Returns how many samples make up one pixel of the filtered `samples`.
    ///
    /// A DCT decoder may already have converted colour, so its output length decides
    /// the count when it matches a whole number of components per pixel.
    fn samples_per_pixel(&self, samples: &[u8]) -> usize {
        let has_dct_filter = self
            .filters
            .as_ref()
            .is_some_and(|filters| filters.has_dct_filter());
        let pixels = self.size.width().saturating_mul(self.size.height());
        let decoded_dct_components = [1, 3, 4]
            .into_iter()
            .find(|components| samples.len() == pixels.saturating_mul(*components))
            .filter(|_| has_dct_filter && self.bits_per_component == 8);
        match (decoded_dct_components, &self.color_space) {
            (Some(components), _) => components,
            (None, Some(ColorSpace::Indexed(_)) | None) => 1,
            (None, Some(color_space)) => color_space.num_color_components(),
        }
    }
}
