//! A ready-made sink that flattens an image into 8-bit channel samples.
//!
//! The decoder delivers each tile's components at their own precision, on
//! their own sampled grid, and possibly behind a palette. Consumers that only
//! need 8-bit output, such as a PDF renderer, can collect the whole image here
//! instead: every output channel is resolved through the palette, scaled to
//! eight bits, and spread over the reference-grid area each sample covers,
//! including the enlargement a decode with discarded resolution levels needs.

use crate::{
    Jpeg2000Error, SampleData, TileSink, TileView,
    codestream::SizeHeader,
    container::{ChannelKind, ChannelSource, ImageChannels, OutputChannel},
};

/// An image's output channels collected as 8-bit samples on the image area.
///
/// Colour channels are interleaved in channel order. The first opacity channel
/// fills a plane of its own, so a consumer can decide whether to use it; any
/// further opacity channel is ignored.
pub struct ByteImage<'a> {
    channels: ImageChannels<'a>,
    targets: Vec<Target>,
    origin_x: u32,
    origin_y: u32,
    width: usize,
    height: usize,
    color_components: usize,
    color: Vec<u8>,
    opacity: Option<Vec<u8>>,
    premultiplied: bool,
    /// One converted row of the channel being written, reused across rows.
    row: Vec<u8>,
}

