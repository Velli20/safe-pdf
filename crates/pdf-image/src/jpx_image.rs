//! JPEG 2000 image data resolved into the channels a PDF image consumes.
//!
//! This module runs the `pdf-jpeg2000` decoder over one PDF image stream and
//! collects the result through its [`ByteImage`] sink as 8-bit colour samples
//! with an optional opacity plane, which is the form the rest of `pdf-image`
//! works in, then resolves the colour space PDF and the container describe.

use pdf_jpeg2000::{
    ByteImage, Decoder, DecoderLimits, DecoderOptions, EnumeratedColorSpace, InputFormat,
};

use pdf_color_space::color_space::ColorSpace;

use crate::{error::PdfImageError, smask_in_data::SMaskInData};

/// Bounds applied to an untrusted JPEG 2000 image in a PDF.
///
/// `pdf-jpeg2000` deliberately offers no default, because only the caller knows
/// its rendering budget. These are ceilings that reject an absurd image before
/// anything is allocated, not sizes this crate expects to reach.
const LIMITS: DecoderLimits = DecoderLimits {
    // Compressed image streams far beyond this are not worth a decode attempt.
    max_input_bytes: 128 << 20,
    // Roughly a 16 000 by 16 000 reference grid.
    max_pixels: 1 << 28,
    // CMYK with opacity needs five; the rest is headroom for exotic files.
    max_components: 32,
    max_tiles: 1 << 16,
    // Peak temporary storage for one tile, including its coefficient planes.
    max_working_bytes: 1 << 30,
};

/// A JPEG 2000 image resolved into the 8-bit channels a PDF image consumes.
///
/// Colour samples are interleaved in channel order. Opacity, when the image
/// carries it, stays in its own plane because `/SMaskInData` decides whether it
/// is used at all.
pub(crate) struct JpxImage {
    /// Image width in the reference grid.
    pub(crate) width: usize,
    /// Image height in the reference grid.
    pub(crate) height: usize,
    /// Number of interleaved colour channels.
    pub(crate) color_components: usize,
    /// Interleaved colour samples, `width * height * color_components` long.
    pub(crate) color: Vec<u8>,
    /// Opacity samples, one per pixel, when a channel carries them.
    pub(crate) opacity: Option<Vec<u8>>,
    /// Whether the colour samples were multiplied by that opacity.
    pub(crate) premultiplied: bool,
    /// The colour space the container names, when it names a known one.
    pub(crate) space: Option<EnumeratedColorSpace>,
}

impl JpxImage {
    /// Decodes a JPEG 2000 codestream or JP2/JPX container into channels.
    ///
    /// # Errors
    ///
    /// Returns [`PdfImageError::Jpx`] for malformed or oversized image data,
    /// including channels that do not fit in memory, and
    /// [`PdfImageError::InvalidColorComponentCount`] when no channel carries
    /// colour.
    pub(crate) fn decode(codestream: &[u8]) -> Result<Self, PdfImageError> {
        let options = DecoderOptions {
            format: InputFormat::Auto,
            limits: LIMITS,
        };
        let decoder = Decoder::new(trim_stream_padding(codestream), options).parse_header()?;
        // An image too large to reconstruct within the limits is decoded at a
        // lower resolution and scaled back up, rather than not shown at all.
        let levels = decoder.levels_to_fit();
        let decoder = decoder.discard_levels(levels);

        let header = decoder.header();
        let size = *header.main().size();
        let channels = header.channels();
        let space = header
            .color_specification()
            .and_then(|specification| specification.enumerated_space());

        let mut image = ByteImage::new(size, channels)?;
        if image.color_components() == 0 {
            return Err(PdfImageError::InvalidColorComponentCount);
        }
        let mut decoded = decoder.decode_next_tile(&mut image)?;
        while decoded.remaining_tiles() > 0 {
            decoded = decoded.decode_next_tile(&mut image)?;
        }
        decoded.finish()?;

        let width = image.width();
        let height = image.height();
        let color_components = image.color_components();
        let premultiplied = image.premultiplied();
        let (color, opacity) = image.into_planes();
        Ok(Self {
            width,
            height,
            color_components,
            color,
            opacity,
            premultiplied,
            space,
        })
    }
}