impl<'a> ByteImage<'a> {
    /// Allocates the planes the resolved channel list calls for.
    ///
    /// The planes are owned by the sink, not charged against the decoder's
    /// working-memory bound.
    ///
    /// # Errors
    ///
    /// Returns a structural error for a channel that names a missing
    /// component, and `Overflow` when the planes cannot be allocated.
    pub fn new(size: SizeHeader<'a>, channels: ImageChannels<'a>) -> Result<Self, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "8-bit image buffer",
        };
        let extent = size.image_size();
        let origin = size.image_origin();
        let width = usize::try_from(extent.width).map_err(|_| overflow())?;
        let height = usize::try_from(extent.height).map_err(|_| overflow())?;
        let pixels = width.checked_mul(height).ok_or_else(overflow)?;

        let mut targets = Vec::new();
        let mut color_components = 0usize;
        let mut premultiplied = false;
        let mut has_opacity = false;
        for channel in channels.iter() {
            let channel = channel?;
            let placement = if channel.is_opacity() {
                if has_opacity {
                    Placement::Ignored
                } else {
                    has_opacity = true;
                    premultiplied = matches!(channel.kind, ChannelKind::PremultipliedOpacity);
                    Placement::Opacity
                }
            } else {
                let slot = color_components;
                color_components = color_components.saturating_add(1);
                Placement::Color { slot }
            };
            let component = size.component(channel.source.component())?;
            targets.push(Target {
                channel,
                placement,
                x_subsampling: u32::from(component.x_subsampling),
                y_subsampling: u32::from(component.y_subsampling),
            });
        }

        let color_bytes = pixels.checked_mul(color_components).ok_or_else(overflow)?;
        let color = zeroed(color_bytes).ok_or_else(overflow)?;
        let opacity = match has_opacity {
            true => Some(zeroed(pixels).ok_or_else(overflow)?),
            false => None,
        };

        Ok(Self {
            channels,
            targets,
            origin_x: origin.x,
            origin_y: origin.y,
            width,
            height,
            color_components,
            color,
            opacity,
            premultiplied,
            row: Vec::new(),
        })
    }

    /// Returns the image width on the reference grid.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns the image height on the reference grid.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Returns the number of interleaved colour channels.
    pub fn color_components(&self) -> usize {
        self.color_components
    }

    /// Returns whether the colour samples were multiplied by the opacity.
    pub fn premultiplied(&self) -> bool {
        self.premultiplied
    }

    /// Returns the interleaved colour samples, `width * height *
    /// color_components` long, and the opacity plane when a channel carries
    /// opacity.
    pub fn into_planes(self) -> (Vec<u8>, Option<Vec<u8>>) {
        (self.color, self.opacity)
    }

    /// Writes one channel of one tile into its destination plane.
    fn write_channel(&mut self, tile: TileView<'_>, index: usize) -> Result<(), Jpeg2000Error> {
        let Some(target) = self.targets.get(index).copied() else {
            return Ok(());
        };
        if matches!(target.placement, Placement::Ignored) {
            return Ok(());
        }
        let component = target.channel.source.component();
        let Some(plane) = tile
            .components()
            .iter()
            .find(|plane| plane.component_index() == component)
        else {
            return Err(Jpeg2000Error::Output {
                message: format!("tile is missing component {component}"),
            });
        };

        // A sample covers its subsampling block of the reference grid, and
        // twice that along each axis for every resolution level discarded.
        let overflow = || Jpeg2000Error::Overflow {
            context: "component sample footprint",
        };
        let discarded = u32::from(plane.discarded_levels());
        let footprint = Footprint {
            width: target
                .x_subsampling
                .checked_shl(discarded)
                .ok_or_else(overflow)?,
            height: target
                .y_subsampling
                .checked_shl(discarded)
                .ok_or_else(overflow)?,
        };
        let bounds = plane.bounds();
        let stride = plane.row_stride();
        let width = usize::try_from(bounds.width).map_err(|_| overflow())?;
        let scale = ByteScale::new(target.channel.precision, target.channel.signed);
        let mut bytes = core::mem::take(&mut self.row);
        bytes.clear();
        bytes.resize(width, 0);
        let mut result = Ok(());
        for row in 0..bounds.height {
            let start = usize::try_from(row)
                .ok()
                .and_then(|row| row.checked_mul(stride))
                .ok_or_else(overflow);
            result = start.and_then(|start| {
                self.convert_row(target.channel, plane.samples(), start, &mut bytes, scale)
            });
            if result.is_err() {
                break;
            }
            self.place_row(
                target.placement,
                footprint,
                bounds.x,
                bounds.y.saturating_add(row),
                &bytes,
            );
        }
        self.row = bytes;
        result
    }

    /// Converts one row of a component plane to eight-bit channel samples.
    ///
    /// A sample width this crate does not know is treated as missing, so a
    /// future variant fails the decode rather than producing wrong pixels.
    fn convert_row(
        &self,
        channel: OutputChannel,
        samples: SampleData<'_>,
        start: usize,
        bytes: &mut [u8],
        scale: ByteScale,
    ) -> Result<(), Jpeg2000Error> {
        let truncated = || Jpeg2000Error::Truncated {
            offset: start,
            context: "component plane samples",
        };
        let range = start..start.checked_add(bytes.len()).ok_or_else(truncated)?;
        let indexed = matches!(channel.source, ChannelSource::Palette { .. });
        let convert = |byte: &mut u8, value: i64| -> Result<(), Jpeg2000Error> {
            let value = match indexed {
                true => self.channels.sample(channel, value)?,
                false => value,
            };
            *byte = scale.byte(value);
            Ok(())
        };
        match samples {
            SampleData::I32(values) => {
                let values = values.get(range).ok_or_else(truncated)?;
                for (byte, value) in bytes.iter_mut().zip(values) {
                    convert(byte, i64::from(*value))?;
                }
            }
            SampleData::I64(values) => {
                let values = values.get(range).ok_or_else(truncated)?;
                for (byte, value) in bytes.iter_mut().zip(values) {
                    convert(byte, *value)?;
                }
            }
        }
        Ok(())
    }

    /// Copies one row of channel samples across the reference-grid area each
    /// sample covers, clipped to the image.
    fn place_row(
        &mut self,
        placement: Placement,
        footprint: Footprint,
        column: u32,
        row: u32,
        bytes: &[u8],
    ) {
        let (plane, pixel_stride, slot) = match placement {
            Placement::Color { slot } => (&mut self.color, self.color_components, slot),
            Placement::Opacity => match self.opacity.as_mut() {
                Some(plane) => (plane, 1, 0),
                None => return,
            },
            Placement::Ignored => return,
        };
        let line_length = self.width.saturating_mul(pixel_stride);
        let top = row.saturating_mul(footprint.height);
        let left = column.saturating_mul(footprint.width);
        for grid_y in 0..footprint.height {
            let Some(y) = top
                .checked_add(grid_y)
                .and_then(|y| y.checked_sub(self.origin_y))
                .and_then(|y| usize::try_from(y).ok())
                .filter(|y| *y < self.height)
            else {
                continue;
            };
            let Some(line) = y
                .checked_mul(line_length)
                .and_then(|start| plane.get_mut(start..start.checked_add(line_length)?))
            else {
                continue;
            };
            spread_line(
                line,
                pixel_stride,
                slot,
                left,
                self.origin_x,
                footprint.width,
                bytes,
            );
        }
    }
}

impl TileSink for ByteImage<'_> {
    fn write_tile(&mut self, tile: TileView<'_>) -> Result<(), Jpeg2000Error> {
        for index in 0..self.targets.len() {
            self.write_channel(tile, index)?;
        }
        Ok(())
    }
}