/// Drops the whitespace a PDF `/Length` may count past the image data.
///
/// A JP2 file ends on a box boundary and a raw codestream ends at EOC, so a
/// decoder rejects anything after them. PDF stream lengths routinely include
/// the end-of-line before `endstream`, which would otherwise turn an ordinary
/// file into a structural error.
fn trim_stream_padding(codestream: &[u8]) -> &[u8] {
    let end = codestream
        .iter()
        .rposition(|byte| !matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
        .map_or(0, |index| index.saturating_add(1));
    codestream.get(..end).unwrap_or(codestream)
}

/// Allocates a zero-filled buffer without aborting on a failed reservation.
fn zeroed(len: usize) -> Option<Vec<u8>> {
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(len).ok()?;
    buffer.resize(len, 0);
    Some(buffer)
}

/// A JPEG 2000 image resolved into samples of one PDF colour space.
pub(crate) struct JpxDisplay {
    /// The colour space the samples are expressed in.
    pub(crate) color_space: ColorSpace,
    /// Number of interleaved colour components per pixel.
    pub(crate) components: usize,
    /// Interleaved 8-bit colour samples at the requested extent.
    pub(crate) color: Vec<u8>,
    /// Straight, unpremultiplied alpha, when `/SMaskInData` selects it.
    pub(crate) alpha: Option<Vec<u8>>,
}

impl JpxImage {
    /// Resolves decoded channels into samples of the colour space that applies.
    ///
    /// A PDF `/ColorSpace` entry overrides the container's own description for
    /// a JPX image, so the embedded description is interpreted only in its
    /// absence, and only then is a transform such as sYCC to RGB applied.
    /// `width` and `height` come from the image dictionary; a codestream of a
    /// different extent is cropped or padded so the caller's geometry holds.
    ///
    /// # Errors
    ///
    /// Returns [`PdfImageError::InvalidColorComponentCount`] when no colour
    /// space fits the channels, and [`PdfImageError::JpxImageTooLarge`] when
    /// the requested extent does not fit in memory.
    pub(crate) fn into_display(
        self,
        declared: Option<&ColorSpace>,
        smask_in_data: SMaskInData,
        width: usize,
        height: usize,
    ) -> Result<JpxDisplay, PdfImageError> {
        let alpha = match smask_in_data.overrides_soft_mask() {
            true => self.opacity.clone(),
            false => None,
        };
        let embedded = declared.is_none().then_some(self.space).flatten();
        let transform = embedded.and_then(EmbeddedTransform::of);
        let components = match transform {
            Some(EmbeddedTransform::AddBlack) => self.color_components.saturating_add(1),
            _ => self.color_components,
        };
        let color_space = match declared {
            Some(declared) if declared.num_color_components() == components => declared.clone(),
            _ => default_color_space(embedded, components)?,
        };

        let mut display = JpxDisplay {
            color_space,
            components,
            color: self.color,
            alpha,
        };
        if let Some(transform) = transform {
            transform.apply(&mut display, self.color_components)?;
        }
        if display.alpha.is_some()
            && (self.premultiplied || matches!(smask_in_data, SMaskInData::PremultipliedOpacity))
        {
            display.unpremultiply();
        }
        display.fit(self.width, self.height, width, height)?;
        Ok(display)
    }
}

impl JpxDisplay {
    /// Divides premultiplied colour samples back out by their opacity.
    ///
    /// A fully transparent pixel carries no colour information, so it is left
    /// at zero rather than amplified.
    fn unpremultiply(&mut self) {
        let Some(alpha) = self.alpha.as_ref() else {
            return;
        };
        for (pixel, opacity) in self
            .color
            .chunks_exact_mut(self.components.max(1))
            .zip(alpha.iter().copied())
        {
            for sample in pixel.iter_mut() {
                *sample = match opacity {
                    0 => 0,
                    opacity => {
                        let scaled = u32::from(*sample)
                            .saturating_mul(u32::from(u8::MAX))
                            .checked_div(u32::from(opacity))
                            .unwrap_or(0);
                        u8::try_from(scaled.min(u32::from(u8::MAX))).unwrap_or(u8::MAX)
                    }
                };
            }
        }
    }

    /// Crops or pads the samples to the extent the image dictionary declares.
    ///
    /// ISO 32000 expects `/Width` and `/Height` to match the codestream. A file
    /// where they do not still renders, with the overlapping region kept and
    /// anything beyond it left blank, rather than shearing every row.
    fn fit(
        &mut self,
        source_width: usize,
        source_height: usize,
        width: usize,
        height: usize,
    ) -> Result<(), PdfImageError> {
        if source_width == width && source_height == height {
            return Ok(());
        }
        let pixels = width
            .checked_mul(height)
            .ok_or(PdfImageError::JpxImageTooLarge {
                pixels: usize::MAX,
                channels: self.components,
            })?;
        let too_large = || PdfImageError::JpxImageTooLarge {
            pixels,
            channels: self.components,
        };
        let mut color = zeroed(pixels.checked_mul(self.components).ok_or_else(too_large)?)
            .ok_or_else(too_large)?;
        let rows = source_height.min(height);
        let columns = source_width.min(width);
        for row in 0..rows {
            let source = row
                .checked_mul(source_width)
                .and_then(|offset| offset.checked_mul(self.components))
                .ok_or_else(too_large)?;
            let destination = row
                .checked_mul(width)
                .and_then(|offset| offset.checked_mul(self.components))
                .ok_or_else(too_large)?;
            let span = columns.saturating_mul(self.components);
            let Some(source) = self.color.get(source..source.saturating_add(span)) else {
                continue;
            };
            let Some(destination) = color.get_mut(destination..destination.saturating_add(span))
            else {
                continue;
            };
            destination.copy_from_slice(source);
        }
        self.color = color;

        if let Some(source_alpha) = self.alpha.as_ref() {
            let mut alpha = zeroed(pixels).ok_or_else(too_large)?;
            for row in 0..rows {
                let source = row.checked_mul(source_width).ok_or_else(too_large)?;
                let destination = row.checked_mul(width).ok_or_else(too_large)?;
                let Some(source) = source_alpha.get(source..source.saturating_add(columns)) else {
                    continue;
                };
                let Some(destination) =
                    alpha.get_mut(destination..destination.saturating_add(columns))
                else {
                    continue;
                };
                destination.copy_from_slice(source);
            }
            self.alpha = Some(alpha);
        }
        Ok(())
    }
}

/// A conversion the embedded colour description asks for.
///
/// Only the descriptions [`EnumeratedColorSpace::is_pdf_renderable`] accepts
/// reach this point, and only when the image dictionary names no colour space
/// of its own.
#[derive(Clone, Copy, Debug)]
enum EmbeddedTransform {
    /// Zero means white, so the samples are the complement of grey.
    InvertGray,
    /// Luma and chroma that become sRGB through the inverse BT.601 matrix.
    YccToRgb,
    /// Subtractive colour without a black channel, which PDF has no space for.
    AddBlack,
}

impl EmbeddedTransform {
    /// Returns the conversion an enumerated space needs, if it needs one.
    fn of(space: EnumeratedColorSpace) -> Option<Self> {
        match space {
            EnumeratedColorSpace::BilevelWhiteZero => Some(Self::InvertGray),
            EnumeratedColorSpace::Sycc | EnumeratedColorSpace::ExtendedSycc => Some(Self::YccToRgb),
            EnumeratedColorSpace::Cmy => Some(Self::AddBlack),
            _ => None,
        }
    }

    /// Rewrites the colour samples into the PDF colour space they map to.
    fn apply(
        self,
        display: &mut JpxDisplay,
        source_components: usize,
    ) -> Result<(), PdfImageError> {
        match self {
            Self::InvertGray => {
                for sample in display.color.iter_mut() {
                    *sample = u8::MAX.saturating_sub(*sample);
                }
            }
            Self::YccToRgb => {
                for pixel in display.color.chunks_exact_mut(source_components.max(1)) {
                    let [luma, blue, red, ..] = pixel else {
                        continue;
                    };
                    let y = f32::from(*luma);
                    let cb = f32::from(*blue) - 128.0;
                    let cr = f32::from(*red) - 128.0;
                    *luma = clamp_to_byte(1.402f32.mul_add(cr, y));
                    *blue = clamp_to_byte(y - 0.344_136 * cb - 0.714_136 * cr);
                    *red = clamp_to_byte(1.772f32.mul_add(cb, y));
                }
            }
            Self::AddBlack => {
                let pixels = display
                    .color
                    .len()
                    .checked_div(source_components.max(1))
                    .unwrap_or(0);
                let mut color = zeroed(pixels.saturating_mul(display.components)).ok_or(
                    PdfImageError::JpxImageTooLarge {
                        pixels,
                        channels: display.components,
                    },
                )?;
                for (source, destination) in display
                    .color
                    .chunks_exact(source_components.max(1))
                    .zip(color.chunks_exact_mut(display.components.max(1)))
                {
                    if let Some(destination) = destination.get_mut(..source.len()) {
                        destination.copy_from_slice(source);
                    }
                }
                display.color = color;
            }
        }
        Ok(())
    }
}

/// Names the PDF colour space an image without a `/ColorSpace` entry uses.
///
/// The enumerated description decides when this crate can honour it; otherwise
/// the channel count does, as a renderer has nothing else to go on.
fn default_color_space(
    embedded: Option<EnumeratedColorSpace>,
    components: usize,
) -> Result<ColorSpace, PdfImageError> {
    let named = embedded
        .filter(|space| space.is_pdf_renderable())
        .and_then(|space| match space {
            EnumeratedColorSpace::Greyscale
            | EnumeratedColorSpace::BilevelBlackZero
            | EnumeratedColorSpace::BilevelWhiteZero => Some(ColorSpace::DeviceGray),
            EnumeratedColorSpace::Srgb
            | EnumeratedColorSpace::ExtendedSrgb
            | EnumeratedColorSpace::Sycc
            | EnumeratedColorSpace::ExtendedSycc => Some(ColorSpace::DeviceRGB),
            EnumeratedColorSpace::Cmy | EnumeratedColorSpace::Cmyk => Some(ColorSpace::DeviceCMYK),
            _ => None,
        });
    match named.filter(|space| space.num_color_components() == components) {
        Some(space) => Ok(space),
        None => match components {
            1 => Ok(ColorSpace::DeviceGray),
            3 => Ok(ColorSpace::DeviceRGB),
            4 => Ok(ColorSpace::DeviceCMYK),
            _ => Err(PdfImageError::InvalidColorComponentCount),
        },
    }
}

/// Rounds a converted colour value into the 8-bit display range.
fn clamp_to_byte(value: f32) -> u8 {
    let rounded = value.round();
    match rounded {
        value if value <= 0.0 => 0,
        value if value >= f32::from(u8::MAX) => u8::MAX,
        value => num_traits::cast(value).unwrap_or(0),
    }
}