/// Where one output channel's samples belong in the flattened image.
#[derive(Clone, Copy, Debug)]
enum Placement {
    /// The channel fills one slot of every interleaved colour pixel.
    Color {
        /// Slot within a pixel, below the colour channel count.
        slot: usize,
    },
    /// The channel fills the opacity plane.
    Opacity,
    /// A further opacity channel, which has no plane of its own.
    Ignored,
}

/// One output channel together with the destination of its samples.
#[derive(Clone, Copy, Debug)]
struct Target {
    channel: OutputChannel,
    placement: Placement,
    x_subsampling: u32,
    y_subsampling: u32,
}

/// The reference-grid area one component sample covers.
#[derive(Clone, Copy, Debug)]
struct Footprint {
    width: u32,
    height: u32,
}

/// Writes one row of samples into one image line, `footprint` pixels each.
///
/// `left` is the reference-grid column of the first sample and `origin` the
/// image's first column. Each pixel of the line holds `pixel_stride` bytes, of
/// which the sample fills `slot`.
fn spread_line(
    line: &mut [u8],
    pixel_stride: usize,
    slot: usize,
    left: u32,
    origin: u32,
    footprint: u32,
    bytes: &[u8],
) {
    let Ok(footprint) = usize::try_from(footprint) else {
        return;
    };
    // Columns left of the image are cut off, which can drop whole samples
    // and leave the first remaining one only part of its footprint.
    let (clipped, first) = match origin.checked_sub(left) {
        Some(clipped) => (usize::try_from(clipped).unwrap_or(usize::MAX), 0),
        None => (
            0,
            usize::try_from(left.saturating_sub(origin)).unwrap_or(usize::MAX),
        ),
    };
    let Some(mut destination) = first
        .checked_mul(pixel_stride)
        .and_then(|start| line.get_mut(start..))
    else {
        return;
    };
    let mut samples = bytes
        .iter()
        .skip(clipped.checked_div(footprint).unwrap_or(usize::MAX));
    let partial = clipped.checked_rem(footprint).unwrap_or(0);
    if partial > 0
        && let Some(byte) = samples.next()
    {
        let lead = footprint
            .saturating_sub(partial)
            .saturating_mul(pixel_stride)
            .min(destination.len());
        let (head, tail) = destination.split_at_mut(lead);
        fill_slot(head, pixel_stride, slot, *byte);
        destination = tail;
    }
    let block = footprint.saturating_mul(pixel_stride);
    for (run, byte) in destination.chunks_mut(block.max(1)).zip(samples) {
        fill_slot(run, pixel_stride, slot, *byte);
    }
}

/// Sets one slot of every pixel in a run of interleaved pixels.
fn fill_slot(run: &mut [u8], pixel_stride: usize, slot: usize, byte: u8) {
    for pixel in run.chunks_mut(pixel_stride.max(1)) {
        if let Some(value) = pixel.get_mut(slot) {
            *value = byte;
        }
    }
}

/// Scales channel samples of one precision to eight bits.
///
/// A signed channel is level-shifted into the unsigned range first, as Part 1
/// defines its zero point. Scaling rounds to nearest so a full-scale sample
/// stays full-scale at any precision.
#[derive(Clone, Copy, Debug)]
struct ByteScale {
    /// Added to a sample to move a signed channel's zero point.
    offset: i64,
    /// Full scale of the channel.
    max: i64,
}

impl ByteScale {
    /// Describes the scaling of a channel with the given precision.
    fn new(precision: u8, signed: bool) -> Self {
        let precision = precision.clamp(1, 38);
        Self {
            offset: match signed {
                true => 1i64 << precision.saturating_sub(1),
                false => 0,
            },
            max: (1i64 << precision).saturating_sub(1),
        }
    }

    /// Scales one sample to eight bits.
    fn byte(self, value: i64) -> u8 {
        let clamped = value.saturating_add(self.offset).clamp(0, self.max);
        if self.max == i64::from(u8::MAX) {
            return u8::try_from(clamped).unwrap_or(u8::MAX);
        }
        let scaled = clamped
            .saturating_mul(i64::from(u8::MAX))
            .saturating_add(self.max / 2)
            .checked_div(self.max)
            .unwrap_or(0);
        u8::try_from(scaled).unwrap_or(u8::MAX)
    }
}

/// Allocates a zero-filled buffer without aborting on a failed reservation.
fn zeroed(len: usize) -> Option<Vec<u8>> {
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(len).ok()?;
    buffer.resize(len, 0);
    Some(buffer)
}
